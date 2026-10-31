//! Bounded reads, staged writes and per-run write reservations.
use super::tool_access::{validate_tool_path, ToolAccess};
use super::{
    MAX_IMAGE_SIZE, MAX_PDF_SIZE, MAX_READ_SIZE, MAX_TOOL_PDF_SIZE, MAX_WRITES_PER_RUN,
    MAX_WRITE_BYTES_PER_RUN, MAX_WRITE_SIZE,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::io::Write as _;
use std::path::PathBuf;

/// Write calls and bytes consumed by one run, counted in its run scope and
/// reset when the run workspace is created.
struct WriteReservation {
    scope: std::sync::Arc<crate::run_scope::RunScope>,
    bytes: usize,
    committed: bool,
}

impl WriteReservation {
    fn reserve(bytes: usize) -> Result<Self, String> {
        let scope = crate::run_scope::current();
        scope
            .write_count
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |used| (used < MAX_WRITES_PER_RUN).then_some(used + 1),
            )
            .map_err(|_| format!("Write limit reached ({MAX_WRITES_PER_RUN} files per run)"))?;
        if scope
            .write_bytes
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |used| {
                    used.checked_add(bytes)
                        .filter(|next| *next <= MAX_WRITE_BYTES_PER_RUN)
                },
            )
            .is_err()
        {
            let _ = scope.write_count.fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |used| Some(used.saturating_sub(1)),
            );
            return Err(format!(
                "Run artifact quota reached ({} MB)",
                MAX_WRITE_BYTES_PER_RUN / 1024 / 1024
            ));
        }
        Ok(Self {
            scope,
            bytes,
            committed: false,
        })
    }

    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for WriteReservation {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.scope.write_count.fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |used| Some(used.saturating_sub(1)),
            );
            let _ = self.scope.write_bytes.fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |used| Some(used.saturating_sub(self.bytes)),
            );
        }
    }
}

/// Reset the run-wide aggregate write quota. Filesystem authority itself is
/// per call in [`ToolAccess`].
pub fn reset_write_budget() {
    crate::run_scope::current().reset_write_budget();
}

/// Validate and perform a Write tool call. The destination must resolve
/// inside the configured write dir; `..` components, absolute paths outside
/// the dir, and symlinked destinations are rejected. Parent subdirectories
/// are created as needed.
pub fn write_file_for_tool(
    access: &ToolAccess,
    path: &str,
    content: &str,
) -> Result<String, String> {
    let raw_root = access
        .write_root
        .clone()
        .ok_or("File writing is not enabled for this step")?;
    // The run dir is created before any LLM call, so this canonicalizes.
    let root = raw_root
        .canonicalize()
        .map_err(|e| format!("Artifact directory unavailable: {e}"))?;

    if content.len() > MAX_WRITE_SIZE {
        return Err(format!(
            "Content too large ({} bytes, max {MAX_WRITE_SIZE})",
            content.len()
        ));
    }
    // Resolve to a path relative to the artifact root. Absolute paths must
    // already point inside it (accepting both the raw and canonical spelling
    // of the root, since models echo back whichever form the prompt used).
    let p = std::path::Path::new(path);
    let rel = if p.is_absolute() {
        p.strip_prefix(&root)
            .or_else(|_| p.strip_prefix(&raw_root))
            .map(|r| r.to_path_buf())
            .map_err(|_| format!("Access denied: {path} is outside the artifact directory"))?
    } else {
        p.to_path_buf()
    };

    // No traversal components, no re-rooting, and a sane depth.
    if rel.as_os_str().is_empty() {
        return Err("Destination path is empty".into());
    }
    if !rel
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err(format!("Access denied: {path} contains path traversal"));
    }
    if rel.components().count() > 8 {
        return Err("Destination path too deep".into());
    }

    let dest = root.join(&rel);
    // Refuse to write through a pre-existing symlink (e.g. planted by an
    // earlier shell command in a CLI-mode step of the same run).
    if let Ok(meta) = std::fs::symlink_metadata(&dest) {
        if meta.file_type().is_symlink() {
            return Err(format!("Access denied: {path} is a symlink"));
        }
        if !meta.is_file() {
            return Err(format!("Access denied: {path} is not a regular file"));
        }
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory for {path}: {e}"))?;
        // A symlinked intermediate directory could redirect the write even
        // though every component is Normal; canonicalize and re-check.
        let canon_parent = parent
            .canonicalize()
            .map_err(|e| format!("Cannot resolve directory for {path}: {e}"))?;
        if !canon_parent.starts_with(&root) {
            return Err(format!(
                "Access denied: {path} resolves outside the artifact directory"
            ));
        }
    }
    let parent = dest.parent().ok_or("Destination has no parent directory")?;
    let mut staged = tempfile::Builder::new()
        .prefix(".pipeline-write-")
        .tempfile_in(parent)
        .map_err(|e| format!("Failed to stage {path}: {e}"))?;
    staged
        .write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write {path}: {e}"))?;
    staged
        .as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync {path}: {e}"))?;

    // Reserve only once validation and the fallible data write have
    // succeeded. The reservation rolls back if the atomic commit fails.
    let reservation = WriteReservation::reserve(content.len())?;
    staged
        .into_temp_path()
        .persist(&dest)
        .map_err(|e| format!("Failed to commit {path}: {}", e.error))?;
    reservation.commit();

    Ok(format!(
        "Wrote {} bytes to {}",
        content.len(),
        dest.to_string_lossy().replace('\\', "/")
    ))
}

pub(super) fn read_bytes_limited(path: &std::path::Path, limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    // Symlink/FIFO-safe open; on Windows it also reopens non-symlink file
    // reparse points (OneDrive-style cloud placeholders) so they hydrate.
    let file = crate::safety::open_regular_file(path)?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    if bytes.len() > limit {
        return Err(format!(
            "File too large (more than {limit} bytes): {}",
            path.display()
        ));
    }
    Ok(bytes)
}

pub(super) fn read_file_for_tool_limited(
    access: &ToolAccess,
    path: &str,
    limit: usize,
) -> Result<String, String> {
    let limit = MAX_READ_SIZE.min(limit);
    let canonical = validate_tool_path(access, path, limit)?;

    if path.to_lowercase().ends_with(".pdf") {
        return Err("Cannot read PDF as text. Use the extracted paper text instead.".into());
    }

    String::from_utf8(read_bytes_limited(&canonical, limit)?)
        .map_err(|e| format!("Failed to read {path} as UTF-8: {e}"))
}

#[derive(Debug)]
pub(super) struct TextRange {
    pub(super) content: String,
    pub(super) start_offset: usize,
    pub(super) end_offset: usize,
    pub(super) next_offset: Option<usize>,
}

pub(super) fn slice_text_range(
    text: &str,
    start_offset: usize,
    requested_end_offset: usize,
    max_bytes: usize,
) -> Result<TextRange, String> {
    if start_offset > text.len() {
        return Err(format!(
            "Offset {start_offset} is past the end of the {}-byte file",
            text.len()
        ));
    }
    if requested_end_offset < start_offset || requested_end_offset > text.len() {
        return Err("Requested text range is outside the file".to_string());
    }
    if !text.is_char_boundary(start_offset) {
        return Err(format!(
            "Offset {start_offset} is not a UTF-8 character boundary; use next_offset from a prior result"
        ));
    }
    let mut end = start_offset
        .saturating_add(max_bytes)
        .min(requested_end_offset);
    while end > start_offset && !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == start_offset && start_offset < requested_end_offset {
        return Err(
            "max_bytes is too small to contain the next UTF-8 character at this offset".to_string(),
        );
    }
    Ok(TextRange {
        content: text[start_offset..end].to_string(),
        start_offset,
        end_offset: end,
        next_offset: (end < requested_end_offset).then_some(end),
    })
}

pub(super) fn text_line_bounds(
    text: &str,
    requested_start_line: Option<usize>,
    requested_end_line: Option<usize>,
) -> Result<(usize, usize), String> {
    let start_line = requested_start_line.unwrap_or(1);
    let mut starts = Vec::with_capacity(text.len() / 64 + 1);
    if !text.is_empty() {
        starts.push(0);
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' && index + 1 < text.len() {
                starts.push(index + 1);
            }
        }
    }
    if start_line == 0 || start_line > starts.len() {
        return Err(format!(
            "start_line {start_line} is outside the file's 1..={} line range",
            starts.len()
        ));
    }
    if let Some(end_line) = requested_end_line {
        if end_line < start_line {
            return Err(format!(
                "end_line {end_line} must be greater than or equal to start_line {start_line}"
            ));
        }
        if end_line > starts.len() {
            return Err(format!(
                "end_line {end_line} is outside the file's 1..={} line range",
                starts.len()
            ));
        }
    }
    let start_offset = starts[start_line - 1];
    let end_offset = requested_end_line
        .filter(|end_line| *end_line < starts.len())
        .map(|end_line| starts[end_line])
        .unwrap_or(text.len());
    Ok((start_offset, end_offset))
}

/// Read a PDF file and return its contents as base64-encoded bytes.
pub(super) fn read_pdf_for_tool(
    access: &ToolAccess,
    path: &str,
    limit: usize,
) -> Result<String, String> {
    let limit = MAX_TOOL_PDF_SIZE.min(limit);
    let canonical = validate_tool_path(access, path, limit)?;
    let bytes = read_bytes_limited(&canonical, limit)?;
    Ok(STANDARD.encode(&bytes))
}

pub(super) fn read_image_for_tool(
    access: &ToolAccess,
    path: &str,
    limit: usize,
) -> Result<(String, String, usize), String> {
    let extension = PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let media_type = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => {
            return Err("Unsupported document image type; use PNG, JPEG, GIF, or WebP".to_string())
        }
    };
    let limit = MAX_IMAGE_SIZE.min(limit);
    let canonical = validate_tool_path(access, path, limit)?;
    let bytes = read_bytes_limited(&canonical, limit)?;
    let raw_bytes = bytes.len();
    Ok((STANDARD.encode(bytes), media_type.to_string(), raw_bytes))
}

/// Read a PDF the app itself is attaching to a request (as opposed to one the
/// model asked for via the Read tool — that path goes through the allowed-dir
/// sandbox above). Only a size cap applies: the path comes from our own
/// extraction code, not from model output.
pub fn pdf_attachment_base64(path: &std::path::Path, max_size: usize) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("Cannot read PDF metadata: {e}"))?;
    if metadata.len() as usize > max_size {
        return Err(format!(
            "PDF too large to attach ({} MB, limit {} MB). Use a CLI provider or a native extraction method.",
            metadata.len() / 1_000_000,
            max_size / 1_000_000
        ));
    }
    let bytes = read_bytes_limited(path, max_size)?;
    Ok(STANDARD.encode(&bytes))
}

/// Attachment size caps. Anthropic and OpenAI accept requests up to 32 MB;
/// Google's inline-data path caps the whole request at 20 MB, and base64
/// inflates by 4/3, so the raw PDF must stay under ~14 MB there.
pub const MAX_ATTACH_PDF: usize = MAX_PDF_SIZE;
pub const MAX_ATTACH_PDF_GOOGLE: usize = 14 * 1024 * 1024;

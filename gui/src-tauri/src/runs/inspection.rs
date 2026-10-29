//! Bounded file inspection, classification and presentation labels.
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

fn title_words(value: &str) -> String {
    let words = value
        .split(['-', '_'])
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let mut text = words.join(" ");
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    text
}

pub(super) fn agent_response_label(rel_path: &str) -> String {
    let stem = Path::new(rel_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(rel_path);
    let Some((producer_with_hash, attempt_and_status)) = stem.rsplit_once("--attempt-") else {
        return title_words(stem);
    };
    let producer = producer_with_hash
        .rsplit_once("--")
        .map(|(readable, _hash)| readable)
        .unwrap_or(producer_with_hash);
    let (attempt, remainder) = attempt_and_status
        .split_once('-')
        .unwrap_or((attempt_and_status, "response-captured"));
    let statuses = [
        ("rejected-envelope", "Rejected boundaries"),
        ("rejected-content", "Rejected content"),
        ("rejected-schema", "Rejected schema"),
        ("accepted", "Accepted"),
        ("ignored", "Not selected"),
        ("captured", "Captured before validation"),
    ];
    let (source, status) = statuses
        .iter()
        .find_map(|(suffix, label)| {
            remainder
                .strip_suffix(&format!("-{suffix}"))
                .map(|source| (source, *label))
        })
        .unwrap_or((remainder, "Captured response"));
    let attempt_display = attempt.trim_start_matches('0');
    let attempt_display = if attempt_display.is_empty() {
        "0"
    } else {
        attempt_display
    };
    format!(
        "{} · Attempt {} · {} · {}",
        title_words(producer),
        attempt_display,
        title_words(source),
        status,
    )
}

/// Detect an artifact's kind from its extension, falling back to a NUL-byte
/// sniff for extensionless files. Deliberately coarse: the viewer only needs
/// to pick a renderer, not identify the language precisely.
pub fn detect_kind(rel_path: &str, head: &[u8]) -> &'static str {
    let ext = Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "md" | "markdown" | "rmd" | "qmd" => "markdown",
        "json" => "json",
        "csv" | "tsv" => "csv",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => "image",
        "ado" | "mata" | "sty" | "cls" | "pyi" | "mjs" | "cjs" | "zsh" | "bash" | "ini" | "cfg"
        | "xml" | "jsonl" | "rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "r" | "do" | "jl" | "c"
        | "cc" | "cpp" | "h" | "hpp" | "java" | "go" | "rb" | "sh" | "sql" | "tex" | "bib"
        | "toml" | "yaml" | "yml" | "html" | "css" | "m" | "f90" | "sas" | "stan" => "code",
        "txt" | "log" => "text",
        "pdf" => "pdf",
        "zip" | "gz" | "xlsx" | "docx" | "pptx" | "dta" | "rds" | "parquet" => "binary",
        // A model-written program must never be one click away from running
        // with the researcher's privileges; the viewer reveals, never opens.
        "exe" | "msi" | "bat" | "cmd" | "ps1" | "scr" | "com" | "jar" | "pkg" | "dmg" | "app"
        | "command" | "appimage" | "deb" | "rpm" => "executable",
        _ => {
            if head.contains(&0) {
                "binary"
            } else {
                "text"
            }
        }
    }
}

pub(super) fn short_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)[..16].to_string()
}

pub(super) fn inspect_file(path: &Path) -> Result<(Vec<u8>, u64, String), String> {
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut head = Vec::with_capacity(512);
    let mut total = 0u64;
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut chunk)
            .map_err(|e| format!("Failed to read file: {e}"))?;
        if count == 0 {
            break;
        }
        if head.len() < 512 {
            let take = count.min(512 - head.len());
            head.extend_from_slice(&chunk[..take]);
        }
        total += count as u64;
        hasher.update(&chunk[..count]);
    }
    Ok((
        head,
        total,
        hex::encode(hasher.finalize())[..16].to_string(),
    ))
}

pub(super) fn read_at_most(path: &Path, limit: usize) -> Result<(Vec<u8>, bool), String> {
    use std::io::Read as _;
    let file =
        crate::safety::open_regular_file(path).map_err(|e| format!("Cannot open artifact: {e}"))?;
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024) + 1);
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read artifact: {e}"))?;
    let truncated = bytes.len() > limit;
    if truncated {
        bytes.truncate(limit);
    }
    Ok((bytes, truncated))
}

/// Flush an already-written run file to disk, plus a best-effort sync of its
/// parent directory (matching `write_manifest`). Finalization deletes the step
/// checkpoints only after `report.json`/`report.md` are durable, so a power
/// loss in the writeback window cannot lose both copies of the step outputs.
pub(crate) fn sync_run_file(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| format!("Failed to sync {}: {e}", path.display()))?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        let _ = fs::File::open(parent).and_then(|directory| directory.sync_all());
    }
    Ok(())
}

pub(super) fn read_utf8_at_most(path: &Path, limit: usize, label: &str) -> Result<String, String> {
    let (bytes, oversized) = read_at_most(path, limit)?;
    if oversized {
        return Err(format!("{label} exceeds the {limit} byte safety limit"));
    }
    String::from_utf8(bytes).map_err(|e| format!("{label} is not valid UTF-8: {e}"))
}

//! Read-only, scope-bound viewer bytes. Writes remain in Studio.
use super::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileReadRequest {
    pub workspace_id: String,
    pub path: String,
    pub checkpoint_id: Option<String>,
    pub revision_id: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub path: String,
    pub hash: String,
    pub bytes: usize,
    pub text: Option<String>,
    pub base64: Option<String>,
    pub mime: String,
    pub editable: bool,
    pub truncated: bool,
    pub external_path: Option<String>,
    /// Authorized live/checkpoint root for resolving links. Captures have none.
    pub scope_root: Option<String>,
    /// Which viewer applies: text, pdf, image, tiff, audio, video, html,
    /// notebook, table, docx or binary.
    pub kind: String,
    /// Opens a `pipeline-file` URL. Viewers fall back to `base64` without one.
    pub stream_token: Option<String>,
    pub range_capture: bool,
    /// Compared with `workbench_file_stat` to notice changes on disk.
    pub stamp: Option<String>,
    pub modified_ms: Option<i64>,
    /// The file is not UTF-8, so `text` is a read-only approximation.
    pub approximate_text: bool,
    /// How many of the file's bytes `text` covers, where Load more carries on.
    /// Decoded text can't say: a replaced byte takes up three in `text`.
    pub text_bytes: Option<usize>,
}
pub(super) fn editable_source(path: &str) -> bool {
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        name.as_str(),
        ".gitignore"
            | "authors"
            | "changelog"
            | "dockerfile"
            | "license"
            | "makefile"
            | "notes"
            | "readme"
            | "snakefile"
            | "todo"
    ) {
        return true;
    }
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "tex"
            | "bib"
            | "sty"
            | "cls"
            | "md"
            | "markdown"
            | "txt"
            | "py"
            | "pyi"
            | "r"
            | "rmd"
            | "qmd"
            | "jl"
            | "do"
            | "ado"
            | "mata"
            | "m"
            | "sql"
            | "sh"
            | "bash"
            | "zsh"
            | "json"
            | "jsonl"
            | "yaml"
            | "yml"
            | "toml"
            | "ini"
            | "cfg"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "ts"
            | "tsx"
            | "rs"
            | "c"
            | "h"
            | "cpp"
            | "cc"
            | "hpp"
            | "java"
            | "go"
            | "rb"
            | "css"
            | "scss"
            | "html"
            | "htm"
            | "ltx"
            | "xml"
            | "svg"
            | "csv"
            | "tsv"
            | "log"
            | "sas"
            | "stan"
            | "f90"
            | "for"
            | "lua"
            | "dot"
            | "mod"
            | "gms"
            | "nb"
            | "ipynb"
    )
}

pub fn read_workspace_file(
    store: &Store,
    request: FileReadRequest,
) -> WorkbenchResult<FilePreview> {
    read_workspace_source(store, request, false).map(|(preview, _)| preview)
}

/// Return the verified file handle only when a captured PDF will use the file scheme.
pub(super) fn read_workspace_source(
    store: &Store,
    request: FileReadRequest,
    stream_pdf: bool,
) -> WorkbenchResult<(FilePreview, Option<super::file_open::CapturedPdf>)> {
    scope(store, &request.workspace_id)?;
    if request.checkpoint_id.is_some() && request.revision_id.is_some() {
        return Err(WorkbenchError::invalid(
            "Choose a working copy or a captured revision",
        ));
    }
    let (bytes, path, external_path, scope_root, expected) = if let Some(revision_id) =
        &request.revision_id
    {
        let revision = documents::revision(store, &request.workspace_id, revision_id)?;
        let storage: String = store.connection()?.query_row(
            "SELECT storage_reference FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 ORDER BY created_at LIMIT 1",
            params![request.workspace_id, revision.content_hash], |row| row.get(0),
        ).optional().map_err(err)?.ok_or_else(|| WorkbenchError::invalid("The captured file is unavailable"))?;
        crate::workbench::store::require_available("This saved document", &storage)?;
        let storage = PathBuf::from(storage);
        if storage.parent() != Some(store.root_path().join("blobs").as_path()) {
            return Err(WorkbenchError::invalid(
                "Captured file is outside immutable storage",
            ));
        }
        let (display_path, expected, folder, source_path) = if revision.input_kind == "source_tree"
        {
            files::relative(&request.path)?;
            let entry = revision.dependency_manifest["files"]
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|entry| entry["path"].as_str() == Some(&request.path))
                })
                .ok_or_else(|| {
                    WorkbenchError::invalid("This link was not included in the captured revision")
                })?;
            let expected = entry["contentHash"]
                .as_str()
                .ok_or_else(|| WorkbenchError::invalid("Missing captured file identity"))?
                .to_string();
            (
                request.path.clone(),
                expected,
                storage,
                request.path.clone(),
            )
        } else {
            let name = Path::new(&revision.entrypoint)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("document");
            if !request.path.is_empty()
                && request.path != name
                && request.path != revision.entrypoint
            {
                return Err(WorkbenchError::invalid("This linked file was not captured with the document. Import its source folder to retain related files."));
            }
            let disk_name = storage
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| WorkbenchError::invalid("Invalid captured file"))?;
            let blobs = store.root_path().join("blobs");
            (
                name.to_string(),
                revision.content_hash.clone(),
                blobs,
                disk_name.to_string(),
            )
        };
        let (mut file, stat) = files::SafeRoot::open(&folder)?
            .open_read(&source_path)?
            .ok_or_else(|| WorkbenchError::invalid("The captured file is unavailable"))?;
        if stat.size > MAX_FILE {
            return Err(WorkbenchError::invalid(
                "Captured file exceeds its size limit",
            ));
        }
        let modified = file.metadata().and_then(|m| m.modified()).map_err(err)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(8192)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        if stream_pdf && super::file_open::classify(&display_path, &bytes).kind == "pdf" {
            let mut digest = Sha256::new();
            digest.update(&bytes);
            let more = std::io::copy(
                &mut Read::by_ref(&mut file).take(MAX_FILE + 1 - bytes.len() as u64),
                &mut digest,
            )
            .map_err(err)?;
            if more + bytes.len() as u64 != stat.size
                || format!("{:x}", digest.finalize()) != expected
            {
                return Err(WorkbenchError::conflict(
                    "Captured file content no longer matches its revision",
                ));
            }
            let preview = FilePreview {
                path: display_path,
                hash: expected.clone(),
                bytes: stat.size as usize,
                text: None,
                base64: None,
                mime: "application/pdf".into(),
                editable: false,
                truncated: false,
                external_path: None,
                scope_root: None,
                kind: "pdf".into(),
                stream_token: None,
                range_capture: false,
                stamp: None,
                modified_ms: None,
                approximate_text: false,
                text_bytes: None,
            };
            return Ok((
                preview,
                Some(super::file_open::CapturedPdf {
                    file,
                    modified,
                    size: stat.size,
                    hash: expected,
                    reads: std::sync::Mutex::new(()),
                }),
            ));
        }
        Read::by_ref(&mut file)
            .take(MAX_FILE + 1 - bytes.len() as u64)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        if bytes.len() as u64 != stat.size {
            return Err(WorkbenchError::conflict("Captured file size changed"));
        }
        (bytes, display_path, None, None, Some(expected))
    } else {
        files::relative(&request.path)?;
        let base = match &request.checkpoint_id {
            Some(cp) => session_task_root(store, &request.workspace_id, cp)?,
            None => root(store, &request.workspace_id)?,
        };
        let bytes = files::SafeRoot::open(&base)?.read(&request.path)?;
        let external = base.join(&request.path).to_string_lossy().into_owned();
        (
            bytes,
            request.path,
            Some(external),
            Some(base.to_string_lossy().into_owned()),
            None,
        )
    };
    let mut preview = preview_from_bytes(path, bytes, external_path, expected.is_some());
    if expected.is_some_and(|expected| expected != preview.hash) {
        return Err(WorkbenchError::conflict(
            "Captured file content no longer matches its revision",
        ));
    }
    preview.scope_root = scope_root;
    Ok((preview, None))
}

fn preview_from_bytes(
    path: String,
    bytes: Vec<u8>,
    external_path: Option<String>,
    captured: bool,
) -> FilePreview {
    let ext = Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "text/plain",
    };
    let kind = super::file_open::classify(&path, &bytes[..bytes.len().min(8192)]).kind;
    let visual = mime != "text/plain";
    let text = if visual && ext != "svg" {
        None
    } else {
        std::str::from_utf8(&bytes)
            .ok()
            .filter(|s| !s.contains('\0'))
    };
    let truncated = text.is_some_and(|s| s.len() > 2 * 1024 * 1024);
    let editable = files::REPLACEMENT
        && !captured
        && !truncated
        && text.is_some()
        && editable_source(&path)
        && !ignored(&path, &[]);
    let text = text.map(|s| {
        let mut end = s.len().min(2 * 1024 * 1024);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s[..end].to_owned()
    });
    FilePreview {
        scope_root: None,
        path,
        hash: hash(&bytes),
        bytes: bytes.len(),
        text,
        base64: visual.then(|| base64::engine::general_purpose::STANDARD.encode(&bytes)),
        mime: mime.into(),
        editable,
        truncated,
        external_path,
        kind: kind.into(),
        stream_token: None,
        range_capture: false,
        stamp: None,
        modified_ms: None,
        approximate_text: false,
        text_bytes: None,
    }
}

fn conversation_relative_path(root: &Path, value: &str) -> WorkbenchResult<String> {
    if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(WorkbenchError::invalid("Invalid conversation file path"));
    }
    let canonical_root = root.canonicalize().map_err(err)?;
    let supplied = Path::new(value);
    let relative = if supplied.is_absolute() {
        let name = supplied
            .file_name()
            .ok_or_else(|| WorkbenchError::invalid("Conversation link has no file name"))?;
        let parent = supplied
            .parent()
            .ok_or_else(|| WorkbenchError::invalid("Conversation link has no parent folder"))?
            .canonicalize()
            .map_err(err)?;
        parent
            .strip_prefix(&canonical_root)
            .map_err(|_| {
                WorkbenchError::invalid(
                    "Conversation links can open only files in this conversation's folder",
                )
            })?
            .join(name)
    } else {
        supplied.to_path_buf()
    };
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => {
                parts.push(part.to_str().ok_or_else(|| {
                    WorkbenchError::invalid("Conversation file path is not UTF-8")
                })?)
            }
            _ => return Err(WorkbenchError::invalid("Invalid conversation file path")),
        }
    }
    let relative = parts.join("/");
    files::relative(&relative)?;
    Ok(relative)
}

fn conversation_file_bytes(
    store: &Store,
    session_id: &str,
    value: &str,
) -> WorkbenchResult<(String, Vec<u8>)> {
    let root = store.runtime_root(session_id)?;
    let relative = conversation_relative_path(&root, value)?;
    let bytes = files::SafeRoot::open(&root)?.read(&relative)?;
    Ok((relative, bytes))
}

pub fn read_conversation_file(
    store: &Store,
    session_id: &str,
    value: &str,
) -> WorkbenchResult<FilePreview> {
    let (path, bytes) = conversation_file_bytes(store, session_id, value)?;
    Ok(preview_from_bytes(path, bytes, None, true))
}

/// Copy a validated conversation artifact into app-owned immutable storage for
/// a native open. The shell receives this snapshot, never a pathname that the
/// active model can replace after validation. A program is refused first, so no
/// copy of it is written.
pub fn snapshot_conversation_file(
    store: &Store,
    session_id: &str,
    value: &str,
) -> WorkbenchResult<PathBuf> {
    let root = store.runtime_root(session_id)?;
    let path = conversation_relative_path(&root, value)?;
    let folder = files::SafeRoot::open(&root)?;
    let program = folder.marked_as_program(&path)?;
    crate::file_viewer::check_openable(&path, program).map_err(WorkbenchError::invalid)?;
    let bytes = folder.read(&path)?;
    let digest = hash(&bytes);
    let suffix = Path::new(&path)
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= 16
                && extension
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    let directory = store.root_path().join("opened-conversation-files");
    fs::create_dir_all(&directory).map_err(err)?;
    #[cfg(unix)]
    crate::safety::restrict_permissions(&directory, 0o700).map_err(err)?;
    let target = directory.join(format!("{digest}{suffix}"));
    if target.exists() {
        verify_conversation_snapshot(&target, &bytes)?;
        return Ok(target);
    }
    crate::workbench::desk::write_blob_atomic(&target, &bytes, &digest).map_err(err)?;
    // Verify the final directory entry even when another writer got there first.
    // Never hand the shell a path based only on the bytes that were validated
    // before publication.
    verify_conversation_snapshot(&target, &bytes)?;
    #[cfg(unix)]
    {
        crate::safety::restrict_permissions(&target, 0o400).map_err(err)?;
        if let Ok(directory) = fs::File::open(&directory) {
            crate::safety::sync_landed(&directory);
        }
    }
    Ok(target)
}

/// The snapshot can sit on a volume that marks every file as a program, where a
/// name that passed on the conversation's own volume would run. It is judged by
/// its own name, which drops an extension the source had if that is unusual.
fn verify_conversation_snapshot(target: &Path, expected: &[u8]) -> WorkbenchResult<()> {
    let mut existing = crate::safety::open_regular_file(target).map_err(err)?;
    let name = target.to_string_lossy();
    if crate::file_viewer::marked_as_program(&name, &existing).map_err(err)? {
        return Err(WorkbenchError::invalid(
            "The drive holding Pipeline's research data would treat this file as a program. \
             Reveal its folder to open it yourself.",
        ));
    }
    let mut existing_bytes = Vec::new();
    Read::by_ref(&mut existing)
        .take(MAX_FILE + 1)
        .read_to_end(&mut existing_bytes)
        .map_err(err)?;
    if existing_bytes != expected {
        return Err(WorkbenchError::conflict(
            "An opened conversation snapshot failed its integrity check",
        ));
    }
    Ok(())
}

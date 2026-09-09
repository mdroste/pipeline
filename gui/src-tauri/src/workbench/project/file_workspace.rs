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
}
pub(super) fn editable_source(path: &str) -> bool {
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
            | "xml"
            | "svg"
            | "csv"
            | "tsv"
            | "log"
    )
}

pub fn read_workspace_file(
    store: &Store,
    request: FileReadRequest,
) -> WorkbenchResult<FilePreview> {
    scope(store, &request.workspace_id)?;
    if request.checkpoint_id.is_some() && request.revision_id.is_some() {
        return Err(WorkbenchError::invalid(
            "Choose a working copy or a captured revision",
        ));
    }
    let (bytes, path, external_path, captured) = if let Some(revision_id) = &request.revision_id {
        let revision = documents::revision(store, &request.workspace_id, revision_id)?;
        let storage: String = store.connection()?.query_row(
            "SELECT storage_reference FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 ORDER BY created_at LIMIT 1",
            params![request.workspace_id, revision.content_hash], |row| row.get(0),
        ).optional().map_err(err)?.ok_or_else(|| WorkbenchError::invalid("The captured file is unavailable"))?;
        let storage = PathBuf::from(storage);
        if storage.parent() != Some(store.root_path().join("blobs").as_path()) {
            return Err(WorkbenchError::invalid(
                "Captured file is outside immutable storage",
            ));
        }
        let (bytes, display_path, expected) = if revision.input_kind == "source_tree" {
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
                files::SafeRoot::open(&storage)?.read(&request.path)?,
                request.path.clone(),
                expected,
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
            (
                files::SafeRoot::open(&store.root_path().join("blobs"))?.read(disk_name)?,
                name.to_string(),
                revision.content_hash.clone(),
            )
        };
        if hash(&bytes) != expected {
            return Err(WorkbenchError::conflict(
                "Captured file content no longer matches its revision",
            ));
        }
        (bytes, display_path, None, true)
    } else {
        files::relative(&request.path)?;
        let base = match &request.checkpoint_id {
            Some(cp) => session_task_root(store, &request.workspace_id, cp)?,
            None => root(store, &request.workspace_id)?,
        };
        let bytes = files::SafeRoot::open(&base)?.read(&request.path)?;
        let external = base.join(&request.path).to_string_lossy().into_owned();
        (bytes, request.path, Some(external), false)
    };
    Ok(preview_from_bytes(path, bytes, external_path, captured))
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
    let visual = mime != "text/plain";
    let text = if visual && ext != "svg" {
        None
    } else {
        String::from_utf8(bytes.clone())
            .ok()
            .filter(|s| !s.contains('\0'))
    };
    let truncated = text.as_ref().is_some_and(|s| s.len() > 2 * 1024 * 1024);
    let editable =
        cfg!(unix) && !captured && !truncated && text.is_some() && editable_source(&path);
    let text = text.map(|mut s| {
        if truncated {
            let mut end = 2 * 1024 * 1024;
            while !s.is_char_boundary(end) {
                end -= 1;
            }
            s.truncate(end);
        }
        s
    });
    FilePreview {
        path,
        hash: hash(&bytes),
        bytes: bytes.len(),
        text,
        base64: visual.then(|| base64::engine::general_purpose::STANDARD.encode(&bytes)),
        mime: mime.into(),
        editable,
        truncated,
        external_path,
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
/// active model can replace after validation.
pub fn snapshot_conversation_file(
    store: &Store,
    session_id: &str,
    value: &str,
) -> WorkbenchResult<PathBuf> {
    let (path, bytes) = conversation_file_bytes(store, session_id, value)?;
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
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(err)?;
    }
    let target = directory.join(format!("{digest}{suffix}"));
    if target.exists() {
        verify_conversation_snapshot(&target, &bytes)?;
        return Ok(target);
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&directory).map_err(err)?;
    temporary.write_all(&bytes).map_err(err)?;
    temporary.flush().map_err(err)?;
    temporary.as_file().sync_all().map_err(err)?;
    match temporary.persist_noclobber(&target) {
        Ok(_) => {}
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(err(error.error)),
    }
    // Verify the final directory entry even when another writer won the
    // no-clobber race. Never hand the shell a path based only on the bytes that
    // were validated before publication.
    verify_conversation_snapshot(&target, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o400)).map_err(err)?;
        fs::File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(err)?;
    }
    Ok(target)
}

fn verify_conversation_snapshot(target: &Path, expected: &[u8]) -> WorkbenchResult<()> {
    let mut existing = crate::safety::open_regular_file(target).map_err(err)?;
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

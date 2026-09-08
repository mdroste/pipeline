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
    Ok(FilePreview {
        path,
        hash: hash(&bytes),
        bytes: bytes.len(),
        text,
        base64: visual.then(|| base64::engine::general_purpose::STANDARD.encode(&bytes)),
        mime: mime.into(),
        editable,
        truncated,
        external_path,
    })
}

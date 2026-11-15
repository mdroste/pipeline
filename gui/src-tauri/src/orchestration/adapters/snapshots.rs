//! Immutable bounded input snapshots and verified artifact paths.
use crate::orchestration::store::{err, hash, Result, Scope, Store};
use crate::workbench::tasks::delivery::DeliveryFile;
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_BYTES: u64 = 64 * 1024 * 1024;

fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && !(cfg!(windows) && name.contains(':'))
}

/// Why a snapshot failed, for callers that hold a model to what it asked for.
pub enum SnapshotFailure {
    /// What was asked for can't be captured: it is missing, unsuitable or too large.
    Input(String),
    /// Pipeline could not read or save it.
    Host(String),
}

impl From<SnapshotFailure> for String {
    fn from(failure: SnapshotFailure) -> Self {
        match failure {
            SnapshotFailure::Input(error) | SnapshotFailure::Host(error) => error,
        }
    }
}

pub fn snapshot(store: &Store, scope: &Scope, input: Value, filename: &str) -> Result<Value> {
    capture(store, scope, input, filename).map_err(String::from)
}

pub fn capture(
    store: &Store,
    scope: &Scope,
    input: Value,
    filename: &str,
) -> std::result::Result<Value, SnapshotFailure> {
    use SnapshotFailure::{Host, Input};
    if !plain_name(filename) {
        return Err(Input("Snapshot requires a plain filename".into()));
    }
    let bytes = if let Some(path) = input.get("path").and_then(Value::as_str) {
        let root = scope
            .runtime_root
            .as_ref()
            .ok_or_else(|| Host("File snapshots need a bound Workspace folder".into()))?;
        let identity =
            crate::workbench::store::root_identity(Path::new(root)).map_err(|e| Host(e.message))?;
        if scope.root_identity.as_deref() != Some(&identity) {
            return Err(Host("Task folder identity changed".into()));
        }
        let selected = Path::new(root).join(path);
        if !selected.exists() {
            return Err(Input(format!("Listed file {path} does not exist")));
        }
        let files = crate::workbench::project::task_capture(Path::new(root), &selected).map_err(
            |e| match e.code.as_str() {
                "invalid_input" => Input(e.message),
                _ => Host(e.message),
            },
        )?;
        if selected.is_dir() {
            if files.is_empty() {
                return Err(Input("Snapshot folder is empty".into()));
            }
            return snapshot_tree(store, files).map_err(Host);
        }
        files
            .into_iter()
            .next()
            .ok_or_else(|| Input("Snapshot is empty".into()))?
            .1
    } else if let Some(text) = input.as_str() {
        text.as_bytes().to_vec()
    } else {
        return Err(Input(
            "Snapshot input must be text or a scoped file path".into(),
        ));
    };
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(Input("Snapshot exceeds 64 MiB".into()));
    }
    use sha2::{Digest, Sha256};
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let dir = store.root.join("blobs").join(&digest);
    let path = dir.join(filename);
    crate::orchestration::durable::publish(&path, &bytes).map_err(Host)?;
    Ok(json!({"kind":"artifact","path":path,"hash":digest,"filename":filename,"bytes":bytes.len()}))
}

/// A locator is only a hint for old consumers. All reads resolve the digest in
/// the active library, including when the former library still exists.
enum Contents {
    File(Vec<u8>),
    Tree(Vec<(String, Vec<u8>)>),
}

fn verified(store: &Store, value: &Value) -> Result<(PathBuf, Contents)> {
    let digest = value["hash"].as_str().ok_or("Missing artifact digest")?;
    let name = value["filename"]
        .as_str()
        .ok_or("Missing artifact filename")?;
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) || !plain_name(name) {
        return Err("Invalid artifact identity".into());
    }
    let root = store.root.canonicalize().map_err(err)?.join("blobs");
    let path = match value["kind"].as_str() {
        Some("artifactTree") => root.join(format!("tree-{digest}")),
        Some("artifact") => root.join(digest).join(name),
        _ => return Err("Expected an immutable snapshot".into()),
    };
    // A legacy path may belong to an old OS or drive. Only the canonical
    // blob-store suffix identifies a reference, never an arbitrary host path.
    if let Some(hint) = value["path"].as_str() {
        let suffix = if value["kind"] == "artifactTree" {
            format!("/blobs/tree-{digest}")
        } else {
            format!("/blobs/{digest}/{name}")
        };
        if !hint.replace('\\', "/").ends_with(&suffix) {
            return Err("Invalid snapshot locator".into());
        }
    }
    // Equality also rejects a directory or file symlink, including a redirected blob root.
    if path.canonicalize().map_err(err)? != path {
        return Err("Snapshot resolves outside its recorded location".into());
    }
    if value["kind"] == "artifactTree" {
        let files = crate::workbench::project::task_capture(&path, &path).map_err(|e| e.message)?;
        if hash(&tree_manifest(&files))? != digest {
            return Err("Snapshot tree changed after capture".into());
        }
        return Ok((path, Contents::Tree(files)));
    }
    let mut bytes = Vec::new();
    crate::safety::open_regular_file(&path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > MAX_BYTES || crate::workbench::desk::hash(&bytes) != digest {
        return Err("Snapshot is missing or changed after capture".into());
    }
    Ok((path, Contents::File(bytes)))
}

pub fn artifact_path(store: &Store, value: &Value) -> Result<String> {
    Ok(verified(store, value)?.0.to_string_lossy().into_owned())
}

/// Package exactly the bytes verified above, including every member of a tree.
pub(in crate::orchestration) fn export_file(store: &Store, value: &Value) -> Result<DeliveryFile> {
    let (_, contents) = verified(store, value)?;
    let filename = value["filename"]
        .as_str()
        .ok_or("Missing artifact filename")?;
    match contents {
        Contents::File(bytes) => Ok(DeliveryFile {
            name: filename.into(),
            bytes,
        }),
        Contents::Tree(files) => {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            let options = zip::write::SimpleFileOptions::default()
                .last_modified_time(zip::DateTime::default());
            for (name, bytes) in files {
                zip.start_file(name, options).map_err(err)?;
                zip.write_all(&bytes).map_err(err)?;
            }
            Ok(DeliveryFile {
                name: format!("{filename}.zip"),
                bytes: zip.finish().map_err(err)?.into_inner(),
            })
        }
    }
}

fn tree_manifest(files: &[(String, Vec<u8>)]) -> Value {
    use sha2::{Digest, Sha256};
    json!(files.iter().map(|(path,bytes)|json!({"path":path,"hash":format!("{:x}",Sha256::digest(bytes)),"bytes":bytes.len()})).collect::<Vec<_>>())
}

fn snapshot_tree(store: &Store, files: Vec<(String, Vec<u8>)>) -> Result<Value> {
    let manifest = tree_manifest(&files);
    let digest = hash(&manifest)?;
    let root = store.root.join("blobs");
    crate::orchestration::durable::create_directories(&root)?;
    let destination = root.join(format!("tree-{digest}"));
    let result = json!({"kind":"artifactTree","path":destination,"hash":digest,"manifest":manifest,"filename":"paper-project","bytes":files.iter().map(|(_,b)|b.len()).sum::<usize>()});
    if !destination.exists() {
        let temp = tempfile::tempdir_in(&root).map_err(err)?;
        for (name, bytes) in &files {
            let path = temp.path().join(name);
            crate::orchestration::durable::publish(&path, bytes)?;
        }
        match crate::orchestration::durable::rename(temp.path(), &destination) {
            Ok(()) => return Ok(result),
            Err(e) if !destination.exists() => return Err(e),
            Err(_) => {}
        }
    }
    // A racing publisher or a previous capture owns the destination.
    verified(store, &result)?;
    crate::orchestration::durable::sync_directory(&root)?;
    Ok(result)
}

#[cfg(test)]
mod tree_tests {
    use super::*;
    #[test]
    fn concurrent_folder_capture_reuses_verified_tree() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let barrier = std::sync::Barrier::new(4);
        let results = std::thread::scope(|threads| {
            let handles = (0..4)
                .map(|_| {
                    threads.spawn(|| {
                        let files = (0..30)
                            .map(|index| (format!("{index:02}.txt"), vec![42; 65536]))
                            .collect();
                        barrier.wait();
                        snapshot_tree(&store, files).unwrap()
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert!(results.iter().all(|v| v == &results[0]));
        let path = artifact_path(&store, &results[0]).unwrap();
        std::fs::write(Path::new(&path).join("00.txt"), "changed").unwrap();
        assert!(snapshot_tree(
            &store,
            (0..30)
                .map(|index| (format!("{index:02}.txt"), vec![42; 65536]))
                .collect()
        )
        .is_err());
    }
}

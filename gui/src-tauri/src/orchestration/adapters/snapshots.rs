//! Immutable bounded input snapshots and verified artifact paths.
use crate::orchestration::store::{err, hash, Result, Scope, Store};
use serde_json::{json, Value};
use std::{io::Read, path::Path};

pub fn snapshot(store: &Store, scope: &Scope, input: Value, filename: &str) -> Result<Value> {
    if filename.is_empty() || filename.contains(['/', '\\']) || filename == "." || filename == ".."
    {
        return Err("Snapshot requires a plain filename".into());
    }
    let bytes = if let Some(path) = input.get("path").and_then(Value::as_str) {
        let root = scope
            .runtime_root
            .as_ref()
            .ok_or("File snapshots need a bound Workspace folder")?;
        if scope.root_identity.as_deref()
            != Some(
                &crate::workbench::store::root_identity(Path::new(root)).map_err(|e| e.message)?,
            )
        {
            return Err("Task folder identity changed".into());
        }
        let selected = Path::new(root).join(path);
        let files = crate::workbench::project::task_capture(Path::new(root), &selected)
            .map_err(|e| e.message)?;
        if selected.is_dir() {
            return snapshot_tree(store, files);
        }
        files.into_iter().next().ok_or("Snapshot is empty")?.1
    } else if let Some(text) = input.as_str() {
        text.as_bytes().to_vec()
    } else {
        return Err("Snapshot input must be text or a scoped file path".into());
    };
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("Snapshot exceeds 64 MiB".into());
    }
    use sha2::{Digest, Sha256};
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let dir = store.root.join("blobs").join(&digest);
    let path = dir.join(filename);
    crate::orchestration::durable::publish(&path, &bytes)?;
    Ok(json!({"kind":"artifact","path":path,"hash":digest,"filename":filename,"bytes":bytes.len()}))
}

pub fn artifact_path(store: &Store, value: &Value) -> Result<String> {
    let path = value
        .get("path")
        .and_then(Value::as_str)
        .ok_or("Review needs an immutable snapshot output")?;
    let digest = value
        .get("hash")
        .and_then(Value::as_str)
        .ok_or("Artifact hash is missing")?;
    if ![Some("artifact"), Some("artifactTree")]
        .contains(&value.get("kind").and_then(Value::as_str))
    {
        return Err("Review input is not an artifact".into());
    }
    let canonical = Path::new(path).canonicalize().map_err(err)?;
    let root = store.root.join("blobs").canonicalize().map_err(err)?;
    if !canonical.starts_with(&root) {
        return Err("Review input is outside task artifact storage".into());
    }
    if value["kind"] == "artifactTree" {
        let files = crate::workbench::project::task_capture(&canonical, &canonical)
            .map_err(|e| e.message)?;
        let manifest = tree_manifest(&files);
        if hash(&manifest)? != digest {
            return Err("Snapshot tree changed after capture".into());
        }
        return Ok(canonical.to_string_lossy().into_owned());
    }
    let mut file = crate::safety::open_regular_file(&canonical)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    use sha2::{Digest, Sha256};
    if bytes.len() > 64 * 1024 * 1024 || format!("{:x}", Sha256::digest(bytes)) != digest {
        return Err("Review input changed after it was captured".into());
    }
    Ok(canonical.to_string_lossy().into_owned())
}

fn tree_manifest(files: &[(String, Vec<u8>)]) -> Value {
    use sha2::{Digest, Sha256};
    json!(files.iter().map(|(path,bytes)|json!({"path":path,"hash":format!("{:x}",Sha256::digest(bytes)),"bytes":bytes.len()})).collect::<Vec<_>>())
}

fn snapshot_tree(store: &Store, files: Vec<(String, Vec<u8>)>) -> Result<Value> {
    if files.is_empty() {
        return Err("Snapshot folder is empty".into());
    }
    let manifest = tree_manifest(&files);
    let digest = hash(&manifest)?;
    let root = store.root.join("blobs");
    crate::orchestration::durable::create_directories(&root)?;
    let destination = root.join(format!("tree-{digest}"));
    if !destination.exists() {
        let temp = tempfile::tempdir_in(&root).map_err(err)?;
        for (name, bytes) in &files {
            let path = temp.path().join(name);
            crate::orchestration::durable::publish(&path, bytes)?;
        }
        crate::orchestration::durable::rename(temp.path(), &destination)?;
    }
    Ok(
        json!({"kind":"artifactTree","path":destination,"hash":digest,"manifest":manifest,"filename":"paper-project","bytes":files.iter().map(|(_,b)|b.len()).sum::<usize>()}),
    )
}

//! Trash journals are device-local metadata, never filesystem authority.
use super::err;
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use std::path::{Component, Path, PathBuf};

pub(super) fn checked_paths(
    store: &Store,
    id: &str,
    original: &Path,
    recorded_trash: &Path,
) -> WorkbenchResult<(PathBuf, PathBuf)> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err(WorkbenchError::invalid("Invalid trash identifier"));
    }
    let relative = original
        .strip_prefix(store.root_path())
        .map_err(|_| WorkbenchError::invalid("Trash belongs to a different data folder"))?;
    let parts = relative.components().collect::<Vec<_>>();
    if parts.len() != 2
        || parts.iter().any(|p| !matches!(p, Component::Normal(_)))
        || !matches!(
            parts[0].as_os_str().to_str(),
            Some("blobs" | "jobs" | "context" | "backups")
        )
        || relative == Path::new("jobs/conversations")
    {
        return Err(WorkbenchError::invalid(
            "Trash original path is not disposable storage",
        ));
    }
    let directory = store.root_path().join("trash").join(id);
    let trash = directory.join(relative);
    if trash != recorded_trash {
        return Err(WorkbenchError::invalid(
            "Trash path does not match its local journal",
        ));
    }
    check_ancestors(store.root_path(), relative)?;
    check_ancestors(
        store.root_path(),
        trash.strip_prefix(store.root_path()).unwrap(),
    )?;
    Ok((directory, trash))
}

fn check_ancestors(root: &Path, relative: &Path) -> WorkbenchResult<()> {
    let mut path = root.to_path_buf();
    for component in std::iter::once(None).chain(relative.components().map(Some)) {
        if let Some(component) = component {
            path.push(component);
        }
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(WorkbenchError::invalid(
                    "Trash paths must not contain symbolic links",
                ))
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(err(e)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_foreign_traversal_and_mismatched_journals() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_at(root.path()).unwrap();
        let original = root.path().join("context/stale");
        let trash = root.path().join("trash/local/context/stale");
        assert!(checked_paths(&store, "local", &original, &trash).is_ok());
        for id in ["../local", "..", "/local", ""] {
            assert!(checked_paths(&store, id, &original, &trash).is_err());
        }
        assert!(
            checked_paths(&store, "local", Path::new("/foreign/context/stale"), &trash).is_err()
        );
        assert!(checked_paths(
            &store,
            "local",
            &root.path().join("context/../sentinel"),
            &trash
        )
        .is_err());
        assert!(checked_paths(
            &store,
            "local",
            &original,
            &root.path().join("trash/other/context/stale")
        )
        .is_err());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_trash_before_any_mutation() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_at(root.path()).unwrap();
        let foreign = tempfile::tempdir().unwrap();
        std::fs::write(foreign.path().join("sentinel"), "keep").unwrap();
        std::os::unix::fs::symlink(foreign.path(), root.path().join("trash")).unwrap();
        assert!(checked_paths(
            &store,
            "local",
            &root.path().join("context/stale"),
            &root.path().join("trash/local/context/stale")
        )
        .is_err());
        assert_eq!(
            std::fs::read_to_string(foreign.path().join("sentinel")).unwrap(),
            "keep"
        );
    }
}

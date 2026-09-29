//! Registration and dispatch share the same project-root authority checks.
use super::*;

pub(crate) fn canonical_workspace_root(root: &Path, store_root: &Path) -> WorkbenchResult<PathBuf> {
    if !root.is_absolute() {
        return Err(WorkbenchError::invalid("Workspace root must be absolute"));
    }
    let canonical = root.canonicalize().map_err(|error| {
        WorkbenchError::invalid(format!("Workspace root is unavailable: {error}"))
    })?;
    if !canonical.is_dir() {
        return Err(WorkbenchError::invalid(
            "Workspace root must be a directory",
        ));
    }
    if canonical.parent().is_none() {
        return Err(WorkbenchError::invalid(
            "Filesystem root cannot be a workspace",
        ));
    }
    let canonical_store = store_root.canonicalize().map_err(|error| {
        WorkbenchError::storage("Failed to resolve Workbench storage root", error)
    })?;
    if canonical.starts_with(&canonical_store) || canonical_store.starts_with(&canonical) {
        return Err(WorkbenchError::invalid(
            "Workspace root cannot overlap Workbench-owned storage",
        ));
    }
    // A custom research directory no longer encloses the local credential
    // store. Keep that store protected when registering project folders.
    if let Ok(local) =
        crate::storage::local_root().and_then(|p| p.canonicalize().map_err(|e| e.to_string()))
    {
        if canonical.starts_with(&local) || local.starts_with(&canonical) {
            return Err(WorkbenchError::invalid(
                "Workspace root cannot overlap Pipeline's local settings and credentials",
            ));
        }
    }
    if let Some(home) = dirs::home_dir().and_then(|path| path.canonicalize().ok()) {
        if canonical == home || home.starts_with(&canonical) {
            return Err(WorkbenchError::invalid(
                "Home directory or one of its ancestors is too broad for a workspace",
            ));
        }
    }
    Ok(canonical)
}

pub(crate) fn root_identity(root: &Path) -> WorkbenchResult<String> {
    #[cfg(unix)]
    let metadata = std::fs::metadata(root).map_err(|error| {
        WorkbenchError::invalid(format!("Cannot inspect workspace root: {error}"))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        };
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(root)
            .map_err(|e| WorkbenchError::storage("Cannot open project directory identity", e))?;
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(WorkbenchError::storage(
                "Cannot read project directory identity",
                std::io::Error::last_os_error(),
            ));
        }
        Ok(format!(
            "windows:{}:{}:{}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(WorkbenchError::invalid(
            "Project directory identities are unsupported on this platform",
        ))
    }
}

impl Store {
    pub(crate) fn registered_root(&self, workspace: &Workspace) -> WorkbenchResult<PathBuf> {
        let registered = workspace
            .root
            .as_deref()
            .ok_or_else(|| WorkbenchError::invalid("Register a project folder first"))?;
        let canonical = canonical_workspace_root(Path::new(registered), &self.root)?;
        if workspace.root_identity.as_deref() != Some(root_identity(&canonical)?.as_str()) {
            return Err(WorkbenchError::invalid(
                "The registered project folder has changed. Register the folder again before continuing",
            ));
        }
        Ok(canonical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Store, Workspace, String) {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("project");
        std::fs::create_dir(&root).unwrap();
        let store = Store::open_at(&t.path().join("store")).unwrap();
        let ws = store
            .create_workspace(CreateWorkspaceRequest {
                name: "test".into(),
                root: Some(root.to_string_lossy().into()),
                operation_id: "workspace".into(),
            })
            .unwrap()
            .record;
        let session = store
            .create_session(CreateSessionRequest {
                workspace_id: Some(ws.id.clone()),
                title: "test".into(),
                operation_id: "session".into(),
            })
            .unwrap()
            .record
            .id;
        (t, store, ws, session)
    }
    #[test]
    fn runtime_rejects_missing_or_replaced_root_until_explicit_registration() {
        let (t, s, ws, session) = fixture();
        let root = PathBuf::from(ws.root.as_ref().unwrap());
        assert_eq!(s.runtime_root(&session).unwrap(), root);
        std::fs::rename(&root, t.path().join("original")).unwrap();
        assert!(s.runtime_root(&session).is_err());
        std::fs::create_dir(&root).unwrap();
        assert!(s
            .runtime_root(&session)
            .unwrap_err()
            .message
            .contains("changed"));
        s.register_workspace_root(RegisterWorkspaceRootRequest {
            workspace_id: ws.id,
            root: root.to_string_lossy().into(),
            operation_id: "register".into(),
            expected_revision: ws.revision,
        })
        .unwrap();
        assert_eq!(s.runtime_root(&session).unwrap(), root);
    }
    #[cfg(unix)]
    #[test]
    fn runtime_rejects_symlink_substitution_including_private_storage() {
        for private in [false, true] {
            let (t, s, ws, session) = fixture();
            let root = PathBuf::from(ws.root.unwrap());
            std::fs::rename(&root, t.path().join("original")).unwrap();
            let target = if private {
                s.root.clone()
            } else {
                let p = t.path().join("other");
                std::fs::create_dir(&p).unwrap();
                p
            };
            std::os::unix::fs::symlink(target, &root).unwrap();
            assert!(s.runtime_root(&session).is_err());
        }
    }
    #[test]
    fn rootless_conversations_keep_their_private_owned_root() {
        let (_t, s, _ws, _session) = fixture();
        let id = s
            .create_session(CreateSessionRequest {
                workspace_id: None,
                title: "unfiled".into(),
                operation_id: "unfiled".into(),
            })
            .unwrap()
            .record
            .id;
        assert_eq!(
            s.runtime_root(&id).unwrap(),
            s.root.join("jobs/conversations").join(id)
        );
    }
}

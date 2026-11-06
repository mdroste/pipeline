//! Restore only the three visual subtrees owned by a Review. Read through
//! pinned directories and publish verified bytes atomically in the new run.
use super::{short_sha256, RunManifest, RunWriter};
use crate::document_bundle::{DocumentBundle, DocumentQualityNote, VisualIdentity};
use std::{collections::HashSet, fs::File, io::Read, path::Path};

const MAX_IMAGE: u64 = 32 * 1024 * 1024;
const MAX_VISUALS: u64 = crate::safety::MAX_RUN_OUTPUT_BYTES as u64;

// The visual producers all write one filename below a known directory. Also
// reject Windows stream and device-path spellings before opening anything.
fn visual_parts(rel: &str) -> Result<Vec<&str>, String> {
    let parts: Vec<_> = rel.split('/').collect();
    let Some((name, directory)) = parts.split_last() else {
        return Err("Empty visual path".into());
    };
    if !matches!(
        directory,
        ["artifacts", "pages"] | ["artifacts", "figures"] | ["artifacts", "document", "figures"]
    ) || name.is_empty()
        || name.ends_with(['.', ' '])
        || name.contains([':', '\\'])
        || name.chars().any(char::is_control)
    {
        return Err("Invalid retained visual path".into());
    }
    Ok(parts)
}

#[cfg(unix)]
fn open_visual(root: &Path, parts: &[&str]) -> std::io::Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::OpenOptionsExt;
    let mut handle = File::options()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(root)?;
    for (index, name) in parts.iter().enumerate() {
        let name = std::ffi::CString::new(*name)?;
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
        if index + 1 < parts.len() {
            flags |= libc::O_DIRECTORY;
        }
        // SAFETY: the directory fd and C string live for the call. A successful
        // openat returns a new fd, which immediately has exactly one owner.
        let fd = unsafe { libc::openat(handle.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        handle = unsafe { File::from_raw_fd(fd) };
    }
    Ok(handle)
}

#[cfg(windows)]
fn open_visual(root: &Path, parts: &[&str]) -> std::io::Result<File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ,
    };
    let mut path = root.to_path_buf();
    let mut directories = Vec::new();
    // Pin every traversed directory against replacement or reparse mutation.
    for index in 0..=parts.len() {
        if index > 0 {
            path.push(parts[index - 1]);
        }
        let handle = File::options()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)?;
        let meta = handle.metadata()?;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(std::io::Error::other("linked visual path"));
        }
        if index == parts.len() {
            return Ok(handle);
        }
        if !meta.is_dir() {
            return Err(std::io::Error::other("invalid visual directory"));
        }
        directories.push(handle);
    }
    unreachable!()
}

impl RunWriter {
    /// Keep each image's checksum inside the bundle even after homogeneous
    /// page records have been compacted in the manifest.
    pub(crate) fn bind_visual_identities(&self, bundle: &mut DocumentBundle) {
        for asset in &mut bundle.assets {
            if let Some(entry) = self.artifacts.iter().find(|a| a.rel_path == asset.rel_path) {
                asset.identity = Some(VisualIdentity {
                    bytes: entry.bytes,
                    sha256: entry.sha256.clone(),
                });
            }
        }
    }

    pub(crate) fn restore_visuals(
        &mut self,
        parent: &Path,
        manifest: &RunManifest,
        bundle: &mut DocumentBundle,
    ) -> Result<(), String> {
        let mut remaining = MAX_VISUALS;
        let mut missing = HashSet::new();
        for asset in &bundle.assets {
            let entry = manifest
                .artifacts
                .iter()
                .find(|e| e.rel_path == asset.rel_path);
            let data = (|| {
                let parts = visual_parts(&asset.rel_path)?;
                let file = open_visual(parent, &parts).map_err(|e| e.to_string())?;
                let meta = file.metadata().map_err(|e| e.to_string())?;
                let limit = MAX_IMAGE.min(remaining);
                if !meta.is_file() || meta.len() > limit {
                    return Err("not a regular image within the restore limit".into());
                }
                let mut bytes = Vec::new();
                file.take(limit + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                let len = bytes.len() as u64;
                remaining = remaining.saturating_sub(len);
                let digest = short_sha256(&bytes);
                if len > limit
                    || len != meta.len()
                    || asset
                        .identity
                        .as_ref()
                        .is_some_and(|i| i.bytes != len || i.sha256 != digest)
                    || entry.is_some_and(|e| {
                        e.bytes != len || (!e.sha256.is_empty() && e.sha256 != digest)
                    })
                {
                    return Err("size or checksum differs from the captured image".to_string());
                }
                Ok(bytes)
            })();
            match data {
                Ok(bytes) => {
                    let group = if asset.kind == "page" {
                        "pages"
                    } else {
                        "figures"
                    };
                    self.add_bytes(&asset.rel_path, &asset.label, group, &bytes)?;
                }
                Err(reason) => {
                    missing.insert(asset.id.clone());
                    bundle.quality.push(DocumentQualityNote {
                        severity: "warning".into(),
                        scope: "visuals".into(),
                        message: format!("Could not restore '{}': {reason}", asset.rel_path),
                    });
                }
            }
        }
        bundle.assets.retain(|a| !missing.contains(&a.id));
        for page in &mut bundle.pages {
            if page
                .asset_id
                .as_ref()
                .is_some_and(|id| missing.contains(id))
            {
                page.asset_id = None;
            }
        }
        for node in &mut bundle.nodes {
            node.asset_ids.retain(|id| !missing.contains(id));
        }
        bundle
            .links
            .retain(|link| !missing.contains(&link.from_id) && !missing.contains(&link.to_id));
        self.bind_visual_identities(bundle);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_omits_missing_replaced_and_linked_visuals() {
        let t = tempfile::tempdir().unwrap();
        let mut parent = RunWriter::create_in(t.path(), "parent").unwrap();
        let page = "artifacts/pages/page-01.png";
        parent
            .add_bytes(page, "Page 1", "pages", b"original")
            .unwrap();
        let mut manifest = parent.current_manifest();
        let make_bundle = || {
            serde_json::from_value::<DocumentBundle>(serde_json::json!({
            "schema_version":"1.0","bundle_id":"b","source_kind":"pdf","origins":[],"pages":[{"number":1,"label":"Page 1","asset_id":"a"}],
            "nodes":[],"links":[],"assets":[{"id":"a","label":"Page 1","kind":"page","rel_path":page,"media_type":"image/png","provenance":{"origin_id":"o","method":"render"}}],
            "extraction":{"method":"test","source_path":"","quality_notes":[],"paper_hash":"","text_chars":0,"quality_note_count":0}
        })).unwrap()
        };
        let mut child = RunWriter::create_in(t.path(), "child").unwrap();
        let mut bundle = make_bundle();
        child
            .restore_visuals(parent.dir(), &manifest, &mut bundle)
            .unwrap();
        assert_eq!(std::fs::read(child.dir().join(page)).unwrap(), b"original");
        let expected = bundle.assets[0].identity.clone();
        let mut unwritable = RunWriter::create_in(t.path(), "unwritable").unwrap();
        std::fs::create_dir_all(unwritable.dir().join("artifacts")).unwrap();
        std::fs::write(unwritable.dir().join("artifacts/pages"), "obstruction").unwrap();
        assert!(unwritable
            .restore_visuals(parent.dir(), &manifest, &mut make_bundle())
            .is_err());
        manifest.artifacts.clear(); // older completed runs have neither per-page checksum
        let mut legacy = RunWriter::create_in(t.path(), "legacy").unwrap();
        let mut old_bundle = make_bundle();
        legacy
            .restore_visuals(parent.dir(), &manifest, &mut old_bundle)
            .unwrap();
        assert!(old_bundle.assets[0].identity.is_some());

        std::fs::write(parent.dir().join(page), b"modified").unwrap();
        let rejected = |id: &str| {
            let mut child = RunWriter::create_in(t.path(), id).unwrap();
            let mut bundle = make_bundle();
            bundle.assets[0].identity = expected.clone();
            child
                .restore_visuals(parent.dir(), &manifest, &mut bundle)
                .unwrap();
            assert!(!child.dir().join(page).exists());
            assert!(bundle.pages[0].asset_id.is_none() && bundle.assets.is_empty());
            assert!(!bundle.quality.is_empty());
        };
        rejected("changed");
        std::fs::remove_file(parent.dir().join(page)).unwrap();
        rejected("missing");
        #[cfg(unix)]
        {
            let outside = t.path().join("outside");
            std::fs::write(&outside, b"original").unwrap(); // matching hash must still be rejected
            std::os::unix::fs::symlink(&outside, parent.dir().join(page)).unwrap();
            rejected("linked");
            std::fs::rename(
                parent.dir().join("artifacts/pages"),
                t.path().join("elsewhere"),
            )
            .unwrap();
            std::os::unix::fs::symlink(
                t.path().join("elsewhere"),
                parent.dir().join("artifacts/pages"),
            )
            .unwrap();
            rejected("ancestor");
        }
    }
}

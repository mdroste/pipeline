//! Streaming file identity for host execution.
//!
//! Empirical inputs and outputs routinely run to many gigabytes, so a file's
//! identity has no size limit. Whether its bytes are also kept is a separate
//! question with its own limits: a file over them keeps its hash and is reported
//! as not kept. A link is refused rather than followed, because the identity
//! must describe the bytes at the declared path.

use crate::workbench::store::{WorkbenchError, WorkbenchResult};
use sha2::{Digest as _, Sha256};
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// Total bytes of declared inputs copied into private storage for one run.
pub(in crate::workbench::research) const SNAPSHOT_BUDGET: u64 = 256 * 1024 * 1024;

/// Why a declared file has no identity.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::workbench::research) enum FileProblem {
    Absent,
    Link,
    NotRegular,
    Unreadable(String),
}

impl FileProblem {
    /// Reads after the file's name, as in "data.dta is a symbolic link".
    pub(in crate::workbench::research) fn reason(&self) -> String {
        match self {
            Self::Absent => "was not found".into(),
            Self::Link => "is a symbolic link, which can't be used here".into(),
            Self::NotRegular => "is not a regular file".into(),
            Self::Unreadable(error) => format!("can't be read: {error}"),
        }
    }
}

pub(in crate::workbench::research) fn hash_regular_file(
    path: &Path,
) -> Result<(String, u64), FileProblem> {
    let metadata = fs::symlink_metadata(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => FileProblem::Absent,
        _ => FileProblem::Unreadable(error.to_string()),
    })?;
    if metadata.file_type().is_symlink() {
        return Err(FileProblem::Link);
    }
    if !metadata.is_file() {
        return Err(FileProblem::NotRegular);
    }
    let mut file = crate::safety::open_regular_file(path).map_err(FileProblem::Unreadable)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| FileProblem::Unreadable(error.to_string()))?;
        if count == 0 {
            break;
        }
        size += count as u64;
        hasher.update(&buffer[..count]);
    }
    Ok((format!("{:x}", hasher.finalize()), size))
}

/// Hash a host-owned file through its resolved target (dotfile managers
/// symlink shell startup files and environment configuration); the caller
/// records the declared path so existing fingerprints are unchanged.
pub(in crate::workbench::research) fn hash_resolved_file(
    path: &Path,
    label: &str,
) -> WorkbenchResult<(PathBuf, String, u64)> {
    let resolved = fs::canonicalize(path)
        .map_err(|error| WorkbenchError::storage(&format!("Failed to resolve {label}"), error))?;
    let (hash, size) = hash_regular_file(&resolved)
        .map_err(|problem| WorkbenchError::invalid(format!("The {label} {}", problem.reason())))?;
    Ok((resolved, hash, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_matches_the_paper_hash_and_has_no_size_limit() {
        let temporary = tempfile::tempdir().unwrap();
        let small = temporary.path().join("small.csv");
        fs::write(&small, "a,b\n1,2\n").unwrap();
        // Saved fingerprints hold these values, so the two must never diverge.
        assert_eq!(
            hash_regular_file(&small).unwrap(),
            crate::workbench::research::papers::hash_file(&small).unwrap()
        );
        let large = temporary.path().join("large.dta");
        let file = fs::File::create(&large).unwrap();
        file.set_len(201 * 1024 * 1024).unwrap();
        drop(file);
        assert_eq!(hash_regular_file(&large).unwrap().1, 201 * 1024 * 1024);
    }

    #[cfg(unix)]
    #[test]
    fn each_problem_is_reported_as_itself() {
        use std::os::unix::fs::PermissionsExt as _;
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        assert_eq!(
            hash_regular_file(&root.join("absent")),
            Err(FileProblem::Absent)
        );
        assert_eq!(hash_regular_file(root), Err(FileProblem::NotRegular));
        fs::write(root.join("real"), "real").unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();
        assert_eq!(
            hash_regular_file(&root.join("link")),
            Err(FileProblem::Link)
        );
        let locked = root.join("locked");
        fs::write(&locked, "locked").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let problem = hash_regular_file(&locked).unwrap_err();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(problem, FileProblem::Unreadable(_)), "{problem:?}");
    }
}

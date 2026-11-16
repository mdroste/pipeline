//! Rootless hard links without resolving a client path in the host namespace.
use fuse_backend_rs::abi::fuse_abi::stat64;
use std::{
    ffi::{CStr, CString},
    fs::File,
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Component, Path},
};

fn open_beneath(root: &File, path: &Path) -> io::Result<File> {
    let mut file = root.try_clone()?;
    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            return Err(io::Error::from_raw_os_error(libc::EINVAL));
        };
        let name = CString::new(name.as_bytes())?;
        let mut flags = libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        if components.peek().is_some() {
            flags |= libc::O_DIRECTORY;
        }
        // Every lookup is relative to a held directory descriptor; symlinks
        // cannot redirect either a parent component or the final component.
        let fd = unsafe { libc::openat(file.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        file = unsafe { File::from_raw_fd(fd) };
    }
    Ok(file)
}

fn same_inode(file: &File, expected: &stat64) -> io::Result<()> {
    let actual = file.metadata()?;
    if actual.dev() != expected.st_dev || actual.ino() != expected.st_ino {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    Ok(())
}

pub(super) fn link(
    root: &File,
    source: &Path,
    parent: &Path,
    name: &CStr,
    source_stat: &stat64,
    parent_stat: &stat64,
) -> io::Result<()> {
    let source = open_beneath(root, source)?;
    let parent = open_beneath(root, parent)?;
    same_inode(&source, source_stat)?;
    same_inode(&parent, parent_stat)?;
    if !source.metadata()?.is_file() || !parent.metadata()?.is_dir() {
        return Err(io::Error::from_raw_os_error(libc::EOPNOTSUPP));
    }
    // The upstream AT_EMPTY_PATH operation needs CAP_DAC_READ_SEARCH. Linux
    // supports the same held-inode operation through procfs without that cap.
    let source_path = CString::new(format!("/proc/self/fd/{}", source.as_raw_fd()))?;
    let result = unsafe {
        libc::linkat(
            libc::AT_FDCWD,
            source_path.as_ptr(),
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::AT_SYMLINK_FOLLOW,
        )
    };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_walk_never_follows_symlinks_or_parent_components() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("alias")).unwrap();
        std::fs::write(outside.path().join("file"), "outside").unwrap();
        let root = File::open(root.path()).unwrap();
        assert!(open_beneath(&root, Path::new("alias/file")).is_err());
        assert!(open_beneath(&root, Path::new("../file")).is_err());
        assert!(open_beneath(&root, Path::new("/file")).is_err());
        assert!(open_beneath(&root, Path::new("alias"))
            .unwrap()
            .metadata()
            .unwrap()
            .file_type()
            .is_symlink());
    }
}

//! No-clobber placement of one file, on volumes with and without hard links.
//! exFAT drives and many SMB shares refuse `linkat`, so an exclusive rename and
//! then an exclusive create take over. None of them can replace an existing file.
use std::ffi::CStr;
use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};

// How many placement methods a test pretends this volume lacks.
#[cfg(test)]
thread_local! {
    static SIMULATED_MISSING: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}
fn missing_on_this_volume(_method: u8) -> bool {
    #[cfg(test)]
    {
        SIMULATED_MISSING.with(|n| n.get() >= _method)
    }
    #[cfg(not(test))]
    {
        false
    }
}
/// Whether a failure means the volume lacks the operation, rather than that
/// something is in the way.
pub(super) fn unsupported(error: &io::Error) -> bool {
    error.raw_os_error().is_some_and(|code| {
        [
            libc::EPERM,
            libc::ENOTSUP,
            libc::EOPNOTSUPP,
            libc::ENOSYS,
            libc::EINVAL,
        ]
        .contains(&code)
    })
}
fn result(status: libc::c_int) -> io::Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
fn link(directory: &fs::File, from: &CStr, to: &CStr) -> io::Result<()> {
    if missing_on_this_volume(1) {
        return Err(io::Error::from_raw_os_error(libc::ENOTSUP));
    }
    let fd = directory.as_raw_fd();
    result(unsafe { libc::linkat(fd, from.as_ptr(), fd, to.as_ptr(), 0) })
}
fn rename_exclusive(directory: &fs::File, from: &CStr, to: &CStr) -> io::Result<()> {
    if missing_on_this_volume(2) {
        return Err(io::Error::from_raw_os_error(libc::ENOTSUP));
    }
    let fd = directory.as_raw_fd();
    #[cfg(target_os = "macos")]
    return result(unsafe {
        libc::renameatx_np(fd, from.as_ptr(), fd, to.as_ptr(), libc::RENAME_EXCL)
    });
    #[cfg(target_os = "linux")]
    return result(unsafe {
        libc::renameat2(fd, from.as_ptr(), fd, to.as_ptr(), libc::RENAME_NOREPLACE)
    });
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (fd, from, to);
        Err(io::Error::from_raw_os_error(libc::ENOTSUP))
    }
}
fn open_at(
    directory: &fs::File,
    name: &CStr,
    flags: libc::c_int,
    mode: u32,
) -> io::Result<fs::File> {
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openat returned a new owned descriptor.
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}
/// The last resort: `O_EXCL` refuses an existing file, and the written bytes are
/// read back before the source can be removed. A failed copy removes only the
/// file this call created.
fn copy_exclusive(directory: &fs::File, from: &CStr, to: &CStr) -> io::Result<()> {
    if missing_on_this_volume(3) {
        return Err(io::Error::from_raw_os_error(libc::ENOTSUP));
    }
    let mut source = open_at(directory, from, libc::O_RDONLY, 0)?;
    let metadata = source.metadata()?;
    let mut bytes = Vec::new();
    source.read_to_end(&mut bytes)?;
    let flags = libc::O_CREAT | libc::O_EXCL | libc::O_WRONLY;
    let mut target = open_at(directory, to, flags, 0o600)?;
    let written = (|| {
        target.write_all(&bytes)?;
        target.sync_all()?;
        let mut check = Vec::new();
        open_at(directory, to, libc::O_RDONLY, 0)?.read_to_end(&mut check)?;
        if check != bytes {
            return Err(io::Error::other("The written file did not read back"));
        }
        Ok(())
    })();
    if let Err(error) = written {
        unsafe { libc::unlinkat(directory.as_raw_fd(), to.as_ptr(), 0) };
        return Err(error);
    }
    // Some of these volumes have no permission bits to set.
    let _ = target.set_permissions(metadata.permissions());
    Ok(())
}
/// Move `from` onto the free name `to` without ever replacing `to`. On failure
/// `from` is still there, and `to` is whatever it was.
pub(super) fn move_no_clobber(directory: &fs::File, from: &CStr, to: &CStr) -> io::Result<()> {
    let unlink = || result(unsafe { libc::unlinkat(directory.as_raw_fd(), from.as_ptr(), 0) });
    match link(directory, from, to) {
        Ok(()) => return unlink(),
        Err(error) if !unsupported(&error) => return Err(error),
        Err(_) => (),
    }
    match rename_exclusive(directory, from, to) {
        Ok(()) => return Ok(()),
        Err(error) if !unsupported(&error) => return Err(error),
        Err(_) => (),
    }
    copy_exclusive(directory, from, to)?;
    unlink()
}

#[cfg(test)]
mod tests {
    use super::super::SafeRoot;
    use super::*;
    use crate::workbench::project::hash;

    fn simulate(methods: u8) {
        SIMULATED_MISSING.with(|n| n.set(methods));
    }
    fn folder() -> (tempfile::TempDir, SafeRoot, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        (temp, SafeRoot::open(&root).unwrap(), root)
    }
    #[test]
    fn saves_and_new_files_land_when_the_volume_has_no_hard_links() {
        for methods in [1, 2] {
            simulate(methods);
            let (_temp, safe, root) = folder();
            fs::write(root.join("main.tex"), "before").unwrap();
            safe.replace(
                "main.tex",
                Some(&hash(b"before")),
                false,
                Some(b"after"),
                false,
                "save",
            )
            .unwrap();
            assert_eq!(fs::read_to_string(root.join("main.tex")).unwrap(), "after");
            assert_eq!(
                fs::read_to_string(root.join(".pipeline-apply-save")).unwrap(),
                "before",
                "the original stays recoverable until the edit is finalized"
            );
            safe.clear_claim("main.tex", "save").unwrap();
            safe.replace("sub/new.tex", None, false, Some(b"new"), true, "new")
                .unwrap();
            assert_eq!(fs::read_to_string(root.join("sub/new.tex")).unwrap(), "new");
            assert!(safe.executable("sub/new.tex").unwrap());
            let left: Vec<_> = fs::read_dir(&root)
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with(".pipeline-apply-"))
                .collect();
            assert!(left.is_empty(), "{left:?}");
        }
        simulate(0);
    }
    #[test]
    fn no_method_ever_replaces_an_existing_file() {
        for methods in 0..=2 {
            simulate(methods);
            let (_temp, _safe, root) = folder();
            fs::write(root.join("from"), "incoming").unwrap();
            fs::write(root.join("to"), "researcher's edit").unwrap();
            let directory = fs::File::open(&root).unwrap();
            let error = move_no_clobber(&directory, c"from", c"to").unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::AlreadyExists, "{methods}");
            assert_eq!(
                fs::read_to_string(root.join("to")).unwrap(),
                "researcher's edit"
            );
            assert_eq!(fs::read_to_string(root.join("from")).unwrap(), "incoming");
        }
        simulate(0);
    }
    #[test]
    fn recovery_restores_the_original_without_hard_links() {
        for methods in [1, 2] {
            simulate(methods);
            let (_temp, safe, root) = folder();
            fs::write(root.join(".pipeline-apply-undo"), "original").unwrap();
            assert!(safe.recover_claim("paper.md", "undo").unwrap());
            assert_eq!(
                fs::read_to_string(root.join("paper.md")).unwrap(),
                "original"
            );
            assert!(!root.join(".pipeline-apply-undo").exists());
        }
        simulate(0);
    }
    #[test]
    fn a_volume_with_no_safe_method_gets_a_plain_message_and_loses_nothing() {
        simulate(3);
        let (_temp, safe, root) = folder();
        fs::write(root.join("main.tex"), "before").unwrap();
        let error = safe
            .replace(
                "main.tex",
                Some(&hash(b"before")),
                false,
                Some(b"after"),
                false,
                "save",
            )
            .unwrap_err();
        simulate(0);
        assert!(
            error.message.contains("The drive holding this project"),
            "{error:?}"
        );
        assert!(
            error.message.contains("Nothing was overwritten"),
            "{error:?}"
        );
        assert_eq!(
            fs::read_to_string(root.join(".pipeline-apply-save")).unwrap(),
            "before"
        );
        assert!(safe.recover_claim("main.tex", "save").unwrap());
        assert_eq!(fs::read_to_string(root.join("main.tex")).unwrap(), "before");
    }
}

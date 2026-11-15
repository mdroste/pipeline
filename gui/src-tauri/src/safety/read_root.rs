//! Read-only, handle-relative file access. Callers own the root authority and
//! size policy; these mechanics never resolve a descendant through a link.
use std::fs::File;
use std::io;
use std::path::{Component, Path};

#[cfg(windows)]
mod windows;

pub(crate) struct ReadRoot {
    directory: File,
}

impl ReadRoot {
    pub(crate) fn open(root: &Path) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            Ok(Self {
                directory: File::options()
                    .read(true)
                    .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                    .open(root)?,
            })
        }
        #[cfg(windows)]
        {
            Ok(Self {
                directory: windows::open_root(root)?,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = root;
            Err(io::Error::other("Confined file reading is unavailable"))
        }
    }

    pub(crate) fn open_regular(&self, relative: &Path) -> io::Result<File> {
        self.open_regular_with(relative, || {})
    }

    // The hook lets tests replace a pathname after the parent handle is open.
    fn open_regular_with(&self, relative: &Path, before_file: impl FnOnce()) -> io::Result<File> {
        let mut parts = relative.components().peekable();
        if parts.peek().is_none()
            || parts
                .clone()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Expected a relative file path",
            ));
        }
        let mut parent = None;
        while let Some(Component::Normal(name)) = parts.next() {
            let directory = parent.as_ref().unwrap_or(&self.directory);
            let last = parts.peek().is_none();
            if last {
                before_file();
                let file = open_child(directory, name, false)?;
                if !file.metadata()?.is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Expected a regular file",
                    ));
                }
                return Ok(file);
            }
            parent = Some(open_child(directory, name, true)?);
        }
        unreachable!("nonempty normal components checked above")
    }
}

#[cfg(unix)]
fn open_child(parent: &File, name: &std::ffi::OsStr, directory: bool) -> io::Result<File> {
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    };
    let name = std::ffi::CString::new(name.as_bytes())?;
    let flags = libc::O_RDONLY
        | libc::O_NOFOLLOW
        | libc::O_NONBLOCK
        | libc::O_CLOEXEC
        | if directory { libc::O_DIRECTORY } else { 0 };
    // SAFETY: the parent and NUL-terminated single component remain live; a
    // successful descriptor is immediately owned by File.
    let descriptor = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if descriptor < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

#[cfg(windows)]
use windows::open_child;

#[cfg(not(any(unix, windows)))]
fn open_child(_: &File, _: &std::ffi::OsStr, _: bool) -> io::Result<File> {
    Err(io::Error::other("Confined file reading is unavailable"))
}

#[cfg(test)]
mod tests;

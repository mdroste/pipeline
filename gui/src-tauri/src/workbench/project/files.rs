//! Descriptor-relative reads and journalled, no-clobber replacement on Unix.
use super::*;

pub(super) fn relative(value: &str) -> WorkbenchResult<()> {
    let p = Path::new(value);
    if value.is_empty()
        || value.len() > 4096
        || value.contains('\\')
        || value.chars().any(char::is_control)
        || value.split('/').any(|part| part.is_empty())
        || p.is_absolute()
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
        || value
            .split('/')
            .any(|c| c == ".git" || c == SCRATCH || c.starts_with(".pipeline-apply-"))
    {
        return Err(WorkbenchError::invalid(
            "Choose a normalized relative file path outside Git/task metadata",
        ));
    }
    Ok(())
}
pub(super) fn executable(metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}
pub(super) struct SafeRoot {
    directory: fs::File,
    #[cfg(not(unix))]
    path: PathBuf,
}
impl SafeRoot {
    pub(super) fn open(root: &Path) -> WorkbenchResult<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let directory = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(root)
                .map_err(err)?;
            Ok(Self { directory })
        }
        #[cfg(not(unix))]
        {
            Ok(Self {
                directory: fs::File::open(root).map_err(err)?,
                path: root.to_owned(),
            })
        }
    }
    #[cfg(unix)]
    fn parent(&self, path: &str, create: bool) -> WorkbenchResult<(fs::File, std::ffi::CString)> {
        use std::os::fd::{AsRawFd, FromRawFd};
        relative(path)?;
        let mut directory = self.directory.try_clone().map_err(err)?;
        let mut parts = path.split('/').peekable();
        while let Some(part) = parts.next() {
            let name = std::ffi::CString::new(part).map_err(err)?;
            if parts.peek().is_none() {
                return Ok((directory, name));
            }
            if create {
                let result = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
                if result != 0
                    && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
                {
                    return Err(err(std::io::Error::last_os_error()));
                }
            }
            let fd = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                let e = std::io::Error::last_os_error();
                let mut error = err(&e);
                if e.kind() == std::io::ErrorKind::NotFound {
                    error.code = "file_missing".into();
                }
                return Err(error);
            }
            // SAFETY: openat returned a new owned descriptor.
            directory = unsafe { fs::File::from_raw_fd(fd) };
        }
        Err(WorkbenchError::invalid("Empty file path"))
    }
    #[cfg(unix)]
    fn read_name(directory: &fs::File, name: &std::ffi::CStr) -> WorkbenchResult<Option<Vec<u8>>> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            let e = std::io::Error::last_os_error();
            return if e.kind() == std::io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(err(e))
            };
        }
        let f = unsafe { fs::File::from_raw_fd(fd) };
        let metadata = f.metadata().map_err(err)?;
        if !metadata.is_file() || metadata.len() > MAX_FILE {
            return Err(WorkbenchError::invalid(
                "Only bounded regular files can be captured",
            ));
        }
        let mut bytes = Vec::new();
        f.take(MAX_FILE + 1).read_to_end(&mut bytes).map_err(err)?;
        if bytes.len() as u64 > MAX_FILE {
            return Err(WorkbenchError::invalid("File grew beyond capture limit"));
        }
        Ok(Some(bytes))
    }
    pub(super) fn optional_read(&self, path: &str) -> WorkbenchResult<Option<Vec<u8>>> {
        #[cfg(unix)]
        {
            let (directory, name) = match self.parent(path, false) {
                Ok(pair) => pair,
                Err(e) if e.code == "file_missing" => return Ok(None),
                Err(e) => return Err(e),
            };
            Self::read_name(&directory, &name)
        }
        #[cfg(not(unix))]
        {
            relative(path)?;
            let path = self.path.join(path);
            let mut f = crate::safety::open_regular_file(&path).map_err(err)?;
            let mut bytes = Vec::new();
            Read::by_ref(&mut f)
                .take(MAX_FILE + 1)
                .read_to_end(&mut bytes)
                .map_err(err)?;
            if bytes.len() as u64 > MAX_FILE {
                return Err(WorkbenchError::invalid("File exceeds capture limit"));
            }
            Ok(Some(bytes))
        }
    }
    pub(super) fn executable(&self, path: &str) -> WorkbenchResult<bool> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let (directory, name) = match self.parent(path, false) {
                Ok(pair) => pair,
                Err(e) if e.code == "file_missing" => return Ok(false),
                Err(e) => return Err(e),
            };
            let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe {
                libc::fstatat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    metadata.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::NotFound {
                    return Ok(false);
                }
                return Err(err(error));
            }
            let metadata = unsafe { metadata.assume_init() };
            if metadata.st_mode & libc::S_IFMT != libc::S_IFREG {
                return Err(WorkbenchError::invalid("Only regular files are supported"));
            }
            Ok(metadata.st_mode & 0o111 != 0)
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(false)
        }
    }
    pub(super) fn read(&self, path: &str) -> WorkbenchResult<Vec<u8>> {
        self.optional_read(path)?
            .ok_or_else(|| WorkbenchError::invalid(format!("Missing file: {path}")))
    }
    /// Journal must be durable before calling this function. The claim is retained
    /// until the whole application is finalized, including when any later file fails.
    pub(super) fn replace(
        &self,
        path: &str,
        expected: Option<&str>,
        expected_executable: bool,
        bytes: Option<&[u8]>,
        is_executable: bool,
        claim: &str,
    ) -> WorkbenchResult<()> {
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            valid_id(claim)?;
            let (directory, name) = self.parent(path, bytes.is_some())?;
            let claim_name =
                std::ffi::CString::new(format!(".pipeline-apply-{claim}")).map_err(err)?;
            if Self::read_name(&directory, &claim_name)?.is_some() {
                return Err(WorkbenchError::conflict(
                    "This application has an existing recovery file; recover it first",
                ));
            }
            if self.executable(path)? != expected_executable {
                return Err(WorkbenchError::conflict(
                    "File permissions changed during the task",
                ));
            }
            let mut original_mode = 0o644;
            if expected.is_some() {
                let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
                let result = unsafe {
                    libc::fstatat(
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        metadata.as_mut_ptr(),
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                };
                if result != 0 {
                    return Err(err(std::io::Error::last_os_error()));
                }
                original_mode = unsafe { metadata.assume_init() }.st_mode as libc::mode_t & 0o777;
            }
            let new_mode = (original_mode & 0o666)
                | if is_executable {
                    if original_mode & 0o111 != 0 {
                        original_mode & 0o111
                    } else {
                        0o100
                    }
                } else {
                    0
                };
            if let Some(expected) = expected {
                // Verify before claiming, then again after the rename. An editor
                // racing either check cannot have its replacement overwritten.
                if Self::read_name(&directory, &name)?
                    .as_deref()
                    .map(hash)
                    .as_deref()
                    != Some(expected)
                {
                    return Err(WorkbenchError::conflict(format!(
                        "External change detected: {path}"
                    )));
                }
                let result = unsafe {
                    libc::renameat(
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        directory.as_raw_fd(),
                        claim_name.as_ptr(),
                    )
                };
                if result != 0 {
                    return Err(err(std::io::Error::last_os_error()));
                }
                directory.sync_all().map_err(err)?;
                if Self::read_name(&directory, &claim_name)?
                    .as_deref()
                    .map(hash)
                    .as_deref()
                    != Some(expected)
                {
                    let _ = Self::restore_claim(&directory, &name, &claim_name);
                    return Err(WorkbenchError::conflict(format!(
                        "File changed while being accepted: {path}; recovery retained"
                    )));
                }
            } else if Self::read_name(&directory, &name)?.is_some() {
                return Err(WorkbenchError::conflict(format!(
                    "A new file already exists: {path}"
                )));
            }
            if let Some(bytes) = bytes {
                let temporary = std::ffi::CString::new(format!(".pipeline-apply-{}", id("write")?))
                    .map_err(err)?;
                let fd = unsafe {
                    libc::openat(
                        directory.as_raw_fd(),
                        temporary.as_ptr(),
                        libc::O_CREAT
                            | libc::O_EXCL
                            | libc::O_WRONLY
                            | libc::O_NOFOLLOW
                            | libc::O_CLOEXEC,
                        new_mode as libc::c_uint,
                    )
                };
                if fd < 0 {
                    return Err(err(std::io::Error::last_os_error()));
                }
                let mut file = unsafe { fs::File::from_raw_fd(fd) };
                file.write_all(bytes).map_err(err)?;
                file.sync_all().map_err(err)?;
                let placed = unsafe {
                    libc::linkat(
                        directory.as_raw_fd(),
                        temporary.as_ptr(),
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        0,
                    )
                };
                let placement_error = std::io::Error::last_os_error();
                unsafe { libc::unlinkat(directory.as_raw_fd(), temporary.as_ptr(), 0) };
                if placed != 0 {
                    return Err(WorkbenchError::conflict(format!("Could not place {path} without overwriting another edit: {placement_error}. Recover this application.")));
                }
            }
            directory.sync_all().map_err(err)?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = (
                path,
                expected,
                expected_executable,
                bytes,
                is_executable,
                claim,
            );
            Err(WorkbenchError::invalid(
                "Journalled file acceptance is not yet qualified on this platform",
            ))
        }
    }
    #[cfg(unix)]
    fn restore_claim(
        directory: &fs::File,
        name: &std::ffi::CStr,
        claim: &std::ffi::CStr,
    ) -> WorkbenchResult<()> {
        use std::os::fd::AsRawFd;
        if unsafe {
            libc::linkat(
                directory.as_raw_fd(),
                claim.as_ptr(),
                directory.as_raw_fd(),
                name.as_ptr(),
                0,
            )
        } != 0
        {
            return Err(WorkbenchError::conflict(
                "An external file prevents recovery; original remains in the recovery file",
            ));
        }
        unsafe { libc::unlinkat(directory.as_raw_fd(), claim.as_ptr(), 0) };
        directory.sync_all().map_err(err)
    }
    pub(super) fn recover_claim(&self, path: &str, claim: &str) -> WorkbenchResult<bool> {
        #[cfg(unix)]
        {
            valid_id(claim)?;
            let (directory, name) = match self.parent(path, false) {
                Ok(pair) => pair,
                Err(e) if e.code == "file_missing" => return Ok(false),
                Err(e) => return Err(e),
            };
            let claim = std::ffi::CString::new(format!(".pipeline-apply-{claim}")).map_err(err)?;
            if Self::read_name(&directory, &claim)?.is_some()
                && Self::read_name(&directory, &name)?.is_none()
            {
                Self::restore_claim(&directory, &name, &claim)?;
                return Ok(true);
            }
            Ok(false)
        }
        #[cfg(not(unix))]
        {
            let _ = (path, claim);
            Ok(false)
        }
    }
    pub(super) fn clear_claim(&self, path: &str, claim: &str) -> WorkbenchResult<()> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            valid_id(claim)?;
            let (directory, _) = match self.parent(path, false) {
                Ok(pair) => pair,
                Err(e) if e.code == "file_missing" => return Ok(()),
                Err(e) => return Err(e),
            };
            let claim = std::ffi::CString::new(format!(".pipeline-apply-{claim}")).map_err(err)?;
            let result = unsafe { libc::unlinkat(directory.as_raw_fd(), claim.as_ptr(), 0) };
            if result != 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::NotFound
            {
                return Err(err(std::io::Error::last_os_error()));
            }
            directory.sync_all().map_err(err)
        }
        #[cfg(not(unix))]
        {
            let _ = (path, claim);
            Ok(())
        }
    }
}

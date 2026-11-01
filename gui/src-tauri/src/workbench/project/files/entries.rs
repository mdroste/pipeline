//! Entry metadata, one-directory listings, unbounded read handles, and
//! no-clobber moves. Everything stays descriptor-relative and no link is
//! followed, so a project of any size can be browsed without reading contents.
use super::*;

/// How long a listing that is already full keeps reading for folders.
const FULL_LISTING_TIME: Duration = Duration::from_secs(5);

/// How a directory entry appears in the browser. A link is reported, never followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::workbench::project) enum EntryKind {
    File,
    Folder,
    Link,
    /// FIFOs, sockets and devices, which Windows does not surface in a folder.
    #[cfg_attr(windows, allow(dead_code))]
    Other,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::workbench::project) struct EntryStat {
    pub kind: EntryKind,
    pub size: u64,
    pub modified_ms: i64,
    pub device: u64,
    pub inode: u64,
    pub executable: bool,
}
impl EntryStat {
    /// Changes when the bytes behind a path are replaced or rewritten.
    pub(in crate::workbench::project) fn stamp(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.device, self.inode, self.size, self.modified_ms
        )
    }
}
#[cfg(unix)]
#[allow(clippy::unnecessary_cast)] // stat field widths differ by platform.
fn entry_stat(metadata: &libc::stat) -> EntryStat {
    let kind = match metadata.st_mode as libc::mode_t & libc::S_IFMT {
        libc::S_IFREG => EntryKind::File,
        libc::S_IFDIR => EntryKind::Folder,
        libc::S_IFLNK => EntryKind::Link,
        _ => EntryKind::Other,
    };
    EntryStat {
        kind,
        size: metadata.st_size.max(0) as u64,
        modified_ms: metadata.st_mtime as i64 * 1000 + metadata.st_mtime_nsec as i64 / 1_000_000,
        device: metadata.st_dev as u64,
        inode: metadata.st_ino as u64,
        executable: metadata.st_mode as libc::mode_t & 0o111 != 0,
    }
}
#[cfg(unix)]
fn stat_at(directory: &fs::File, name: &std::ffi::CStr) -> WorkbenchResult<Option<EntryStat>> {
    use std::os::fd::AsRawFd;
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: both pointers remain valid through this synchronous call.
    let result = unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        return if error.kind() == std::io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(err(error))
        };
    }
    // SAFETY: fstatat succeeded, so the buffer is initialized.
    Ok(Some(entry_stat(&unsafe { metadata.assume_init() })))
}
/// Adds one entry to a listing of at most `limit`. A folder takes the place of a
/// file once the listing is full, because a missing folder hides all it holds.
/// False when the listing holds only folders, so nothing later can get in.
pub(super) fn keep_listed(
    entries: &mut Vec<(String, EntryStat)>,
    limit: usize,
    truncated: &mut bool,
    entry: (String, EntryStat),
) -> bool {
    if entries.len() < limit {
        entries.push(entry);
        return true;
    }
    *truncated = true;
    if entry.1.kind != EntryKind::Folder {
        return true;
    }
    match entries
        .iter()
        .rposition(|(_, stat)| stat.kind != EntryKind::Folder)
    {
        Some(file) => {
            entries[file] = entry;
            true
        }
        None => false,
    }
}
impl SafeRoot {
    /// Metadata for the root itself. Adding or removing an entry changes its stamp.
    pub(in crate::workbench::project) fn root_stat(&self) -> WorkbenchResult<EntryStat> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: the descriptor stays open and the pointer stays valid
            // through this synchronous call.
            if unsafe { libc::fstat(self.directory.as_raw_fd(), metadata.as_mut_ptr()) } != 0 {
                return Err(err(std::io::Error::last_os_error()));
            }
            // SAFETY: fstat succeeded, so the buffer is initialized.
            Ok(entry_stat(&unsafe { metadata.assume_init() }))
        }
        #[cfg(windows)]
        {
            windows::handle_stat(&self.directory).map_err(err)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(WorkbenchError::invalid(
                "Scoped file access is unsupported on this platform",
            ))
        }
    }
    /// Metadata for one path, or `None` when it, or any parent, does not exist.
    pub(in crate::workbench::project) fn stat(
        &self,
        path: &str,
    ) -> WorkbenchResult<Option<EntryStat>> {
        #[cfg(unix)]
        {
            let (directory, name) = match self.parent(path, false) {
                Ok(pair) => pair,
                Err(e) if e.code == "file_missing" => return Ok(None),
                Err(e) => return Err(e),
            };
            stat_at(&directory, &name)
        }
        #[cfg(windows)]
        {
            let (directory, name) = match self.windows_parent(path, false) {
                Ok(pair) => pair,
                Err(e) if e.code == "file_missing" => return Ok(None),
                Err(e) => return Err(e),
            };
            windows::stat(&directory, name).map_err(err)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = path;
            Err(WorkbenchError::invalid(
                "Scoped file access is unsupported on this platform",
            ))
        }
    }
    /// One directory, folders first, without reading any file. Reserved names
    /// stay hidden. `directory` is empty for the root. Returns whether the
    /// listing was cut short. A full listing keeps looking for folders only
    /// for a limited time, because a slow volume can hold millions of entries.
    pub(in crate::workbench::project) fn list(
        &self,
        directory: &str,
        limit: usize,
    ) -> WorkbenchResult<(Vec<(String, EntryStat)>, bool)> {
        let started = Instant::now();
        self.list_until(directory, limit, |full| {
            full && started.elapsed() > FULL_LISTING_TIME
        })
    }
    /// `list` that also ends early, as cut short, once `stop` says so. It is
    /// asked before each entry, and told whether the listing has reached `limit`.
    pub(in crate::workbench::project) fn list_until(
        &self,
        directory: &str,
        limit: usize,
        mut stop: impl FnMut(bool) -> bool,
    ) -> WorkbenchResult<(Vec<(String, EntryStat)>, bool)> {
        #[cfg(unix)]
        let (mut entries, truncated) = self.unix_list(directory, limit, &mut stop)?;
        #[cfg(windows)]
        let (mut entries, truncated) = {
            let handle = if directory.is_empty() {
                self.directory.try_clone().map_err(err)?
            } else {
                relative(directory)?;
                windows::open_directory(&self.directory, directory).map_err(|e| {
                    let missing = e.kind() == std::io::ErrorKind::NotFound;
                    let mut error = err(&e);
                    if missing {
                        error.code = "file_missing".into();
                    }
                    error
                })?
            };
            windows::list(&handle, limit, &mut stop).map_err(err)?
        };
        #[cfg(not(any(unix, windows)))]
        let (mut entries, truncated): (Vec<(String, EntryStat)>, bool) = {
            let _ = (directory, limit, &mut stop);
            return Err(WorkbenchError::invalid(
                "Scoped file access is unsupported on this platform",
            ));
        };
        entries.sort_by_cached_key(|(name, stat)| {
            (
                stat.kind != EntryKind::Folder,
                name.to_lowercase(),
                name.clone(),
            )
        });
        Ok((entries, truncated))
    }
    #[cfg(unix)]
    fn unix_list(
        &self,
        directory: &str,
        limit: usize,
        stop: &mut impl FnMut(bool) -> bool,
    ) -> WorkbenchResult<(Vec<(String, EntryStat)>, bool)> {
        use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
        let open_flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        let handle = if directory.is_empty() {
            // A fresh description: a clone would share the root's read offset.
            let name = std::ffi::CString::new(".").map_err(err)?;
            // SAFETY: the descriptor and name are valid for this call.
            unsafe { libc::openat(self.directory.as_raw_fd(), name.as_ptr(), open_flags) }
        } else {
            let (parent, name) = self.parent(directory, false)?;
            // SAFETY: the descriptor and name are valid for this call.
            unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), open_flags) }
        };
        if handle < 0 {
            let e = std::io::Error::last_os_error();
            let mut error = err(&e);
            if e.kind() == std::io::ErrorKind::NotFound {
                error.code = "file_missing".into();
            }
            return Err(error);
        }
        // SAFETY: openat returned a new owned descriptor.
        let directory = unsafe { fs::File::from_raw_fd(handle) };
        let walker = directory.try_clone().map_err(err)?;
        // SAFETY: fdopendir takes ownership of the descriptor on success.
        let stream = unsafe { libc::fdopendir(walker.as_raw_fd()) };
        if stream.is_null() {
            return Err(err(std::io::Error::last_os_error()));
        }
        let _ = walker.into_raw_fd();
        let mut entries = Vec::new();
        let mut truncated = false;
        loop {
            if stop(entries.len() >= limit) {
                truncated = true;
                break;
            }
            // SAFETY: the stream stays open until closedir below; the entry is
            // read before the next readdir call.
            let entry = unsafe { libc::readdir(stream) };
            if entry.is_null() {
                break;
            }
            // SAFETY: readdir returns a NUL-terminated name.
            let raw = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) };
            let Ok(name) = raw.to_str() else { continue };
            if name == "." || name == ".." || reserved_component(name) {
                continue;
            }
            // SAFETY: the entry stays valid until the next readdir call.
            let kind = unsafe { (*entry).d_type };
            // Past the limit only folders still count, and the listing usually
            // says which those are without a stat of every file.
            if entries.len() == limit && !matches!(kind, libc::DT_DIR | libc::DT_UNKNOWN) {
                truncated = true;
                continue;
            }
            // A file removed mid-listing is simply no longer an entry.
            if let Some(stat) = stat_at(&directory, raw)? {
                if !keep_listed(
                    &mut entries,
                    limit,
                    &mut truncated,
                    (name.to_string(), stat),
                ) {
                    break;
                }
            }
        }
        // SAFETY: the stream is open and not used after this call.
        unsafe { libc::closedir(stream) };
        Ok((entries, truncated))
    }
    /// A read handle of any size for one regular file, with its metadata.
    /// Callers bound what they read.
    pub(in crate::workbench::project) fn open_read(
        &self,
        path: &str,
    ) -> WorkbenchResult<Option<(fs::File, EntryStat)>> {
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            let (directory, name) = match self.parent(path, false) {
                Ok(pair) => pair,
                Err(e) if e.code == "file_missing" => return Ok(None),
                Err(e) => return Err(e),
            };
            // SAFETY: the descriptor and name are valid for this call.
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
            // SAFETY: openat returned a new owned descriptor.
            let file = unsafe { fs::File::from_raw_fd(fd) };
            let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: the descriptor is open and the buffer is writable.
            if unsafe { libc::fstat(fd, metadata.as_mut_ptr()) } != 0 {
                return Err(err(std::io::Error::last_os_error()));
            }
            // SAFETY: fstat succeeded, so the buffer is initialized.
            let stat = entry_stat(&unsafe { metadata.assume_init() });
            if stat.kind != EntryKind::File {
                return Err(WorkbenchError::invalid("Only regular files can be opened"));
            }
            Ok(Some((file, stat)))
        }
        #[cfg(windows)]
        {
            relative(path)?;
            let file = match windows::open_relative(&self.directory, path) {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(err(e)),
            };
            let stat = windows::handle_stat(&file).map_err(err)?;
            Ok(Some((file, stat)))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = path;
            Err(WorkbenchError::invalid(
                "Scoped file access is unsupported on this platform",
            ))
        }
    }
    /// Create one folder under an existing parent. An existing entry is refused.
    pub(in crate::workbench::project) fn create_folder(&self, path: &str) -> WorkbenchResult<()> {
        let exists = || WorkbenchError::conflict(format!("Already exists: {path}"));
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let (directory, name) = self.parent(path, false)?;
            // SAFETY: the descriptor and name are valid for this call.
            if unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o755) } != 0 {
                let error = std::io::Error::last_os_error();
                return Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
                    exists()
                } else {
                    err(error)
                });
            }
            directory.sync_all().map_err(err)
        }
        #[cfg(windows)]
        {
            let (directory, name) = self.windows_parent(path, false)?;
            windows::create_directory(&directory, name).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    exists()
                } else {
                    err(error)
                }
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (path, exists);
            Err(WorkbenchError::invalid(
                "Scoped file access is unsupported on this platform",
            ))
        }
    }
    /// Rename or move one entry without replacing anything at `to`. The
    /// destination's parent must already exist.
    pub(in crate::workbench::project) fn move_entry(
        &self,
        from: &str,
        to: &str,
    ) -> WorkbenchResult<()> {
        relative(from)?;
        relative(to)?;
        if to == from || to.starts_with(&format!("{from}/")) {
            return Err(WorkbenchError::invalid(
                "A folder cannot be moved into itself",
            ));
        }
        let source = self
            .stat(from)?
            .ok_or_else(|| WorkbenchError::invalid(format!("Missing file: {from}")))?;
        if !matches!(source.kind, EntryKind::File | EntryKind::Folder) {
            return Err(WorkbenchError::invalid(
                "Only files and folders can be moved",
            ));
        }
        // A rename that changes only letter case finds the entry itself at the
        // destination on a case-insensitive volume, so it moves through a
        // private name instead of being refused as a collision.
        let same_entry = self
            .stat(to)?
            .is_some_and(|target| target.device == source.device && target.inode == source.inode);
        if same_entry {
            let (folder, name) = from.rsplit_once('/').unwrap_or(("", from));
            let (target_folder, _) = to.rsplit_once('/').unwrap_or(("", to));
            let target_name = to.rsplit('/').next().unwrap();
            if folder != target_folder || name.to_lowercase() != target_name.to_lowercase() {
                return Err(WorkbenchError::conflict(format!("Already exists: {to}")));
            }
            let private = format!(".pipeline-apply-{}", id("rename")?);
            let private = if folder.is_empty() {
                private
            } else {
                format!("{folder}/{private}")
            };
            self.move_unchecked(from, &private)?;
            return self.move_unchecked(&private, to).inspect_err(|_| {
                let _ = self.move_unchecked(&private, from);
            });
        }
        self.move_unchecked(from, to)
    }
    /// `from` and `to` are validated by `move_entry`; `to` may be a private name.
    fn move_unchecked(&self, from: &str, to: &str) -> WorkbenchResult<()> {
        let collision = || WorkbenchError::conflict(format!("Already exists: {to}"));
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let (from_directory, from_name) = self.private_parent(from)?;
            let (to_directory, to_name) = self.private_parent(to)?;
            let result = exclusive_rename(
                from_directory.as_raw_fd(),
                &from_name,
                to_directory.as_raw_fd(),
                &to_name,
            );
            if let Err(error) = result {
                return Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
                    collision()
                } else {
                    err(error)
                });
            }
            from_directory.sync_all().map_err(err)?;
            to_directory.sync_all().map_err(err)
        }
        #[cfg(windows)]
        {
            let (from_directory, from_name) = self.windows_private_parent(from)?;
            let (to_directory, to_name) = self.windows_private_parent(to)?;
            windows::move_new(&from_directory, from_name, &to_directory, to_name).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    collision()
                } else {
                    err(error)
                }
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (from, to, collision);
            Err(WorkbenchError::invalid(
                "Scoped file access is unsupported on this platform",
            ))
        }
    }
    /// `parent` walks the folders of a path, so the private last component of a
    /// case-only rename must not reach the reserved-name check in `relative`.
    #[cfg(unix)]
    fn private_parent(&self, path: &str) -> WorkbenchResult<(fs::File, std::ffi::CString)> {
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
        let name = std::ffi::CString::new(name).map_err(err)?;
        if folder.is_empty() {
            return Ok((self.directory.try_clone().map_err(err)?, name));
        }
        // A placeholder last component lets `parent` return the folder itself.
        let (directory, _) = self.parent(&format!("{folder}/x"), false)?;
        Ok((directory, name))
    }
    #[cfg(windows)]
    fn windows_private_parent<'a>(&self, path: &'a str) -> WorkbenchResult<(fs::File, &'a str)> {
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
        if folder.is_empty() {
            return Ok((self.directory.try_clone().map_err(err)?, name));
        }
        let (directory, _) = self.windows_parent(&format!("{folder}/x"), false)?;
        Ok((directory, name))
    }
}
#[cfg(target_os = "macos")]
fn exclusive_rename(
    from_directory: i32,
    from: &std::ffi::CStr,
    to_directory: i32,
    to: &std::ffi::CStr,
) -> std::io::Result<()> {
    // SAFETY: descriptors and names are valid for this synchronous call.
    let result = unsafe {
        libc::renameatx_np(
            from_directory,
            from.as_ptr(),
            to_directory,
            to.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ENOTSUP) {
        return checked_rename(from_directory, from, to_directory, to);
    }
    Err(error)
}
#[cfg(target_os = "linux")]
fn exclusive_rename(
    from_directory: i32,
    from: &std::ffi::CStr,
    to_directory: i32,
    to: &std::ffi::CStr,
) -> std::io::Result<()> {
    // SAFETY: descriptors and names are valid for this synchronous call.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            from_directory,
            from.as_ptr(),
            to_directory,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if matches!(
        error.raw_os_error(),
        Some(libc::EINVAL) | Some(libc::ENOSYS)
    ) {
        return checked_rename(from_directory, from, to_directory, to);
    }
    Err(error)
}
#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn exclusive_rename(
    from_directory: i32,
    from: &std::ffi::CStr,
    to_directory: i32,
    to: &std::ffi::CStr,
) -> std::io::Result<()> {
    checked_rename(from_directory, from, to_directory, to)
}
/// For volumes with no exclusive rename, such as some network shares and exFAT.
/// Files keep no-clobber through a hard link or, where a volume has none, through
/// a placeholder claimed at the destination and renamed over; the project lock
/// keeps other writers out in between. A folder can only replace an empty folder
/// at worst, which holds nothing to lose.
#[cfg(unix)]
fn checked_rename(
    from_directory: i32,
    from: &std::ffi::CStr,
    to_directory: i32,
    to: &std::ffi::CStr,
) -> std::io::Result<()> {
    checked_rename_with(from_directory, from, to_directory, to, || {
        // SAFETY: descriptors and names are valid for this synchronous call.
        match unsafe { libc::linkat(from_directory, from.as_ptr(), to_directory, to.as_ptr(), 0) } {
            0 => Ok(()),
            _ => Err(std::io::Error::last_os_error()),
        }
    })
}
#[cfg(unix)]
fn checked_rename_with(
    from_directory: i32,
    from: &std::ffi::CStr,
    to_directory: i32,
    to: &std::ffi::CStr,
    link: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let linked = link();
    // SAFETY: descriptors and names are valid for these synchronous calls.
    unsafe {
        let link_error = match linked {
            Ok(()) => {
                if libc::unlinkat(from_directory, from.as_ptr(), 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Err(error),
            Err(error) => error,
        };
        let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
        if libc::fstatat(
            to_directory,
            to.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        ) == 0
        {
            return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
        }
        if libc::fstatat(
            from_directory,
            from.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        ) != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let folder = metadata.assume_init().st_mode as libc::mode_t & libc::S_IFMT == libc::S_IFDIR;
        if !folder {
            let unsupported = [libc::ENOTSUP, libc::EOPNOTSUPP, libc::EPERM, libc::ENOSYS];
            if !link_error
                .raw_os_error()
                .is_some_and(|code| unsupported.contains(&code))
            {
                return Err(link_error);
            }
            let placeholder = libc::openat(
                to_directory,
                to.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_WRONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600 as libc::c_uint,
            );
            if placeholder < 0 {
                return Err(std::io::Error::last_os_error());
            }
            libc::close(placeholder);
        }
        if libc::renameat(from_directory, from.as_ptr(), to_directory, to.as_ptr()) != 0 {
            let error = std::io::Error::last_os_error();
            if !folder {
                libc::unlinkat(to_directory, to.as_ptr(), 0);
            }
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::os::fd::AsRawFd;

    /// Moves `from` to `to` in `root` on a volume whose hard links fail with `code`.
    fn rename_without_links(root: &Path, code: i32, from: &str, to: &str) -> std::io::Result<()> {
        let directory = fs::File::open(root).unwrap();
        let (from, to) = (CString::new(from).unwrap(), CString::new(to).unwrap());
        let fd = directory.as_raw_fd();
        checked_rename_with(fd, &from, fd, &to, || {
            Err(std::io::Error::from_raw_os_error(code))
        })
    }

    #[test]
    fn a_file_moves_on_a_volume_with_no_hard_links_and_replaces_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("one.txt"), "one").unwrap();
        fs::write(root.join("two.txt"), "two").unwrap();
        rename_without_links(root, libc::ENOTSUP, "one.txt", "moved.txt").unwrap();
        assert_eq!(fs::read(root.join("moved.txt")).unwrap(), b"one");
        assert!(!root.join("one.txt").exists());
        let collision = rename_without_links(root, libc::ENOTSUP, "moved.txt", "two.txt");
        assert_eq!(
            collision.unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(root.join("two.txt")).unwrap(), b"two");
        assert_eq!(fs::read(root.join("moved.txt")).unwrap(), b"one");
    }

    #[test]
    fn another_link_failure_still_stops_a_file_and_leaves_no_placeholder() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("one.txt"), "one").unwrap();
        let denied = rename_without_links(temp.path(), libc::EACCES, "one.txt", "moved.txt");
        assert_eq!(denied.unwrap_err().raw_os_error(), Some(libc::EACCES));
        assert!(!temp.path().join("moved.txt").exists());
        assert_eq!(fs::read(temp.path().join("one.txt")).unwrap(), b"one");
    }

    #[test]
    fn a_folder_still_moves_when_links_fail() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("folder")).unwrap();
        fs::write(temp.path().join("folder/a.txt"), "a").unwrap();
        rename_without_links(temp.path(), libc::EPERM, "folder", "moved").unwrap();
        assert_eq!(fs::read(temp.path().join("moved/a.txt")).unwrap(), b"a");
    }
}

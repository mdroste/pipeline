//! Single-component, handle-relative opens: no ancestor pathname is re-resolved.
//! Creation, rename, and deletion act on handles relative to a verified parent.
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::{
    ffi::OsStringExt,
    fs::OpenOptionsExt,
    io::{AsRawHandle, FromRawHandle},
};
use std::path::{Path, PathBuf};
use windows_sys::Wdk::{
    Foundation::OBJECT_ATTRIBUTES,
    Storage::FileSystem::{
        FileRenameInformation, NtCreateFile, NtSetInformationFile, FILE_CREATE,
        FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
        FILE_RENAME_INFORMATION, FILE_SYNCHRONOUS_IO_NONALERT,
    },
};
use windows_sys::Win32::{
    Foundation::{
        RtlNtStatusToDosError, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER,
        ERROR_NOT_SUPPORTED, INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, UNICODE_STRING,
    },
    Storage::FileSystem::{
        FileAttributeTagInfo, FileDispositionInfo, FileDispositionInfoEx,
        GetFileInformationByHandleEx, GetFinalPathNameByHandleW, ReOpenFile,
        SetFileInformationByHandle, DELETE, FILE_ATTRIBUTE_TAG_INFO, FILE_DISPOSITION_FLAG_DELETE,
        FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
        FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY,
        FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, SYNCHRONIZE,
    },
    System::IO::IO_STATUS_BLOCK,
};

const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "Project files cannot traverse links or metadata",
    )
}

fn check_reparse(file: &File) -> io::Result<bool> {
    let mut tag = FILE_ATTRIBUTE_TAG_INFO::default();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileAttributeTagInfo,
            (&mut tag as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            std::mem::size_of_val(&tag) as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // Only Cloud Files placeholders may be hydrated. Refuse name-surrogate
    // tags (junctions/symlinks) and unknown reparse providers, including at root.
    const CLOUD_TAG: u32 = 0x9000_001a;
    const CLOUD_VARIANTS: u32 = 0x0000_f000;
    if tag.ReparseTag != 0 && tag.ReparseTag & !CLOUD_VARIANTS != CLOUD_TAG {
        return Err(invalid());
    }
    Ok(tag.ReparseTag != 0)
}

pub(super) fn open_root(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if !file.metadata()?.file_type().is_dir() {
        return Err(invalid());
    }
    check_reparse(&file)?;
    Ok(file)
}

fn final_path(file: &File) -> io::Result<PathBuf> {
    let mut buffer = vec![0u16; 512];
    loop {
        // SAFETY: the handle is live and buffer is writable for its stated length.
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_NORMALIZED,
            )
        };
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if (length as usize) < buffer.len() {
            buffer.truncate(length as usize);
            return Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer)));
        }
        buffer.resize(length as usize + 1, 0);
    }
}

/// One name relative to a directory handle: an empty name would reopen the
/// directory itself, and separators or a colon would reach another path or a stream.
fn component(name: &str) -> io::Result<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) {
        return Err(invalid());
    }
    Ok(())
}

fn nt_open(
    parent: &File,
    name: &str,
    access: u32,
    share: u32,
    disposition: u32,
    options: u32,
) -> io::Result<File> {
    component(name)?;
    let mut encoded: Vec<u16> = name.encode_utf16().collect();
    let length = u16::try_from(encoded.len() * 2).map_err(|_| invalid())?;
    let object_name = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: encoded.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &object_name,
        Attributes: OBJ_CASE_INSENSITIVE,
        ..Default::default()
    };
    let mut handle = std::ptr::null_mut();
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: all pointers remain live through this synchronous call; successful
    // handles are immediately transferred to File for exactly-once ownership.
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            access,
            &attributes,
            &mut status_block,
            std::ptr::null(),
            0,
            share,
            disposition,
            FILE_SYNCHRONOUS_IO_NONALERT | options,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(nt_error(status));
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}

fn nt_error(status: i32) -> io::Error {
    io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(status) } as i32)
}

/// Checking the long name from the opened object also catches DOS 8.3 aliases.
/// App-owned recovery names are reachable only when requested exactly.
fn permitted(resolved: &Path, internal: Option<&str>) -> bool {
    let Some(name) = resolved.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    match internal {
        Some(expected) => name.eq_ignore_ascii_case(expected),
        None => !super::reserved_component(name),
    }
}

/// Reopen the verified *object*, not its pathname, so cloud filters can
/// hydrate it without introducing a second path-resolution race.
fn reopen(file: &File, resolved: &Path, access: u32, share: u32) -> io::Result<File> {
    let handle = unsafe { ReOpenFile(file.as_raw_handle(), access, share, 0) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let reopened = unsafe { File::from_raw_handle(handle) };
    check_reparse(&reopened)?;
    if !reopened.metadata()?.is_file() || final_path(&reopened)? != resolved {
        return Err(invalid());
    }
    Ok(reopened)
}

fn open_child(parent: &File, name: &str, directory: bool) -> io::Result<File> {
    let kind = if directory {
        FILE_DIRECTORY_FILE
    } else {
        FILE_NON_DIRECTORY_FILE
    };
    let file = nt_open(
        parent,
        name,
        FILE_GENERIC_READ,
        SHARE_ALL,
        FILE_OPEN,
        FILE_OPEN_REPARSE_POINT | kind,
    )?;
    let cloud = check_reparse(&file)?;
    let kind = file.metadata()?.file_type();
    if (directory && !kind.is_dir()) || (!directory && !kind.is_file()) {
        return Err(invalid());
    }
    let resolved = final_path(&file)?;
    if !permitted(&resolved, None) {
        return Err(invalid());
    }
    if !directory && cloud {
        return reopen(&file, &resolved, FILE_GENERIC_READ, SHARE_ALL);
    }
    Ok(file)
}

pub(super) fn open_relative(root: &File, path: &str) -> io::Result<File> {
    let mut parent = root.try_clone()?;
    let mut parts = path.split('/').peekable();
    while let Some(name) = parts.next() {
        parent = open_child(&parent, name, parts.peek().is_some())?;
    }
    Ok(parent)
}

/// Walk to the directory holding the last component of a validated relative
/// path, creating missing directories when asked. Each step is verified as a
/// real directory before it becomes the next parent handle.
pub(super) fn open_parent<'a>(
    root: &File,
    path: &'a str,
    create: bool,
) -> io::Result<(File, &'a str)> {
    let mut directory = root.try_clone()?;
    let mut parts = path.split('/').peekable();
    while let Some(name) = parts.next() {
        if parts.peek().is_none() {
            return Ok((directory, name));
        }
        if create {
            match nt_open(
                &directory,
                name,
                FILE_LIST_DIRECTORY | SYNCHRONIZE,
                SHARE_ALL,
                FILE_CREATE,
                FILE_DIRECTORY_FILE,
            ) {
                Ok(_) => (),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error),
            }
        }
        directory = open_child(&directory, name, true)?;
    }
    Err(invalid())
}

/// Whether any entry, including a link or directory, has this name.
pub(super) fn exists(parent: &File, name: &str) -> io::Result<bool> {
    match nt_open(
        parent,
        name,
        FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        SHARE_ALL,
        FILE_OPEN,
        FILE_OPEN_REPARSE_POINT,
    ) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Open a regular file to verify its bytes and then rename or delete it. The
/// share mode admits other readers only, so no writer can change the file
/// between verification and the handle-relative rename.
pub(super) fn open_exclusive(parent: &File, name: &str, internal: bool) -> io::Result<File> {
    let probe = nt_open(
        parent,
        name,
        FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        SHARE_ALL,
        FILE_OPEN,
        FILE_OPEN_REPARSE_POINT | FILE_NON_DIRECTORY_FILE,
    )?;
    check_reparse(&probe)?;
    if !probe.metadata()?.is_file() {
        return Err(invalid());
    }
    let resolved = final_path(&probe)?;
    if !permitted(&resolved, internal.then_some(name)) {
        return Err(invalid());
    }
    // The probe holds no data access, so this stricter reopen cannot conflict
    // with it; any other open writer or non-delete-sharing reader refuses it.
    reopen(
        &probe,
        &resolved,
        FILE_GENERIC_READ | DELETE,
        FILE_SHARE_READ,
    )
}

/// Create a new app-owned file for writing. Any existing entry, including a
/// dangling link, makes this fail instead of being followed or replaced.
pub(super) fn create_new(parent: &File, name: &str) -> io::Result<File> {
    nt_open(
        parent,
        name,
        FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE,
        FILE_SHARE_READ,
        FILE_CREATE,
        FILE_NON_DIRECTORY_FILE,
    )
}

/// Rename an open file within its current directory. An existing entry with
/// the new name is never replaced. This uses the NT call because the Win32
/// wrapper resolves a bare name against the process's current directory.
pub(super) fn rename_new(file: &File, name: &str) -> io::Result<()> {
    component(name)?;
    let encoded: Vec<u16> = name.encode_utf16().collect();
    let name_bytes = encoded.len() * 2;
    let size = std::mem::size_of::<FILE_RENAME_INFORMATION>() + name_bytes;
    // Zeroed u64 storage is suitably aligned and already says: do not replace
    // an existing entry, and with no root directory and a single component,
    // rename within the file's own directory.
    let mut buffer = vec![0u64; size.div_ceil(8)];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    // SAFETY: the buffer is large enough for the header and the name with a
    // trailing NUL; the name pointer derives from the whole buffer.
    unsafe {
        (*info).FileNameLength = u32::try_from(name_bytes).map_err(|_| invalid())?;
        let target = buffer
            .as_mut_ptr()
            .cast::<u8>()
            .add(std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName))
            .cast::<u16>();
        std::ptr::copy_nonoverlapping(encoded.as_ptr(), target, encoded.len());
    }
    let mut status_block = IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut status_block,
            buffer.as_ptr().cast(),
            u32::try_from(size).map_err(|_| invalid())?,
            FileRenameInformation,
        )
    };
    if status < 0 {
        return Err(nt_error(status));
    }
    Ok(())
}

/// Delete an open file. POSIX semantics remove the name at once; file systems
/// without them fall back to removal when the last handle closes.
pub(super) fn delete(file: &File) -> io::Result<()> {
    let info = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE
            | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
            | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    };
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfoEx,
            (&info as *const FILE_DISPOSITION_INFO_EX).cast(),
            std::mem::size_of_val(&info) as u32,
        )
    };
    if ok != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    let unsupported = [
        ERROR_INVALID_PARAMETER,
        ERROR_NOT_SUPPORTED,
        ERROR_INVALID_FUNCTION,
    ];
    if !error
        .raw_os_error()
        .is_some_and(|code| unsupported.contains(&(code as u32)))
    {
        return Err(error);
    }
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&info as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of_val(&info) as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn junction(link: &Path, target: &Path) {
        let status = std::process::Command::new("cmd")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn junctions_are_refused_and_replaced_roots_do_not_redirect_open_handles() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let outside = temp.path().join("outside");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(root.join("nested/file.txt"), "inside").unwrap();
        fs::write(outside.join("file.txt"), "outside").unwrap();
        junction(&root.join("escape"), &outside);
        junction(&root.join("alias"), &root.join("nested"));
        let safe = super::super::SafeRoot::open(&root).unwrap();
        assert_eq!(safe.read("nested/file.txt").unwrap(), b"inside");
        assert!(safe.read("escape/file.txt").is_err());
        assert!(safe.read("alias/file.txt").is_err());
        match std::os::windows::fs::symlink_dir(&outside, root.join("symlink")) {
            Ok(()) => assert!(safe.read("symlink/file.txt").is_err()),
            // Symlink creation needs Developer Mode or SeCreateSymbolicLinkPrivilege.
            // Junction coverage above always runs without either prerequisite.
            Err(error) if error.raw_os_error() == Some(1314) => {}
            Err(error) => panic!("Failed to create directory-symlink fixture: {error}"),
        }
        match std::os::windows::fs::symlink_file(outside.join("file.txt"), root.join("linked.txt"))
        {
            Ok(()) => assert!(safe.read("linked.txt").is_err()),
            Err(error) if error.raw_os_error() == Some(1314) => {}
            Err(error) => panic!("Failed to create file-symlink fixture: {error}"),
        }
        assert_eq!(safe.optional_read("missing/file.txt").unwrap(), None);
        let parent = open_child(&safe.directory, "nested", true).unwrap();
        fs::rename(root.join("nested"), root.join("moved")).unwrap();
        junction(&root.join("nested"), &outside);
        assert!(safe.read("nested/file.txt").is_err());
        let mut original = open_child(&parent, "file.txt", false).unwrap();
        let mut text = String::new();
        std::io::Read::read_to_string(&mut original, &mut text).unwrap();
        assert_eq!(text, "inside");
        // Windows may refuse to rename an ancestor while descendant handles
        // remain open. Their replacement check is complete; retain only the
        // root handle for the independent root-replacement scenario below.
        drop(original);
        drop(parent);
        fs::rename(&root, temp.path().join("moved-project")).unwrap();
        junction(&root, &outside);
        assert_eq!(safe.read("moved/file.txt").unwrap(), b"inside");
        // Junction removal must not recursively traverse the target.
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn replacement_is_handle_relative_no_clobber_and_refuses_open_writers() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let outside = temp.path().join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        junction(&root.join("escape"), &outside);
        let safe = super::super::SafeRoot::open(&root).unwrap();
        let hash = super::super::super::hash;
        let first = hash(b"first");
        // New files create their folders; links and existing names are refused.
        safe.replace("notes/new.md", None, false, Some(b"first"), false, "new")
            .unwrap();
        safe.clear_claim("notes/new.md", "new").unwrap();
        assert_eq!(fs::read(root.join("notes/new.md")).unwrap(), b"first");
        assert!(safe
            .replace("escape/new.md", None, false, Some(b"bad"), false, "escape")
            .is_err());
        assert!(!outside.join("new.md").exists());
        assert!(safe
            .replace("notes/new.md", None, false, Some(b"bad"), false, "again")
            .is_err());
        // Stale bytes, a recorded executable bit, or an open writer change nothing.
        let stale = hash(b"other");
        let error = safe
            .replace(
                "notes/new.md",
                Some(&stale),
                false,
                Some(b"x"),
                false,
                "stale",
            )
            .unwrap_err();
        assert!(error.message.contains("External change"));
        assert!(safe
            .replace(
                "notes/new.md",
                Some(&first),
                true,
                Some(b"x"),
                false,
                "mode"
            )
            .is_err());
        let writer = fs::OpenOptions::new()
            .write(true)
            .open(root.join("notes/new.md"))
            .unwrap();
        let error = safe
            .replace(
                "notes/new.md",
                Some(&first),
                false,
                Some(b"x"),
                false,
                "busy",
            )
            .unwrap_err();
        assert!(error.message.contains("open in another program"));
        drop(writer);
        assert_eq!(fs::read(root.join("notes/new.md")).unwrap(), b"first");
        // The original stays under its claim until it is cleared or recovered.
        safe.replace(
            "notes/new.md",
            Some(&first),
            false,
            Some(b"second"),
            false,
            "edit",
        )
        .unwrap();
        assert_eq!(fs::read(root.join("notes/new.md")).unwrap(), b"second");
        let claim = root.join("notes/.pipeline-apply-edit");
        assert_eq!(fs::read(&claim).unwrap(), b"first");
        assert!(!safe.recover_claim("notes/new.md", "edit").unwrap());
        fs::remove_file(root.join("notes/new.md")).unwrap();
        assert!(safe.recover_claim("notes/new.md", "edit").unwrap());
        assert_eq!(fs::read(root.join("notes/new.md")).unwrap(), b"first");
        assert!(!claim.exists());
        // Deletion moves the file to its claim; clearing the claim removes it.
        safe.replace("notes/new.md", Some(&first), false, None, false, "delete")
            .unwrap();
        assert!(!root.join("notes/new.md").exists());
        safe.clear_claim("notes/new.md", "delete").unwrap();
        assert_eq!(fs::read_dir(root.join("notes")).unwrap().count(), 0);
    }
}

//! Single-component, handle-relative opens: no ancestor pathname is re-resolved.
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
        NtCreateFile, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
        FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
    },
};
use windows_sys::Win32::{
    Foundation::{
        RtlNtStatusToDosError, INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, UNICODE_STRING,
    },
    Storage::FileSystem::{
        FileAttributeTagInfo, GetFileInformationByHandleEx, GetFinalPathNameByHandleW, ReOpenFile,
        FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_GENERIC_READ, FILE_NAME_NORMALIZED, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE,
    },
    System::IO::IO_STATUS_BLOCK,
};

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

fn open_child(parent: &File, name: &str, directory: bool) -> io::Result<File> {
    // The caller supplies one validated component, never an NT path or stream.
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
            FILE_GENERIC_READ,
            &attributes,
            &mut status_block,
            std::ptr::null(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
            FILE_SYNCHRONOUS_IO_NONALERT
                | FILE_OPEN_REPARSE_POINT
                | if directory {
                    FILE_DIRECTORY_FILE
                } else {
                    FILE_NON_DIRECTORY_FILE
                },
            std::ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(status) } as i32,
        ));
    }
    let file = unsafe { File::from_raw_handle(handle) };
    let cloud = check_reparse(&file)?;
    let kind = file.metadata()?.file_type();
    if (directory && !kind.is_dir()) || (!directory && !kind.is_file()) {
        return Err(invalid());
    }
    // Checking the long name from the opened object also catches DOS 8.3 aliases.
    let resolved = final_path(&file)?;
    if resolved
        .file_name()
        .and_then(|s| s.to_str())
        .is_none_or(super::reserved_component)
    {
        return Err(invalid());
    }
    if !directory && cloud {
        // Reopen the verified *object*, not its pathname, so cloud filters can
        // hydrate it without introducing a second path-resolution race.
        let handle = unsafe {
            ReOpenFile(
                file.as_raw_handle(),
                FILE_GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                0,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let hydrated = unsafe { File::from_raw_handle(handle) };
        if !hydrated.metadata()?.is_file() || final_path(&hydrated)? != resolved {
            return Err(invalid());
        }
        return Ok(hydrated);
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
        fs::rename(&root, temp.path().join("moved-project")).unwrap();
        junction(&root, &outside);
        assert_eq!(safe.read("moved/file.txt").unwrap(), b"inside");
        // Junction removal must not recursively traverse the target.
        fs::remove_dir(&root).unwrap();
    }
}

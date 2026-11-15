//! Native single-component opens retain the parent handle through resolution.
//! Only Cloud Files placeholders may be hydrated; links/junctions are refused.
use std::os::windows::{
    ffi::OsStrExt,
    fs::OpenOptionsExt,
    io::{AsRawHandle, FromRawHandle},
};
use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};
use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            NtCreateFile, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
            FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
        },
    },
    Win32::{
        Foundation::{
            RtlNtStatusToDosError, INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, UNICODE_STRING,
        },
        Storage::FileSystem::{
            FileAttributeTagInfo, GetFileInformationByHandleEx, ReOpenFile,
            FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_GENERIC_READ, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        },
        System::IO::IO_STATUS_BLOCK,
    },
};

const SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "File access cannot traverse links or junctions",
    )
}

fn cloud_placeholder(file: &File) -> io::Result<bool> {
    let mut tag = FILE_ATTRIBUTE_TAG_INFO::default();
    // SAFETY: the live handle and writable output structure span the call.
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
    if !file.metadata()?.is_dir() {
        return Err(invalid());
    }
    cloud_placeholder(&file)?;
    Ok(file)
}

pub(super) fn open_child(
    parent: &File,
    name: &std::ffi::OsStr,
    directory: bool,
) -> io::Result<File> {
    let mut encoded: Vec<u16> = name.encode_wide().collect();
    if encoded.is_empty()
        || encoded
            .iter()
            .any(|ch| [0, b'/' as u16, b'\\' as u16, b':' as u16].contains(ch))
    {
        return Err(invalid());
    }
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
    // SAFETY: pointers and parent remain live for this synchronous call. The
    // new handle transfers directly to File and is closed exactly once.
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            FILE_GENERIC_READ,
            &attributes,
            &mut status_block,
            std::ptr::null(),
            0,
            SHARE,
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
    let cloud = cloud_placeholder(&file)?;
    let kind = file.metadata()?.file_type();
    if (directory && !kind.is_dir()) || (!directory && !kind.is_file()) {
        return Err(invalid());
    }
    if !directory && cloud {
        // Reopen the checked object, not its path, to permit cloud hydration
        // without a second path-resolution race.
        let handle = unsafe { ReOpenFile(file.as_raw_handle(), FILE_GENERIC_READ, SHARE, 0) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let hydrated = unsafe { File::from_raw_handle(handle) };
        cloud_placeholder(&hydrated)?;
        if !hydrated.metadata()?.is_file() {
            return Err(invalid());
        }
        return Ok(hydrated);
    }
    Ok(file)
}

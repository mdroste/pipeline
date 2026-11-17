//! Copy into new disposable destinations using checked, open source handles.
//! No writes to source files or their ACLs. This is not a publication protocol.
use std::{
    fs::{File, OpenOptions},
    io,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
const OPEN_REPARSE_POINT: u32 = 0x00200000;
const BACKUP_SEMANTICS: u32 = 0x02000000;
const SHARE_READ: u32 = 1;

fn source(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(SHARE_READ)
        .custom_flags(OPEN_REPARSE_POINT | BACKUP_SEMANTICS)
        .open(path)?;
    if file.metadata()?.file_attributes() & 0x400 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "reparse-point source",
        ));
    }
    Ok(file)
}

pub fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    let root = source(from).map_err(|e| format!("staging source {}: {e}", from.display()))?;
    if !root.metadata().map_err(|e| e.to_string())?.is_dir() {
        return Err("staging source is not a directory".into());
    }
    directory(from, to, root)
        .map_err(|e| format!("staging {} -> {}: {e}", from.display(), to.display()))
}

fn directory(from: &Path, to: &Path, _source_guard: File) -> io::Result<()> {
    // A destination must be fresh. Never merge or overwrite a previous tree.
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_name() == "__pycache__" {
            continue;
        }
        let path = entry.path();
        let mut input = source(&path)?;
        let kind = input.metadata()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            directory(&path, &target, input)?;
        } else if kind.is_file() {
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)?;
            io::copy(&mut input, &mut output)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported source file type",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(format!(
                    "staging-test-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let target = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .canonicalize()
                .unwrap();
            let root = self.0.canonicalize().unwrap();
            assert!(
                root.starts_with(target)
                    && root
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("staging-test-")
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn copies_binary_files_and_refuses_existing_destination() {
        let f = Fixture::new();
        let src = f.0.join("source");
        let dst = f.0.join("copy");
        std::fs::create_dir(&src).unwrap();
        std::fs::create_dir(src.join("nested")).unwrap();
        let bytes: Vec<u8> = (0..100000).map(|i| (i % 251) as u8).collect();
        std::fs::write(src.join("nested/data.bin"), &bytes).unwrap();
        copy_tree(&src, &dst).unwrap();
        assert_eq!(std::fs::read(dst.join("nested/data.bin")).unwrap(), bytes);
        assert!(copy_tree(&src, &dst).is_err());
    }
    #[test]
    fn source_handle_prevents_write_and_replacement_during_copy() {
        let f = Fixture::new();
        let path = f.0.join("source.bin");
        std::fs::write(&path, b"original").unwrap();
        let guard = source(&path).unwrap();
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(std::fs::rename(&path, f.0.join("renamed.bin")).is_err());
        drop(guard);
        std::fs::rename(&path, f.0.join("renamed.bin")).unwrap();
    }
    #[test]
    fn rejects_directory_junction_without_copying_its_target() {
        use std::os::windows::io::AsRawHandle;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn DeviceIoControl(
                handle: *mut std::ffi::c_void,
                code: u32,
                input: *const u8,
                input_len: u32,
                output: *mut u8,
                output_len: u32,
                returned: *mut u32,
                overlapped: *mut std::ffi::c_void,
            ) -> i32;
        }
        let f = Fixture::new();
        let src = f.0.join("source");
        let external = f.0.join("external");
        let link = src.join("link");
        std::fs::create_dir(&src).unwrap();
        std::fs::create_dir(&external).unwrap();
        std::fs::create_dir(&link).unwrap();
        std::fs::write(external.join("canary"), b"outside").unwrap();
        let print = external
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .to_owned();
        let substitute: Vec<u16> = format!("\\??\\{print}").encode_utf16().collect();
        let print: Vec<u16> = print.encode_utf16().collect();
        let data_len = 8 + (substitute.len() + print.len() + 2) * 2;
        let mut data = Vec::new();
        data.extend(0xa0000003u32.to_le_bytes());
        data.extend((data_len as u16).to_le_bytes());
        data.extend(0u16.to_le_bytes());
        for value in [
            0,
            substitute.len() * 2,
            (substitute.len() + 1) * 2,
            print.len() * 2,
        ] {
            data.extend((value as u16).to_le_bytes());
        }
        for value in substitute.into_iter().chain([0]).chain(print).chain([0]) {
            data.extend(value.to_le_bytes());
        }
        let handle = OpenOptions::new()
            .write(true)
            .custom_flags(OPEN_REPARSE_POINT | BACKUP_SEMANTICS)
            .open(&link)
            .unwrap();
        let mut returned = 0;
        let ok = unsafe {
            DeviceIoControl(
                handle.as_raw_handle(),
                0x900a4,
                data.as_ptr(),
                data.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0, "{}", io::Error::last_os_error());
        drop(handle);
        let result = copy_tree(&src, &f.0.join("copy"));
        assert!(result.is_err());
        assert!(!f.0.join("copy/link/canary").exists());
        assert_eq!(std::fs::read(external.join("canary")).unwrap(), b"outside");
        // Remove only the junction itself; keep its target for fixture cleanup.
        std::fs::remove_dir(&link).unwrap();
    }
}

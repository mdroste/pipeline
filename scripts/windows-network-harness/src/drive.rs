//! Temporary per-logon DOS alias for the harness's own disposable profile only.
//! Never a security boundary; target ACLs still apply. Exact-match cleanup.
use std::{
    io::{Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use windows_sys::Win32::{Foundation::*, System::Threading::*};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLogicalDrives() -> u32;
    fn QueryDosDeviceW(name: *const u16, target: *mut u16, capacity: u32) -> u32;
    fn DefineDosDeviceW(flags: u32, name: *const u16, target: *const u16) -> i32;
}
pub struct Drive {
    owner: Child,
    pub root: PathBuf,
}
impl Drive {
    pub fn create(profile: &Path) -> Result<Self, String> {
        let mut owner = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("drive-owner")
            .arg(profile)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut output = owner.stdout.take().ok_or("drive owner stdout")?;
        let (send, receive) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut ready = [0; 4];
            let result = output.read_exact(&mut ready).map(|_| ready);
            let _ = send.send(result);
        });
        let ready = receive.recv_timeout(Duration::from_secs(5));
        let root = match ready {
            Ok(Ok([letter, b':', b'\\', b'\n'])) if (b'H'..=b'Z').contains(&letter) => {
                PathBuf::from(format!("{}:\\", letter as char))
            }
            _ => {
                finish_owner(&mut owner);
                if reader.is_finished() {
                    let _ = reader.join();
                }
                return Err("drive owner did not become ready".into());
            }
        };
        reader
            .join()
            .map_err(|_| "drive readiness reader panicked")?;
        Ok(Self { owner, root })
    }
}
impl Drop for Drive {
    fn drop(&mut self) {
        finish_owner(&mut self.owner);
    }
}
fn finish_owner(owner: &mut Child) {
    // The writer stays in the host, outside the command's inherited-handle
    // list. Host termination also closes it, triggering the same cleanup.
    drop(owner.stdin.take());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match owner.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    eprintln!("drive owner cleanup failed: {status}");
                }
                break;
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                eprintln!("drive owner cleanup deadline; owner remains responsible for alias");
                break;
            }
        }
    }
}

/// Small ordinary-user helper. Its entire lifetime is tied to stdin's pipe.
pub fn owner(profile: &Path) -> Result<(), String> {
    let token = crate::identity::token()?;
    if token["elevated"] != false || token["appcontainer"] != false {
        return Err("drive owner requires the ordinary host token".into());
    }
    let path = profile.canonicalize().map_err(|e| e.to_string())?;
    let packages = PathBuf::from(std::env::var("LOCALAPPDATA").map_err(|e| e.to_string())?)
        .join("Packages")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let package = path.parent().ok_or("missing package directory")?;
    let name = package
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid package name")?;
    let suffix = name
        .to_ascii_lowercase()
        .strip_prefix("pipeline.networkprobe.")
        .map(str::to_owned)
        .ok_or("not a harness profile")?;
    if package.parent() != Some(packages.as_path())
        || path.file_name().and_then(|n| n.to_str()) != Some("AC")
        || suffix.split('.').count() != 2
        || suffix
            .split('.')
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("drive target is not a disposable harness profile".into());
    }
    let alias = Alias::create(&path)?;
    println!("{}", alias.root.display());
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut byte = [0; 1];
    while std::io::stdin()
        .read(&mut byte)
        .map_err(|e| e.to_string())?
        != 0
    {}
    drop(alias);
    Ok(())
}

struct Allocation(HANDLE);
impl Allocation {
    fn lock() -> Result<Self, String> {
        unsafe {
            let handle = CreateMutexW(
                std::ptr::null(),
                0,
                crate::identity::wide("Local\\Pipeline.NetworkHarness.DriveAllocator").as_ptr(),
            );
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            match WaitForSingleObject(handle, 5000) {
                WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self(handle)),
                _ => {
                    CloseHandle(handle);
                    Err("drive allocation mutex deadline".into())
                }
            }
        }
    }
}
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

struct Alias {
    name: Vec<u16>,
    target: Vec<u16>,
    pub root: PathBuf,
}
pub(crate) fn exists(root: &Path) -> Result<bool, String> {
    let name = root
        .to_str()
        .ok_or("invalid drive name")?
        .trim_end_matches('\\');
    if name.len() != 2 || name.as_bytes()[1] != b':' {
        return Err("invalid drive root".into());
    }
    let mut buffer = vec![0u16; 32768];
    let result = unsafe {
        QueryDosDeviceW(
            crate::identity::wide(name).as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    };
    if result != 0 {
        Ok(true)
    } else if std::io::Error::last_os_error().raw_os_error() == Some(2) {
        Ok(false)
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}
impl Alias {
    pub fn create(profile: &Path) -> Result<Self, String> {
        let _allocation = Allocation::lock()?;
        let absolute = std::fs::canonicalize(profile).map_err(|e| e.to_string())?;
        let text = absolute.to_string_lossy();
        let target = crate::identity::wide(&format!(
            "\\??\\{}",
            text.strip_prefix("\\\\?\\").unwrap_or(&text)
        ));
        unsafe {
            let mask = GetLogicalDrives();
            if mask == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            for letter in (b'H'..=b'Z').rev() {
                if mask & (1 << (letter - b'A')) != 0 {
                    continue;
                }
                let name = crate::identity::wide(&format!("{}:", letter as char));
                let mut existing = vec![0u16; 32768];
                if QueryDosDeviceW(name.as_ptr(), existing.as_mut_ptr(), existing.len() as u32) != 0
                {
                    continue;
                }
                if std::io::Error::last_os_error().raw_os_error() != Some(2) {
                    continue;
                }
                if DefineDosDeviceW(1 | 8, name.as_ptr(), target.as_ptr()) == 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                let alias = Self {
                    name,
                    target,
                    root: PathBuf::from(format!("{}:\\", letter as char)),
                };
                let mut actual = vec![0u16; 32768];
                let size = QueryDosDeviceW(
                    alias.name.as_ptr(),
                    actual.as_mut_ptr(),
                    actual.len() as u32,
                );
                if size == 0 || !actual.starts_with(&alias.target) {
                    return Err("DOS alias verification failed".into());
                }
                return Ok(alias);
            }
        }
        Err("No unused drive letter for observational probe".into())
    }
}
impl Drop for Alias {
    fn drop(&mut self) {
        unsafe {
            let result = DefineDosDeviceW(1 | 2 | 4 | 8, self.name.as_ptr(), self.target.as_ptr());
            let mut buffer = vec![0u16; 32768];
            let remaining =
                QueryDosDeviceW(self.name.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32);
            eprintln!(
                "drive_cleanup root={} removed={} remains={remaining}",
                self.root.display(),
                result != 0
            );
        }
    }
}

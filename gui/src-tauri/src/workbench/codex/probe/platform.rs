//! Fixture commands and process cleanup checks for each platform.

use std::path::Path;
use std::time::Duration;

#[cfg(unix)]
pub(super) fn read_file_command(path: &Path) -> Vec<String> {
    vec!["/bin/cat".to_string(), path.display().to_string()]
}

#[cfg(unix)]
pub(super) fn write_file_command(path: &Path) -> Vec<String> {
    vec!["/usr/bin/touch".to_string(), path.display().to_string()]
}

#[cfg(unix)]
pub(super) fn long_running_command() -> Vec<String> {
    vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "printf 'PIPELINE_PROBE_PID:%s:END\\n' \"$$\"; exec /bin/sleep 30".to_string(),
    ]
}

#[cfg(unix)]
pub(super) fn stdin_echo_command() -> Vec<String> {
    vec!["/bin/cat".to_string()]
}

#[cfg(unix)]
fn process_is_running(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    let result = unsafe { libc::kill(pid as i32, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
pub(super) fn read_file_command(path: &Path) -> Vec<String> {
    vec![
        "cmd.exe".to_string(),
        "/D".to_string(),
        "/C".to_string(),
        "type".to_string(),
        path.display().to_string(),
    ]
}

#[cfg(not(unix))]
pub(super) fn write_file_command(path: &Path) -> Vec<String> {
    vec![
        "powershell.exe".to_string(),
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        format!(
            "$ErrorActionPreference='Stop'; Set-Content -LiteralPath '{}' -Value probe",
            path.display().to_string().replace('\'', "''")
        ),
    ]
}

#[cfg(not(unix))]
pub(super) fn long_running_command() -> Vec<String> {
    vec![
        "powershell.exe".to_string(),
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        "[Console]::Out.WriteLine('PIPELINE_PROBE_PID:' + $PID + ':END'); Start-Sleep -Seconds 30"
            .to_string(),
    ]
}

#[cfg(not(unix))]
pub(super) fn stdin_echo_command() -> Vec<String> {
    vec![
        "powershell.exe".to_string(),
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        "[Console]::Out.Write([Console]::In.ReadToEnd())".to_string(),
    ]
}

/// Capture the live fixture before requesting termination. Keeping its Windows
/// handle avoids confusing PID reuse or access errors with successful cleanup.
pub(super) struct ProbeProcess {
    pub pid: u32,
    #[cfg(windows)]
    handle: std::os::windows::io::OwnedHandle,
}

impl ProbeProcess {
    pub fn observe(pid: u32) -> Result<Self, String> {
        if pid == 0 || pid > i32::MAX as u32 {
            return Err("Invalid fixture process id".into());
        }
        #[cfg(windows)]
        let handle = {
            use std::os::windows::io::FromRawHandle as _;
            use windows_sys::Win32::System::Threading::{
                OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
            };
            let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, pid) };
            if raw.is_null() {
                return Err(format!(
                    "Cannot observe fixture process: {}",
                    std::io::Error::last_os_error()
                ));
            }
            unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(raw) }
        };
        Ok(Self {
            pid,
            #[cfg(windows)]
            handle,
        })
    }

    pub fn terminate(&self) {
        #[cfg(unix)]
        unsafe {
            libc::kill(self.pid as i32, libc::SIGKILL);
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle as _;
            unsafe {
                windows_sys::Win32::System::Threading::TerminateProcess(
                    self.handle.as_raw_handle(),
                    1,
                );
            }
        }
    }

    fn stopped(&self) -> Result<bool, String> {
        #[cfg(unix)]
        {
            Ok(!process_is_running(self.pid))
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle as _;
            use windows_sys::Win32::{
                Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
                System::Threading::WaitForSingleObject,
            };
            match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) } {
                WAIT_OBJECT_0 => Ok(true),
                WAIT_TIMEOUT => Ok(false),
                _ => Err(format!(
                    "Cannot inspect fixture process: {}",
                    std::io::Error::last_os_error()
                )),
            }
        }
    }

    pub async fn wait_stopped(&self) -> Result<bool, String> {
        for _ in 0..20 {
            if self.stopped()? {
                return Ok(true);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        Ok(false)
    }
}

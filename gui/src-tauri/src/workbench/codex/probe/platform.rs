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
        "printf '%s\\n' \"$$\"; exec /bin/sleep 30".to_string(),
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

#[cfg(unix)]
pub(super) fn terminate_probe_process(pid: u32) {
    if pid > 0 && pid <= i32::MAX as u32 {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
}

#[cfg(unix)]
fn active_command_cleanup_verified(pid: u32) -> Result<bool, String> {
    Ok(!process_is_running(pid))
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
        "New-Item -ItemType File -LiteralPath $args[0]".to_string(),
        path.display().to_string(),
    ]
}

#[cfg(not(unix))]
pub(super) fn long_running_command() -> Vec<String> {
    vec![
        "powershell.exe".to_string(),
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        "Write-Output $PID; Start-Sleep -Seconds 30".to_string(),
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

#[cfg(not(unix))]
fn active_command_cleanup_verified(_pid: u32) -> Result<bool, String> {
    Err("Active-command PID cleanup qualification is not implemented on this platform".to_string())
}

#[cfg(not(unix))]
pub(super) fn terminate_probe_process(_pid: u32) {}

pub(super) async fn wait_for_process_stopped(pid: u32) -> Result<bool, String> {
    for _ in 0..20 {
        if active_command_cleanup_verified(pid)? {
            return Ok(true);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Ok(false)
}

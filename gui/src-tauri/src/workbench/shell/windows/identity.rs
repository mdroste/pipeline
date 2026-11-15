//! Kernel handles, token identities and containment cleanup for the firewall
//! guard. Decisions use opened handles rather than trusting caller-supplied SIDs
//! or a PID that could have been recycled between inspection and termination.
use std::{
    ptr::null_mut,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::Authorization::*,
    Security::*,
    System::{Diagnostics::ToolHelp::*, Threading::*},
};

pub(super) struct Handle(pub HANDLE);
// Handles are owned, never inherited, and Win32 permits cross-thread use.
unsafe impl Send for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
impl Handle {
    pub fn new(raw: HANDLE) -> Result<Self, String> {
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            Err(error())
        } else {
            Ok(Self(raw))
        }
    }
    pub fn alive(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_TIMEOUT }
    }
}
pub(super) fn error() -> String {
    std::io::Error::last_os_error().to_string()
}
pub(super) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub(super) fn process(pid: u32, terminate: bool) -> Result<Handle, String> {
    Handle::new(unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION
                | PROCESS_SYNCHRONIZE
                | if terminate { PROCESS_TERMINATE } else { 0 },
            0,
            pid,
        )
    })
}
pub(super) fn created(process: &Handle) -> Result<u64, String> {
    let (mut creation, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    if unsafe { GetProcessTimes(process.0, &mut creation, &mut exit, &mut kernel, &mut user) } == 0
    {
        return Err(error());
    }
    Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}
fn token(process: &Handle) -> Result<Handle, String> {
    let mut raw = null_mut();
    if unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut raw) } == 0 {
        return Err(error());
    }
    Handle::new(raw)
}
fn information(token: &Handle, class: TOKEN_INFORMATION_CLASS) -> Result<Vec<usize>, String> {
    let mut size = 0;
    unsafe {
        GetTokenInformation(token.0, class, null_mut(), 0, &mut size);
    }
    if size == 0 || size > 65536 {
        return Err("Invalid command token information".into());
    }
    let mut buffer = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    if unsafe { GetTokenInformation(token.0, class, buffer.as_mut_ptr().cast(), size, &mut size) }
        == 0
    {
        return Err(error());
    }
    Ok(buffer)
}
pub(super) unsafe fn sid_text(sid: PSID) -> Result<String, String> {
    if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
        return Err("Invalid sandbox identity".into());
    }
    let mut output = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut output) } == 0 {
        return Err(error());
    }
    let mut len = 0;
    while unsafe { *output.add(len) } != 0 {
        len += 1;
    }
    let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(output, len) });
    unsafe {
        LocalFree(output.cast());
    }
    Ok(text)
}
pub(super) struct Sid(pub PSID);
impl Sid {
    pub fn parse(value: &str) -> Result<Self, String> {
        let mut sid = null_mut();
        if unsafe { ConvertStringSidToSidW(wide(value).as_ptr(), &mut sid) } == 0 {
            return Err(error());
        }
        Ok(Self(sid))
    }
}
impl Drop for Sid {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
pub(super) fn user(process: &Handle) -> Result<String, String> {
    let data = information(&token(process)?, TokenUser)?;
    unsafe { sid_text((*(data.as_ptr().cast::<TOKEN_USER>())).User.Sid) }
}
pub(super) fn package(process: &Handle) -> Result<Option<String>, String> {
    let data = information(&token(process)?, TokenAppContainerSid)?;
    let sid =
        unsafe { (*(data.as_ptr().cast::<TOKEN_APPCONTAINER_INFORMATION>())).TokenAppContainer };
    if sid.is_null() {
        Ok(None)
    } else {
        unsafe { sid_text(sid).map(Some) }
    }
}
pub(super) fn elevated() -> Result<bool, String> {
    let data = information(
        &token(&process(std::process::id(), false)?)?,
        TokenElevation,
    )?;
    Ok(unsafe { (*(data.as_ptr().cast::<TOKEN_ELEVATION>())).TokenIsElevated != 0 })
}
pub(super) fn enable_cleanup_privilege() -> Result<(), String> {
    let mut raw = null_mut();
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut raw,
        )
    } == 0
    {
        return Err(error());
    }
    let token = Handle::new(raw)?;
    let mut luid = LUID::default();
    if unsafe {
        LookupPrivilegeValueW(
            std::ptr::null(),
            wide("SeDebugPrivilege").as_ptr(),
            &mut luid,
        )
    } == 0
    {
        return Err(error());
    }
    let privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    unsafe {
        SetLastError(0);
    }
    if unsafe { AdjustTokenPrivileges(token.0, 0, &privileges, 0, null_mut(), null_mut()) } == 0
        || unsafe { GetLastError() } != 0
    {
        return Err(error());
    }
    Ok(())
}
fn processes() -> Result<Vec<PROCESSENTRY32W>, String> {
    let snapshot = Handle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })?;
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut entries = Vec::new();
    if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
        return Err(error());
    }
    loop {
        entries.push(entry);
        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            if unsafe { GetLastError() } != ERROR_NO_MORE_FILES {
                return Err(error());
            }
            break;
        }
    }
    Ok(entries)
}
pub(super) fn is_child(pid: u32, parent: u32) -> Result<bool, String> {
    Ok(processes()?
        .iter()
        .any(|p| p.th32ProcessID == pid && p.th32ParentProcessID == parent))
}
pub(super) fn terminate_identity(package_sid: &str, user_sid: &str) -> Result<(), String> {
    // Repeat snapshots because a live descendant can fork while its siblings
    // are being stopped. Never delete the firewall rules after an incomplete sweep.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut empty = false;
    loop {
        let mut found = false;
        for entry in processes()? {
            let Ok(process) = process(entry.th32ProcessID, true) else {
                continue;
            };
            // Protected OS processes cannot be command descendants. The guard
            // enables SeDebugPrivilege so a command cannot hide behind its DACL.
            if user(&process).ok().as_deref() != Some(user_sid) {
                continue;
            }
            if package(&process)?.as_deref() != Some(package_sid) {
                continue;
            }
            if !process.alive() {
                continue;
            }
            found = true;
            if unsafe { TerminateProcess(process.0, 125) } == 0 && process.alive() {
                return Err(error());
            }
        }
        if !found && empty {
            return Ok(());
        }
        empty = !found;
        if Instant::now() >= deadline {
            return Err(
                "Windows could not stop every command process; firewall restrictions were retained"
                    .into(),
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

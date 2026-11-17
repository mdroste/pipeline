//! A disposable, unpackaged AppContainer. No firewall, loopback-exemption,
//! privilege, or host ACL modifications. The profile owns its own storage.
use std::{ffi::c_void, path::PathBuf, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, Isolation::*, *},
    System::{Com::CoTaskMemFree, Threading::*},
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn check(ok: i32) -> std::io::Result<()> {
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn hr(result: i32) -> Result<(), String> {
    if result < 0 {
        Err(format!("HRESULT {result:#x}"))
    } else {
        Ok(())
    }
}
pub fn token() -> Result<serde_json::Value, String> {
    process_token(unsafe { GetCurrentProcess() })
}
pub fn process_token(process: HANDLE) -> Result<serde_json::Value, String> {
    unsafe {
        let mut token = null_mut();
        check(OpenProcessToken(process, TOKEN_QUERY, &mut token)).map_err(|e| e.to_string())?;
        let mut elevated = 0u32;
        let mut appcontainer = 0u32;
        let mut size = 0;
        let a = GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevated as *mut u32).cast(),
            4,
            &mut size,
        );
        let b = GetTokenInformation(
            token,
            TokenIsAppContainer,
            (&mut appcontainer as *mut u32).cast(),
            4,
            &mut size,
        );
        let mut sid_info = TOKEN_APPCONTAINER_INFORMATION {
            TokenAppContainer: null_mut(),
        };
        let c = GetTokenInformation(
            token,
            TokenAppContainerSid,
            (&mut sid_info as *mut TOKEN_APPCONTAINER_INFORMATION).cast(),
            size_of_val(&sid_info) as u32,
            &mut size,
        );
        // TokenAppContainerSid contains a variable-sized SID; query again into aligned storage.
        let mut data = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
        let mut sid_text = String::new();
        if c == 0
            && size > 0
            && GetTokenInformation(
                token,
                TokenAppContainerSid,
                data.as_mut_ptr().cast(),
                size,
                &mut size,
            ) != 0
        {
            let sid = (*(data.as_ptr().cast::<TOKEN_APPCONTAINER_INFORMATION>())).TokenAppContainer;
            if !sid.is_null() {
                sid_text = sid_string(sid)?;
            }
        }
        let mut capability_size = 0;
        GetTokenInformation(
            token,
            TokenCapabilities,
            null_mut(),
            0,
            &mut capability_size,
        );
        let mut capability_data =
            vec![0usize; (capability_size as usize).div_ceil(size_of::<usize>())];
        let mut capabilities = Vec::new();
        let capability_ok = GetTokenInformation(
            token,
            TokenCapabilities,
            capability_data.as_mut_ptr().cast(),
            capability_size,
            &mut capability_size,
        );
        if capability_ok != 0 {
            let groups = &*capability_data.as_ptr().cast::<TOKEN_GROUPS>();
            for group in
                std::slice::from_raw_parts(groups.Groups.as_ptr(), groups.GroupCount as usize)
            {
                capabilities.push(sid_string(group.Sid)?);
            }
        }
        let mut lpac = 0u32;
        let mut restricted_size = 0;
        GetTokenInformation(
            token,
            TokenRestrictedSids,
            null_mut(),
            0,
            &mut restricted_size,
        );
        let mut restricted_data =
            vec![0usize; (restricted_size as usize).div_ceil(size_of::<usize>())];
        let mut restricted_sids = Vec::new();
        if GetTokenInformation(
            token,
            TokenRestrictedSids,
            restricted_data.as_mut_ptr().cast(),
            restricted_size,
            &mut restricted_size,
        ) != 0
        {
            let groups = &*restricted_data.as_ptr().cast::<TOKEN_GROUPS>();
            for group in
                std::slice::from_raw_parts(groups.Groups.as_ptr(), groups.GroupCount as usize)
            {
                restricted_sids.push(sid_string(group.Sid)?);
            }
        }
        let lpac_ok = GetTokenInformation(
            token,
            TokenIsLessPrivilegedAppContainer,
            (&mut lpac as *mut u32).cast(),
            4,
            &mut size,
        );
        CloseHandle(token);
        check(a).and_then(|_| check(b)).map_err(|e| e.to_string())?;
        check(capability_ok).map_err(|e| e.to_string())?;
        Ok(
            serde_json::json!({"elevated":elevated != 0,"appcontainer":appcontainer != 0,"lpac_query_supported":lpac_ok != 0,"lpac": if lpac_ok != 0 { Some(lpac != 0) } else { None },"sid":sid_text,"capabilities":capabilities,"restricted_sids":restricted_sids}),
        )
    }
}
unsafe fn sid_string(sid: PSID) -> Result<String, String> {
    unsafe {
        let mut text = null_mut();
        check(ConvertSidToStringSidW(sid, &mut text)).map_err(|e| e.to_string())?;
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        let result = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        Ok(result)
    }
}

pub struct Profile {
    pub name: String,
    pub folder: PathBuf,
    sid: PSID,
    process: HANDLE,
}
pub fn restricted_sid(parent: &str) -> Result<String, String> {
    unsafe {
        let mut parent_sid = null_mut();
        check(ConvertStringSidToSidW(
            wide(parent).as_ptr(),
            &mut parent_sid,
        ))
        .map_err(|e| e.to_string())?;
        let mut child_sid = null_mut();
        let result = DeriveRestrictedAppContainerSidFromAppContainerSidAndRestrictedName(
            parent_sid,
            wide("Pipeline.NetworkProbe.Child").as_ptr(),
            &mut child_sid,
        );
        LocalFree(parent_sid);
        hr(result)?;
        let text = sid_string(child_sid);
        FreeSid(child_sid);
        text
    }
}
impl Profile {
    pub fn sid_text(&self) -> Result<String, String> {
        unsafe { sid_string(self.sid) }
    }
    pub fn create() -> Result<Self, String> {
        let name = format!(
            "Pipeline.NetworkProbe.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );
        let wname = wide(&name);
        let mut sid = null_mut();
        unsafe {
            hr(CreateAppContainerProfile(
                wname.as_ptr(),
                wname.as_ptr(),
                wname.as_ptr(),
                null_mut(),
                0,
                &mut sid,
            ))?;
        }
        let mut profile = Self {
            name,
            folder: PathBuf::new(),
            sid,
            process: null_mut(),
        };
        unsafe {
            let text = wide(&sid_string(sid)?);
            let mut path = null_mut();
            hr(GetAppContainerFolderPath(text.as_ptr(), &mut path))?;
            let mut len = 0;
            while *path.add(len) != 0 {
                len += 1;
            }
            profile.folder = PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(
                path, len,
            )));
            CoTaskMemFree(path.cast());
        }
        Ok(profile)
    }
    pub fn launch(&mut self) -> Result<(), String> {
        self.launch_mode("proxy", true)
    }
    pub fn launch_mode(&mut self, mode: &str, network: bool) -> Result<(), String> {
        let exe = self.folder.join("proxy.exe");
        std::fs::copy(std::env::current_exe().map_err(|e| e.to_string())?, &exe)
            .map_err(|e| e.to_string())?;
        let mut command = wide(&format!(
            "\"{}\" {mode} \"{}\"",
            exe.display(),
            self.folder.join("ready.json").display()
        ));
        let mut caps = [SID_AND_ATTRIBUTES {
            Sid: null_mut(),
            Attributes: 0x4, /* SE_GROUP_ENABLED */
        }; 2];
        let mut cap_storage = [[0u32; 17]; 2];
        unsafe {
            for (i, kind) in [
                WinCapabilityInternetClientSid,
                WinCapabilityPrivateNetworkClientServerSid,
            ]
            .iter()
            .enumerate()
            {
                let mut len = 68;
                check(CreateWellKnownSid(
                    *kind,
                    null_mut(),
                    cap_storage[i].as_mut_ptr().cast(),
                    &mut len,
                ))
                .map_err(|e| e.to_string())?;
                caps[i].Sid = cap_storage[i].as_mut_ptr().cast();
            }
            let mut security = SECURITY_CAPABILITIES {
                AppContainerSid: self.sid,
                Capabilities: if network {
                    caps.as_mut_ptr()
                } else {
                    null_mut()
                },
                CapabilityCount: if network { 2 } else { 0 },
                Reserved: 0,
            };
            let mut size = 0;
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size);
            let mut storage = vec![0usize; size.div_ceil(size_of::<usize>())];
            let attrs = storage.as_mut_ptr().cast();
            check(InitializeProcThreadAttributeList(attrs, 1, 0, &mut size))
                .map_err(|e| e.to_string())?;
            let result = (|| {
                check(UpdateProcThreadAttribute(
                    attrs,
                    0,
                    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                    (&mut security as *mut SECURITY_CAPABILITIES).cast::<c_void>(),
                    size_of_val(&security),
                    null_mut(),
                    null_mut(),
                ))?;
                let mut startup: STARTUPINFOEXW = std::mem::zeroed();
                startup.StartupInfo.cb = size_of_val(&startup) as u32;
                startup.lpAttributeList = attrs;
                let mut process: PROCESS_INFORMATION = std::mem::zeroed();
                check(CreateProcessW(
                    wide(&exe.to_string_lossy()).as_ptr(),
                    command.as_mut_ptr(),
                    null_mut(),
                    null_mut(),
                    0,
                    EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW,
                    null_mut(),
                    wide("C:\\Windows\\System32").as_ptr(),
                    &startup.StartupInfo,
                    &mut process,
                ))?;
                CloseHandle(process.hThread);
                self.process = process.hProcess;
                Ok::<(), std::io::Error>(())
            })();
            DeleteProcThreadAttributeList(attrs);
            result.map_err(|e| e.to_string())
        }
    }
    pub fn ready(&self) -> Result<serde_json::Value, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if let Ok(data) = std::fs::read(self.folder.join("ready.json"))
                && let Ok(value) = serde_json::from_slice(&data)
            {
                return Ok(value);
            }
            let mut code = 0;
            unsafe {
                GetExitCodeProcess(self.process, &mut code);
            }
            if code != 259 || std::time::Instant::now() > deadline {
                return Err(format!(
                    "proxy did not become ready; exit={code:#x}, profile={}",
                    self.folder.display()
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        unsafe {
            if !self.process.is_null() {
                TerminateProcess(self.process, 0);
                WaitForSingleObject(self.process, 5000);
                CloseHandle(self.process);
            }
            let result = DeleteAppContainerProfile(wide(&self.name).as_ptr());
            eprintln!("profile_cleanup name={} HRESULT={result:#x}", self.name);
            FreeSid(self.sid);
        }
    }
}

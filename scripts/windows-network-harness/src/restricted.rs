//! Optional extra restricted-token access check, for standalone experiments only.
use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    System::Threading::*,
};
const SE_GROUP_LOGON_ID: u32 = 0xc0000000; // winnt.h
pub struct Token(pub HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub fn create(sids: &str, app_sid: Option<&str>) -> Result<Token, String> {
    unsafe {
        let mut primary = null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ASSIGN_PRIMARY | TOKEN_ADJUST_DEFAULT,
            &mut primary,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let primary = Token(primary);
        let mut owned = Vec::new();
        let app_sid = app_sid
            .filter(|_| std::env::var("PIPELINE_PROBE_RESTRICT_APP_SID").as_deref() == Ok("1"));
        for sid in sids.split(',').filter(|s| !s.is_empty()).chain(app_sid) {
            let mut parsed = null_mut();
            if ConvertStringSidToSidW(crate::identity::wide(sid).as_ptr(), &mut parsed) == 0 {
                for ptr in owned {
                    LocalFree(ptr);
                }
                return Err(std::io::Error::last_os_error().to_string());
            }
            owned.push(parsed);
        }
        let mut groups: Vec<_> = owned
            .iter()
            .map(|sid| SID_AND_ATTRIBUTES {
                Sid: *sid,
                Attributes: 0,
            })
            .collect();
        let mut group_size = 0;
        GetTokenInformation(primary.0, TokenGroups, null_mut(), 0, &mut group_size);
        let mut group_data = vec![0usize; (group_size as usize).div_ceil(size_of::<usize>())];
        if std::env::var("PIPELINE_PROBE_RESTRICT_LOGON").as_deref() == Ok("1") {
            if GetTokenInformation(
                primary.0,
                TokenGroups,
                group_data.as_mut_ptr().cast(),
                group_size,
                &mut group_size,
            ) == 0
            {
                for ptr in owned {
                    LocalFree(ptr);
                }
                return Err(format!("TokenGroups: {}", std::io::Error::last_os_error()));
            }
            let info = &*group_data.as_ptr().cast::<TOKEN_GROUPS>();
            for group in std::slice::from_raw_parts(info.Groups.as_ptr(), info.GroupCount as usize)
            {
                if group.Attributes & SE_GROUP_LOGON_ID == SE_GROUP_LOGON_ID {
                    groups.push(SID_AND_ATTRIBUTES {
                        Sid: group.Sid,
                        Attributes: 0,
                    });
                }
            }
        }
        let mut restricted = null_mut();
        let ok = CreateRestrictedToken(
            primary.0,
            DISABLE_MAX_PRIVILEGE,
            0,
            null_mut(),
            0,
            null_mut(),
            groups.len() as u32,
            groups.as_ptr(),
            &mut restricted,
        );
        let error = std::io::Error::last_os_error();
        for ptr in owned {
            LocalFree(ptr);
        }
        if ok == 0 {
            return Err(format!("CreateRestrictedToken: {error}"));
        }
        let restricted = Token(restricted);
        if std::env::var("PIPELINE_PROBE_RESTRICT_DEFAULT_DACL").as_deref() == Ok("1") {
            grant_default_dacl(restricted.0)?;
        }
        Ok(restricted)
    }
}

fn grant_default_dacl(token: HANDLE) -> Result<(), String> {
    unsafe {
        let mut size = 0;
        GetTokenInformation(token, TokenDefaultDacl, null_mut(), 0, &mut size);
        let mut data = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
        if GetTokenInformation(
            token,
            TokenDefaultDacl,
            data.as_mut_ptr().cast(),
            size,
            &mut size,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let old = &*data.as_ptr().cast::<TOKEN_DEFAULT_DACL>();
        let mut sid = null_mut();
        if ConvertStringSidToSidW(crate::identity::wide("S-1-5-12").as_ptr(), &mut sid) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: GENERIC_ALL,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: 0,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                ptstrName: sid.cast(),
            },
        };
        let mut acl = null_mut();
        let error = SetEntriesInAclW(1, &entry, old.DefaultDacl, &mut acl);
        LocalFree(sid);
        if error != 0 {
            return Err(format!("default DACL merge: {error}"));
        }
        let dacl = TOKEN_DEFAULT_DACL { DefaultDacl: acl };
        let ok = SetTokenInformation(
            token,
            TokenDefaultDacl,
            (&dacl as *const TOKEN_DEFAULT_DACL).cast(),
            size_of_val(&dacl) as u32,
        );
        let error = std::io::Error::last_os_error();
        LocalFree(acl.cast());
        if ok == 0 {
            return Err(format!("default DACL: {error}"));
        }
        Ok(())
    }
}

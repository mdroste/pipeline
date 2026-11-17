//! ACLs and integrity labels for new disposable copies only. These do not
//! replace PSEC's explicit path denials for the rest of the host filesystem.
use std::path::{Path, PathBuf};

pub fn staged(appid: &str, sid: &str) -> Result<PathBuf, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("staged-{appid}"));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    for dir in ["readonly", "work", "work/.git", "work/.secrets", "outside"] {
        let path = root.join(dir);
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        std::fs::write(path.join("canary.txt"), "original-canary").map_err(|e| e.to_string())?;
    }
    // Changes apply only to these new disposable files, never source paths.
    protect(&root, sid, "0x1200a9", "ME")?;
    for (name, access, integrity) in [
        ("readonly", "0x1200a9", "ME"),
        ("work", "0x1301bf", "LW"),
        ("work/.git", "0x1200a9", "ME"),
        ("work/.secrets", "0", "ME"),
        ("outside", "0", "ME"),
    ] {
        let dir = root.join(name);
        protect(&dir, sid, access, integrity)?;
        protect(&dir.join("canary.txt"), sid, access, integrity)?;
    }
    if std::env::var("PIPELINE_PROBE_AMBIENT_ACL").as_deref() == Ok("1") {
        // Synthetic host file readable by ALL APPLICATION PACKAGES. A raw
        // AppContainer inherits this access even without a per-profile grant.
        protect(&root.join("outside"), "S-1-15-2-1", "0x1200a9", "ME")?;
        protect(
            &root.join("outside/canary.txt"),
            "S-1-15-2-1",
            "0x1200a9",
            "ME",
        )?;
    }
    Ok(root)
}

pub(crate) fn protect(path: &Path, sid: &str, access: &str, integrity: &str) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
    };
    unsafe {
        let mut token = std::ptr::null_mut();
        if windows_sys::Win32::System::Threading::OpenProcessToken(
            windows_sys::Win32::System::Threading::GetCurrentProcess(),
            TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut size = 0;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
        let mut data = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
        let ok = GetTokenInformation(token, TokenUser, data.as_mut_ptr().cast(), size, &mut size);
        CloseHandle(token);
        if ok == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut owner = std::ptr::null_mut();
        if ConvertSidToStringSidW((*(data.as_ptr().cast::<TOKEN_USER>())).User.Sid, &mut owner) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut len = 0;
        while *owner.add(len) != 0 {
            len += 1;
        }
        let owner_text = String::from_utf16_lossy(std::slice::from_raw_parts(owner, len));
        LocalFree(owner.cast());
        let restricted = if std::env::var("PIPELINE_PROBE_RESTRICTED_COPIES").as_deref() == Ok("1")
            && sid != "S-1-15-2-1"
        {
            format!("(A;OICI;{access};;;RC)")
        } else {
            String::new()
        };
        let ambient_users = if sid == "S-1-15-2-1"
            && std::env::var("PIPELINE_PROBE_AMBIENT_USERS").as_deref() == Ok("1")
        {
            "(A;OICI;0x1200a9;;;BU)"
        } else {
            ""
        };
        let sddl = format!(
            "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;{owner_text})(A;OICI;{access};;;{sid}){restricted}{ambient_users}(A;OICI;RC;;;OW)S:(ML;OICI;NW;;;{integrity})"
        );
        let mut descriptor = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            crate::identity::wide(&sddl).as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut dacl = std::ptr::null_mut();
        let mut sacl = std::ptr::null_mut();
        let mut present = 0;
        let mut defaulted = 0;
        GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted);
        GetSecurityDescriptorSacl(descriptor, &mut present, &mut sacl, &mut defaulted);
        let path_wide = crate::identity::wide(&path.to_string_lossy());
        let mut result = SetNamedSecurityInfoW(
            path_wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl,
            std::ptr::null_mut(),
        );
        // Creator ownership allows DACL setup first. The inherited workspace ACL
        // need not initially include WRITE_OWNER, needed to set the new label.
        if result == 0 {
            result = SetNamedSecurityInfoW(
                path_wide.as_ptr(),
                SE_FILE_OBJECT,
                LABEL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                sacl,
            );
        }
        LocalFree(descriptor);
        if result != 0 {
            return Err(format!(
                "staged ACL {}: {}",
                path.display(),
                std::io::Error::from_raw_os_error(result as i32)
            ));
        }
    }
    Ok(())
}

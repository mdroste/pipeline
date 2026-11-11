use windows_sys::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
};

fn registry_path(root: HKEY, subkey: &str) -> Option<String> {
    let subkey: Vec<u16> = subkey.encode_utf16().chain(Some(0)).collect();
    let value: Vec<u16> = "Path".encode_utf16().chain(Some(0)).collect();
    let mut bytes = 0u32;
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ;
    // RegGetValue expands REG_EXPAND_SZ using the process environment.
    // Both UTF-16 names are terminated and out buffers are sized in bytes.
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            value.as_ptr(),
            flags,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut bytes,
        )
    };
    if status != 0 || bytes == 0 || bytes > 128 * 1024 {
        return None;
    }
    let mut data = vec![0u16; (bytes as usize).div_ceil(2)];
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            value.as_ptr(),
            flags,
            std::ptr::null_mut(),
            data.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if status != 0 {
        return None;
    }
    let length = data.iter().position(|c| *c == 0).unwrap_or(data.len());
    String::from_utf16(&data[..length]).ok()
}

pub(super) fn refresh(inherited: &str) -> String {
    let machine = registry_path(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
    )
    .unwrap_or_default();
    let user = registry_path(HKEY_CURRENT_USER, "Environment").unwrap_or_default();
    merge(&[&machine, &user, inherited])
}

fn merge(paths: &[&str]) -> String {
    let mut seen = std::collections::HashSet::new();
    paths
        .iter()
        .flat_map(|path| path.split(';'))
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter(|entry| seen.insert(entry.to_lowercase()))
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refreshed_registry_paths_keep_custom_inherited_tools_without_duplicates() {
        assert_eq!(
            merge(&[
                r"C:\Windows;C:\new-tool",
                r"C:\UserTools",
                r"c:\windows;C:\dev"
            ]),
            r"C:\Windows;C:\new-tool;C:\UserTools;C:\dev"
        );
    }
}

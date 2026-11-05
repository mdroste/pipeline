//! Paths given to Paddle's native code on Windows.
//!
//! Python hands Paddle UTF-8 strings, and its native file loading reads them
//! in the system code page, so a folder name outside ASCII does not open. The
//! managed location sits under the user's home and cannot move. Windows' short
//! name for the folder reaches the same files in ASCII. Where a volume has no
//! short names, a new install is refused before anything is downloaded.

use std::path::{Path, PathBuf};

fn is_ascii(path: &Path) -> bool {
    path.to_str().is_some_and(str::is_ascii)
}

/// The ASCII spelling of `path`, if there is one. The deepest existing folder
/// is shortened because Windows only knows short names for what exists.
fn ascii_form(path: &Path, shorten: &dyn Fn(&Path) -> Option<PathBuf>) -> Option<PathBuf> {
    if is_ascii(path) {
        return Some(path.to_path_buf());
    }
    let existing = path.ancestors().find(|ancestor| ancestor.exists())?;
    let missing = path.strip_prefix(existing).ok()?;
    let form = shorten(existing)?.join(missing);
    is_ascii(&form).then_some(form)
}

fn require_ascii_form(
    home: &Path,
    shorten: &dyn Fn(&Path) -> Option<PathBuf>,
) -> Result<(), String> {
    match ascii_form(home, shorten) {
        Some(_) => Ok(()),
        None => Err(format!(
            "The Full Parser can't be installed in {}. It can't read a folder name with accented \
             or non-Latin letters, and Windows has no short name for this folder. Nothing was \
             downloaded. Use a Windows account whose folder name has only English letters and \
             numbers, or ask your IT administrator to turn on short file names for this drive.",
            home.display()
        )),
    }
}

/// `path` in the form Paddle's native code can open. Outside Windows, and when
/// no such form exists, this is `path` itself.
pub(crate) fn native_path(path: &Path) -> PathBuf {
    if cfg!(windows) {
        ascii_form(path, &short_path).unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

/// Refuse an install whose files Paddle could never open, before the download.
pub(super) fn ensure_native_path(home: &Path) -> Result<(), String> {
    if cfg!(windows) {
        require_ascii_form(home, &short_path)
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn short_path(path: &Path) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    #[link(name = "kernel32")]
    extern "system" {
        fn GetShortPathNameW(long_path: *const u16, short_path: *mut u16, capacity: u32) -> u32;
    }

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buffer = vec![0u16; 1024];
    loop {
        // The result is the length written, or the capacity needed when the
        // buffer is too small, or zero on failure.
        let length =
            unsafe { GetShortPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) }
                as usize;
        if length == 0 {
            return None;
        }
        if length < buffer.len() {
            return Some(OsString::from_wide(&buffer[..length]).into());
        }
        buffer.resize(length + 1, 0);
    }
}

#[cfg(not(windows))]
fn short_path(_: &Path) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What NTFS generates for the folder this fixture creates.
    fn shorten_home(temp: &Path) -> impl Fn(&Path) -> Option<PathBuf> + '_ {
        move |path| {
            Some(
                temp.join("JOS_~1")
                    .join(path.strip_prefix(temp.join("José")).ok()?),
            )
        }
    }

    #[test]
    fn an_ascii_path_is_left_alone() {
        let path = Path::new("C:/Users/Jose/.pipeline/native");
        let unused = |_: &Path| -> Option<PathBuf> { panic!("no lookup is needed") };
        assert_eq!(ascii_form(path, &unused).as_deref(), Some(path));
    }

    #[test]
    fn folders_not_created_yet_keep_their_names_under_the_short_home() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("José");
        std::fs::create_dir(&home).unwrap();
        assert_eq!(
            ascii_form(&home.join("assets/page-1"), &shorten_home(temp.path())),
            Some(temp.path().join("JOS_~1/assets/page-1"))
        );
    }

    #[test]
    fn a_home_with_a_short_name_may_install() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("José");
        std::fs::create_dir(&home).unwrap();
        assert_eq!(
            require_ascii_form(&home, &shorten_home(temp.path())),
            Ok(())
        );
    }

    #[test]
    fn a_home_without_an_ascii_short_name_is_refused_and_says_why() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("José");
        std::fs::create_dir(&home).unwrap();
        for shorten in [
            &(|_: &Path| None) as &dyn Fn(&Path) -> Option<PathBuf>,
            &|path: &Path| Some(path.to_path_buf()),
        ] {
            let error = require_ascii_form(&home, shorten).unwrap_err();
            assert!(error.contains("José"), "{error}");
            assert!(error.contains("Nothing was downloaded"), "{error}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn other_platforms_keep_their_paths() {
        let path = Path::new("/Users/José/.pipeline");
        assert_eq!(native_path(path), path);
        assert_eq!(ensure_native_path(path), Ok(()));
    }
}

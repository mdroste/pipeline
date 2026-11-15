use super::*;
use std::{fs, io::Read as _};

#[test]
fn reads_nested_unicode_files_and_rejects_non_files_and_traversal() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("résultats")).unwrap();
    fs::write(temp.path().join("résultats/a b.txt"), "exact bytes").unwrap();
    let root = ReadRoot::open(temp.path()).unwrap();
    let mut text = String::new();
    root.open_regular(Path::new("résultats/a b.txt"))
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "exact bytes");
    for invalid in ["", "..", "../outside", "/absolute", "résultats", "missing"] {
        assert!(root.open_regular(Path::new(invalid)).is_err(), "{invalid}");
    }
}

#[cfg(unix)]
#[test]
fn a_parent_path_swap_cannot_redirect_the_open_file() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("run");
    let outside = temp.path().join("outside");
    fs::create_dir_all(folder.join("nested")).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(folder.join("nested/file"), "inside").unwrap();
    fs::write(outside.join("file"), "outside").unwrap();
    let root = ReadRoot::open(&folder).unwrap();
    let mut file = root
        .open_regular_with(Path::new("nested/file"), || {
            fs::rename(folder.join("nested"), folder.join("retained")).unwrap();
            symlink(&outside, folder.join("nested")).unwrap();
        })
        .unwrap();
    let mut text = String::new();
    file.read_to_string(&mut text).unwrap();
    assert_eq!(text, "inside");
    assert!(root.open_regular(Path::new("nested/file")).is_err());
    symlink(outside.join("file"), folder.join("linked")).unwrap();
    assert!(root.open_regular(Path::new("linked")).is_err());
    // Replacing the root pathname also cannot change an already captured root.
    fs::rename(&folder, temp.path().join("old-run")).unwrap();
    symlink(&outside, &folder).unwrap();
    assert!(root.open_regular(Path::new("file")).is_err());
}

#[cfg(windows)]
#[test]
fn a_directory_junction_cannot_escape_the_read_root() {
    let temp = tempfile::tempdir().unwrap();
    let root_path = temp.path().join("run");
    let outside = temp.path().join("outside");
    fs::create_dir(&root_path).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("file"), "outside").unwrap();
    let output = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(root_path.join("junction"))
        .arg(&outside)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root = ReadRoot::open(&root_path).unwrap();
    assert!(root.open_regular(Path::new("junction/file")).is_err());
    // Remove only the junction; keep its target for the fixture's own cleanup.
    fs::remove_dir(root_path.join("junction")).unwrap();
}

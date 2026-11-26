use super::*;

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app folder");
    std::fs::create_dir_all(&app).unwrap();
    let source = app.join(name());
    let application = app.join("pipeline-gui");
    std::fs::write(&source, "launcher").unwrap();
    std::fs::write(&application, "app").unwrap();
    let bin = root.path().join("user bin");
    (root, bin, application, source)
}

#[test]
fn installs_repairs_and_relocates_only_the_user_launcher() {
    let (_root, bin, app, source) = fixture();
    install_at(&bin, &app, &source).unwrap();
    assert_eq!(
        std::fs::read_to_string(bin.join(LOCATOR)).unwrap(),
        app.to_str().unwrap()
    );
    assert_eq!(
        digest(&bin.join(name())).unwrap(),
        ownership(&bin).unwrap().sha256
    );
    std::fs::write(bin.join(name()), "broken").unwrap();
    let moved = app.with_file_name("moved app");
    std::fs::write(&moved, "app").unwrap();
    install_at(&bin, &moved, &source).unwrap();
    assert_eq!(
        std::fs::read_to_string(bin.join(name())).unwrap(),
        "launcher"
    );
    assert_eq!(
        std::fs::read_to_string(bin.join(LOCATOR)).unwrap(),
        moved.to_str().unwrap()
    );
    std::fs::write(bin.join("another-tool"), "keep").unwrap();
    uninstall_at(&bin).unwrap();
    assert!(bin.join("another-tool").is_file());
    assert!(!bin.join(name()).exists());
    assert!(source.is_file());
    assert!(app.is_file());
}

#[test]
fn never_overwrites_or_uninstalls_an_existing_unowned_command() {
    let (_root, bin, app, source) = fixture();
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join(name()), "user command").unwrap();
    assert!(install_at(&bin, &app, &source)
        .unwrap_err()
        .contains("unowned"));
    assert!(uninstall_at(&bin).is_err());
    assert_eq!(
        std::fs::read_to_string(bin.join(name())).unwrap(),
        "user command"
    );
}

#[test]
fn corrupt_ownership_does_not_authorize_removal() {
    let (_root, bin, app, source) = fixture();
    install_at(&bin, &app, &source).unwrap();
    std::fs::write(bin.join(OWNER), "{}").unwrap();
    assert!(uninstall_at(&bin).is_err());
    assert!(bin.join(name()).is_file());
}

#[cfg(unix)]
#[test]
fn command_directory_symlinks_are_not_followed() {
    let (root, bin, app, source) = fixture();
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, &bin).unwrap();
    assert!(install_at(&bin, &app, &source).is_err());
    assert!(!outside.join(name()).exists());
}

use super::*;

fn fixture() -> (tempfile::TempDir, Storage, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let local = base.join("home/.pipeline");
    std::fs::create_dir_all(&local).unwrap();
    let custom = base.join("Research data");
    std::fs::create_dir(&custom).unwrap();
    (temp, Storage::new(local), custom)
}

#[test]
fn changing_folder_is_persisted_but_never_moves_or_switches_live_data() {
    let (_temp, storage, custom) = fixture();
    std::fs::write(storage.local.join("saved-review.md"), "original").unwrap();
    storage.save(&custom).unwrap();
    assert_eq!(storage.data_root().unwrap(), storage.local);
    let settings = storage.settings().unwrap();
    assert!(settings.restart_required);
    assert_eq!(settings.configured_directory, custom.display().to_string());
    assert_eq!(
        std::fs::read_to_string(storage.local.join("saved-review.md")).unwrap(),
        "original"
    );
    assert_eq!(std::fs::read_dir(&custom).unwrap().count(), 0);
    assert_eq!(
        Storage::new(storage.local.clone()).data_root().unwrap(),
        custom
    );
}

#[test]
fn returning_to_default_reopens_existing_data_after_restart() {
    let (_temp, original, custom) = fixture();
    original.save(&custom).unwrap();
    let custom_storage = Storage::new(original.local.clone());
    std::fs::write(custom.join("research.txt"), "keep").unwrap();
    custom_storage.save(&original.local).unwrap();
    assert_eq!(custom_storage.data_root().unwrap(), custom);
    let reopened = Storage::new(original.local.clone());
    assert_eq!(reopened.data_root().unwrap(), original.local);
    assert!(!reopened.settings().unwrap().restart_required);
    assert_eq!(
        std::fs::read_to_string(custom.join("research.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn missing_custom_drive_never_recreates_it_or_falls_back() {
    let (_temp, original, custom) = fixture();
    original.save(&custom).unwrap();
    std::fs::remove_dir(&custom).unwrap();
    let reopened = Storage::new(original.local.clone());
    assert!(reopened.data_root().unwrap_err().contains("unavailable"));
    assert!(reopened.settings().unwrap().error.is_some());
    assert!(!custom.exists());
    // The settings screen can still select the default without opening data.
    reopened.save(&original.local).unwrap();
    assert!(reopened.data_root().is_err());
    assert_eq!(
        Storage::new(original.local.clone()).data_root().unwrap(),
        original.local
    );
}

#[test]
fn invalid_selections_preserve_the_saved_setting() {
    let (_temp, storage, custom) = fixture();
    storage.save(&custom).unwrap();
    let before = std::fs::read(storage.local.join("storage.json")).unwrap();
    let file = custom.join("a-file");
    std::fs::write(&file, "existing").unwrap();
    let nested = storage.local.join("workbench");
    std::fs::create_dir(&nested).unwrap();
    for invalid in [
        Path::new("relative"),
        file.as_path(),
        nested.as_path(),
        storage.local.parent().unwrap(),
        custom.join("missing").as_path(),
    ] {
        assert!(storage.save(invalid).is_err(), "{}", invalid.display());
        assert_eq!(
            std::fs::read(storage.local.join("storage.json")).unwrap(),
            before
        );
    }
}

#[test]
fn corrupt_configuration_is_visible_and_repairable_without_data_fallback() {
    let (_temp, storage, custom) = fixture();
    std::fs::write(storage.local.join("storage.json"), "{unfinished").unwrap();
    let reopened = Storage::new(storage.local.clone());
    assert!(reopened.data_root().is_err());
    assert!(reopened.settings().unwrap().error.is_some());
    reopened.save(&custom).unwrap();
    assert!(reopened.data_root().is_err());
    assert_eq!(
        Storage::new(storage.local.clone()).data_root().unwrap(),
        custom
    );
}

#[test]
fn existing_library_contents_are_preserved_and_nested_locations_rejected() {
    let (_temp, original, custom) = fixture();
    let runs = custom.join("runs");
    std::fs::create_dir(&runs).unwrap();
    std::fs::write(runs.join("report.md"), "saved").unwrap();
    original.save(&custom).unwrap();
    let reopened = Storage::new(original.local.clone());
    assert!(reopened.save(&runs).is_err());
    assert!(reopened.save(custom.parent().unwrap()).is_err());
    assert_eq!(
        std::fs::read_to_string(reopened.data_root().unwrap().join("runs/report.md")).unwrap(),
        "saved"
    );
}

#[cfg(unix)]
#[test]
fn folder_alias_is_canonicalized_and_later_retargeting_is_rejected() {
    use std::os::unix::fs::symlink;
    let (_temp, original, custom) = fixture();
    let alias = custom.with_file_name("alias");
    symlink(&custom, &alias).unwrap();
    original.save(&alias).unwrap();
    let reopened = Storage::new(original.local.clone());
    assert_eq!(reopened.data_root().unwrap(), custom);
    let moved = custom.with_file_name("moved");
    std::fs::rename(&custom, &moved).unwrap();
    symlink(&moved, &custom).unwrap();
    assert!(reopened
        .data_root()
        .unwrap_err()
        .contains("different location"));
}

#[test]
fn relative_or_unknown_configuration_never_opens_a_store() {
    let (_temp, storage, _) = fixture();
    for raw in [
        r#"{"directory":"relative"}"#,
        r#"{"directory":null,"unknown":1}"#,
    ] {
        std::fs::write(storage.local.join("storage.json"), raw).unwrap();
        assert!(Storage::new(storage.local.clone()).data_root().is_err());
    }
}

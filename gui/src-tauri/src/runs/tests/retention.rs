use super::*;

#[test]
fn annotations_require_a_bounded_json_object() {
    assert!(validate_annotation_content(r#"{"a":{"status":"done"}}"#).is_ok());
    assert!(validate_annotation_content("[]").is_err());
    assert!(validate_annotation_content("not-json").is_err());
    assert!(validate_annotation_content(&"x".repeat(MAX_ANNOTATION_BYTES + 1)).is_err());
}

#[test]
fn retention_preview_matches_count_and_byte_limits() {
    // Newest first. The running oldest entry is protected, so the next
    // oldest completed run is selected when the count limit is exceeded.
    let plan = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("middle".to_string(), false, 20),
            ("oldest".to_string(), true, 30),
        ],
        2,
        0,
    );
    let preview = plan.preview;
    assert_eq!(preview.delete_count, 1);
    assert_eq!(preview.delete_bytes, 20);
    assert_eq!(preview.remaining_count, 2);
    assert_eq!(preview.remaining_bytes, 40);
    assert!(!preview.preview_token.is_empty());
    assert_eq!(plan.candidates, vec!["middle".to_string()]);

    let preview = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("middle".to_string(), false, 20),
            ("oldest".to_string(), false, 30),
        ],
        0,
        25,
    )
    .preview;
    assert_eq!(preview.delete_count, 2);
    assert_eq!(preview.delete_bytes, 50);
    assert_eq!(preview.remaining_count, 1);
    assert_eq!(preview.remaining_bytes, 10);
}

#[test]
fn retention_never_deletes_the_newest_run() {
    // A byte limit smaller than a single run must not delete the run that
    // just completed; only the older runs are candidates.
    let plan = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 100),
            ("middle".to_string(), false, 20),
            ("oldest".to_string(), false, 30),
        ],
        0,
        50,
    );
    assert_eq!(
        plan.candidates,
        vec!["oldest".to_string(), "middle".to_string()]
    );
    assert_eq!(plan.preview.remaining_count, 1);
    assert_eq!(plan.preview.remaining_bytes, 100);
}

#[test]
fn retention_preview_token_binds_the_confirmed_deletion_plan() {
    let initial = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("oldest".to_string(), false, 20),
        ],
        1,
        0,
    )
    .preview;
    let unchanged = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("oldest".to_string(), false, 20),
        ],
        1,
        0,
    )
    .preview;
    let changed_candidate = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("different-oldest".to_string(), false, 20),
        ],
        1,
        0,
    )
    .preview;
    let changed_size = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("oldest".to_string(), false, 21),
        ],
        1,
        0,
    )
    .preview;

    assert_eq!(initial.preview_token, unchanged.preview_token);
    assert_ne!(initial.preview_token, changed_candidate.preview_token);
    assert_ne!(initial.preview_token, changed_size.preview_token);
}

#[test]
fn trashed_runs_restore_through_symlinked_storage_paths() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    fs::create_dir_all(real.join("runs")).unwrap();
    fs::create_dir_all(real.join("trash/runs/abc123_run")).unwrap();
    fs::write(real.join("trash/runs/abc123_run/manifest.json"), "{}").unwrap();
    // The configured data folder reaches Trash through a link, as a
    // symlinked folder (or a Windows verbatim prefix) does.
    #[cfg(unix)]
    let data = {
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        link
    };
    #[cfg(not(unix))]
    let data = real.clone();
    restore_trashed_run_at(&data.join("runs"), &data.join("trash/runs"), "abc123_run").unwrap();
    assert!(real.join("runs/abc123_run/manifest.json").is_file());
    assert!(!real.join("trash/runs/abc123_run").exists());
    assert!(
        restore_trashed_run_at(&data.join("runs"), &data.join("trash/runs"), "abc123_run").is_err()
    );
}

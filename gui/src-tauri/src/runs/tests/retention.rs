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
fn retention_keeps_a_finished_run_behind_a_later_started_review() {
    // The later-started review is newest and still running, so the run that
    // just finished sits second. Its hold keeps it; the oldest run still goes.
    let finished = "retention-test-finished";
    let mut state = vec![
        ("still-running".to_string(), true, 10),
        (finished.to_string(), false, 20),
        ("oldest".to_string(), false, 30),
    ];
    let hold = hold_from_retention(finished);
    apply_retention_holds(&mut state);
    assert_eq!(
        retention_plan_from_state(&state, 1, 0).candidates,
        vec!["oldest".to_string()]
    );

    drop(hold);
    state[1].1 = false;
    apply_retention_holds(&mut state);
    assert_eq!(
        retention_plan_from_state(&state, 1, 0).candidates,
        vec!["oldest".to_string(), finished.to_string()]
    );
}

#[test]
fn retention_skips_a_parent_held_after_the_plan_was_made() {
    let parent = "retention-test-parent";
    let _hold = hold_from_retention(parent);
    let trashed = std::sync::Mutex::new(Vec::new());
    let removed = trash_unheld(
        vec![parent.to_string(), "unheld".to_string()],
        |run_id: &str| {
            trashed.lock().unwrap().push(run_id.to_string());
            Ok(())
        },
    );
    assert_eq!(removed, 1);
    assert_eq!(*trashed.lock().unwrap(), vec!["unheld".to_string()]);
}

#[test]
fn run_order_follows_the_instant_across_offsets() {
    // The clocks went back: the later run has the smaller local time.
    let earlier = "2026-11-01T01:50:00-04:00".to_string();
    let later = "2026-11-01T01:10:00-05:00".to_string();
    let mut created = vec![earlier.clone(), "unreadable".to_string(), later.clone()];
    sort_newest_first(&mut created, |value| value.as_str());
    assert!(super::created_before(&earlier, &later));
    assert!(!super::created_before(&later, &earlier));
    assert!(super::created_before("unreadable", &earlier));
    assert_eq!(created, vec![later, earlier, "unreadable".to_string()]);
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

#[test]
fn manual_trash_honors_a_queued_rerun_before_touching_the_run_store() {
    let parent = "protected-parent";
    let _hold = hold_from_retention(parent);
    assert!(delete_run(parent)
        .unwrap_err()
        .contains("still needs this run"));
}

fn trashed_run(trash: &Path, run_id: &str) {
    fs::create_dir_all(trash.join(run_id)).unwrap();
    fs::write(trash.join(run_id).join("manifest.json"), "{}").unwrap();
}

#[test]
fn trash_deletes_runs_held_past_retention_and_dates_older_runs_from_first_sight() {
    let temp = tempfile::tempdir().unwrap();
    let trash = temp.path().join("trash/runs");
    for run_id in ["expired_run", "recent_run", "untimed_run"] {
        trashed_run(&trash, run_id);
    }
    let now = chrono::Utc::now();
    let keep_for = chrono::Duration::days(30);
    record_trashed_at(&trash, "expired_run", now - chrono::Duration::days(31)).unwrap();
    record_trashed_at(&trash, "recent_run", now - chrono::Duration::days(29)).unwrap();
    // Left behind by a restore that could not remove it.
    record_trashed_at(&trash, "restored_run", now - chrono::Duration::days(1)).unwrap();

    let (removed, failures) = empty_expired_trash_at(&trash, now, keep_for).unwrap();

    assert_eq!((removed, failures.len()), (1, 0));
    assert!(!trash.join("expired_run").exists());
    assert!(!trash.join("expired_run.trashed-at").exists());
    assert!(trash.join("recent_run").is_dir());
    assert!(!trash.join("restored_run.trashed-at").exists());
    // A run trashed before times were recorded starts its 30 days now,
    // however old its folder is.
    assert!(trash.join("untimed_run").is_dir());
    assert!(trash.join("untimed_run.trashed-at").is_file());

    let (removed, failures) = empty_expired_trash_at(&trash, now + keep_for, keep_for).unwrap();
    assert_eq!((removed, failures.len()), (2, 0));
    assert_eq!(fs::read_dir(&trash).unwrap().count(), 0);
}

#[test]
fn trash_lists_the_recorded_deletion_time_and_restore_clears_it() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let trash = temp.path().join("trash/runs");
    fs::create_dir_all(&runs).unwrap();
    trashed_run(&trash, "abc123_run");
    let moved = chrono::DateTime::parse_from_rfc3339("2026-10-01T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    record_trashed_at(&trash, "abc123_run", moved).unwrap();

    let listed = list_trashed_runs_in(&trash, chrono::Utc::now()).unwrap();
    assert_eq!(listed.len(), 1);
    let listed_at = chrono::DateTime::parse_from_rfc3339(&listed[0].deleted_at)
        .unwrap()
        .with_timezone(&chrono::Utc);
    assert_eq!(listed_at, moved);

    restore_trashed_run_at(&runs, &trash, "abc123_run").unwrap();
    assert!(runs.join("abc123_run/manifest.json").is_file());
    assert!(!trash.join("abc123_run.trashed-at").exists());
}

#[test]
fn run_folder_usage_counts_run_folders_but_not_loose_records() {
    let temp = tempfile::tempdir().unwrap();
    let trash = temp.path().join("trash/runs");
    fs::create_dir_all(trash.join("abc123_run/artifacts")).unwrap();
    fs::write(trash.join("abc123_run/report.md"), "12345").unwrap();
    fs::write(trash.join("abc123_run/artifacts/page.jpg"), "1234567890").unwrap();
    record_trashed_at(&trash, "abc123_run", chrono::Utc::now()).unwrap();

    let mut counted = Default::default();
    assert_eq!(
        run_folders_usage(&trash, "Test scan", &mut counted).unwrap(),
        (1, 15)
    );
    assert_eq!(
        run_folders_usage(&temp.path().join("missing"), "Test scan", &mut counted).unwrap(),
        (0, 0)
    );
}

#[test]
fn a_file_linked_from_history_into_trash_counts_once() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let trash = temp.path().join("trash/runs");
    fs::create_dir_all(runs.join("abc123_kept/context")).unwrap();
    fs::create_dir_all(trash.join("abc123_trashed/context")).unwrap();
    let shared = vec![7u8; 100 * 1024];
    fs::write(runs.join("abc123_kept/context/blocks.jsonl.gz"), &shared).unwrap();
    fs::hard_link(
        runs.join("abc123_kept/context/blocks.jsonl.gz"),
        trash.join("abc123_trashed/context/blocks.jsonl.gz"),
    )
    .unwrap();
    fs::write(trash.join("abc123_trashed/report.md"), "12345").unwrap();

    let mut counted = Default::default();
    assert_eq!(
        run_folders_usage(&runs, "Test scan", &mut counted).unwrap(),
        (1, shared.len() as u64)
    );
    // Trash adds only what History does not already hold.
    assert_eq!(
        run_folders_usage(&trash, "Test scan", &mut counted).unwrap(),
        (1, 5)
    );
}

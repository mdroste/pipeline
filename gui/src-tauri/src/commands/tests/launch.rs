//! Launch regression coverage.

use super::*;

#[test]
fn workflow_dependency_uses_live_connection_instead_of_cli_probe() {
    use crate::agent_runtime::codex::{AccountState, AccountStatus};
    use crate::deps::{dependency_ready, CliAuthStatus, DepStatus};
    use crate::pipeline::codex_server::connection::ConnectionStatus;

    let mut dep = DepStatus {
        name: "Codex CLI".into(),
        found: false,
        version: String::new(),
        path: "/test/codex".into(),
        required: true,
        hint: "Run codex login".into(),
        help_url: None,
        authenticated: None,
        cli_auth_status: Some(CliAuthStatus::Unknown),
    };
    let status = |account_status, login_in_progress| ConnectionStatus {
        account: AccountState {
            status: account_status,
            email: None,
            plan_type: None,
            unsupported_account_type: None,
            requires_openai_auth: true,
        },
        version: "0.153.4".into(),
        epoch: 1,
        home: "/test/workflows/codex".into(),
        login_in_progress,
        unresolved_attempts: Vec::new(),
    };
    run_context::apply_workflow_codex_status(&mut dep, Ok(status(AccountStatus::Chatgpt, false)));
    assert!(dependency_ready(&dep));
    assert_eq!(dep.name, "Workflow ChatGPT");
    assert_eq!(dep.cli_auth_status, None);
    assert!(dep.hint.is_empty());

    for account in [AccountStatus::SignedOut, AccountStatus::Unsupported] {
        // A previously signed-in CLI must not mask the managed account state.
        dep.cli_auth_status = Some(CliAuthStatus::SignedIn);
        run_context::apply_workflow_codex_status(&mut dep, Ok(status(account, false)));
        assert!(!dependency_ready(&dep));
        assert_eq!(dep.cli_auth_status, None);
        assert!(dep.hint.contains("Settings → Providers → ChatGPT"));
    }
    run_context::apply_workflow_codex_status(&mut dep, Ok(status(AccountStatus::Chatgpt, true)));
    assert!(!dependency_ready(&dep));
    assert!(dep.hint.contains("Complete Workflow ChatGPT sign-in"));

    let error = "Workflow ChatGPT connection is in use by another Pipeline process";
    run_context::apply_workflow_codex_status(&mut dep, Err(error.into()));
    assert!(!dependency_ready(&dep));
    assert_eq!(dep.hint, error);
    dep.required = false;
    assert!(dependency_ready(&dep));
}

#[test]
fn primary_input_validation_enforces_explicit_mode_path_kinds() {
    let root = tempfile::tempdir().unwrap();
    let document = root.path().join("paper.pdf");
    std::fs::write(&document, b"fixture").unwrap();
    let document = document.to_str().unwrap();
    let folder = root.path().to_str().unwrap();

    let mut config = empty_test_config();
    config.extraction.input_mode.clear();
    assert!(validate_primary_input_path(&config, Some(document)).is_ok());
    assert!(validate_primary_input_path(&config, Some(folder)).is_ok());

    config.extraction.input_mode = "document".to_string();
    assert!(validate_primary_input_path(&config, Some(document)).is_ok());
    assert!(validate_primary_input_path(&config, Some(folder)).is_err());

    config.extraction.input_mode = "folder".to_string();
    assert!(validate_primary_input_path(&config, Some(folder)).is_ok());
    assert!(validate_primary_input_path(&config, Some(document)).is_err());

    config.extraction.input_mode = "none".to_string();
    assert!(validate_primary_input_path(&config, Some("")).is_ok());
    assert!(validate_primary_input_path(&config, Some(document)).is_err());

    config.extraction.input_mode = "document".to_string();
    assert!(validate_primary_input_path(&config, Some("")).is_err());
    assert!(validate_primary_input_path(&config, None).is_ok());
}

#[test]
fn explicit_input_interpretation_separates_container_from_meaning() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("main.tex"),
        b"\\documentclass{article}\n\\begin{document}Paper\\end{document}",
    )
    .unwrap();
    let document = root.path().join("paper.pdf");
    std::fs::write(&document, b"fixture").unwrap();
    let config = empty_test_config();

    assert!(
        validate_primary_input_selection(&config, project.to_str(), Some("latex_project"),).is_ok()
    );
    assert!(
        validate_primary_input_selection(&config, project.to_str(), Some("source_tree"),).is_ok()
    );
    assert!(
        validate_primary_input_selection(&config, project.to_str(), Some("document"),).is_err()
    );
    assert!(
        validate_primary_input_selection(&config, document.to_str(), Some("document"),).is_ok()
    );
    assert_eq!(
        resolved_input_interpretation("document", project.to_str().unwrap(), Some("latex_project")),
        "latex_project"
    );
}

#[test]
fn named_input_validation_matches_backend_extraction_contract() {
    let root = tempfile::tempdir().unwrap();
    let document = root.path().join("APPENDIX.PDF");
    let unsupported = root.path().join("notes.txt");
    std::fs::write(&document, b"fixture").unwrap();
    std::fs::write(&unsupported, b"fixture").unwrap();
    let folder = root.path().join("sources");
    std::fs::create_dir(&folder).unwrap();

    let mut config = empty_test_config();
    config.extraction.extra_inputs = vec![
        crate::pipeline_config::InputSlot {
            key: "appendix".to_string(),
            label: "Appendix".to_string(),
            mode: "document".to_string(),
            required: true,
            ..Default::default()
        },
        crate::pipeline_config::InputSlot {
            key: "sources".to_string(),
            label: "Sources".to_string(),
            mode: "folder".to_string(),
            required: false,
            ..Default::default()
        },
    ];

    assert!(validate_named_input_paths(&config, &Default::default(), false).is_ok());
    assert!(validate_named_input_paths(&config, &Default::default(), true).is_err());

    let valid = std::collections::HashMap::from([
        (
            "appendix".to_string(),
            document.to_string_lossy().to_string(),
        ),
        ("sources".to_string(), folder.to_string_lossy().to_string()),
    ]);
    assert!(validate_named_input_paths(&config, &valid, true).is_ok());

    let wrong_extension = std::collections::HashMap::from([(
        "appendix".to_string(),
        unsupported.to_string_lossy().to_string(),
    )]);
    assert!(validate_named_input_paths(&config, &wrong_extension, true).is_err());

    let wrong_document_kind = std::collections::HashMap::from([(
        "appendix".to_string(),
        folder.to_string_lossy().to_string(),
    )]);
    assert!(validate_named_input_paths(&config, &wrong_document_kind, true).is_err());

    let wrong_folder_kind = std::collections::HashMap::from([
        (
            "appendix".to_string(),
            document.to_string_lossy().to_string(),
        ),
        (
            "sources".to_string(),
            document.to_string_lossy().to_string(),
        ),
    ]);
    assert!(validate_named_input_paths(&config, &wrong_folder_kind, true).is_err());
}

#[test]
fn headless_run_future_stays_behind_scheduler_boundary() {
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let future = run_headless(bus, "", Default::default(), Default::default());
    let size = std::mem::size_of_val(&future);
    assert!(
        size <= 8 * 1024,
        "run_headless future grew to {size} bytes; keep orchestration behind PipelineTask"
    );

    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let future = run_headless_with_options(bus, HeadlessRunOptions::default());
    let size = std::mem::size_of_val(&future);
    assert!(
        size <= 8 * 1024,
        "run_headless_with_options future grew to {size} bytes; keep orchestration behind PipelineTask"
    );

    let options = HeadlessRunOptions::default();
    let future = check_headless_dependencies(&options);
    let size = std::mem::size_of_val(&future);
    assert!(
        size <= 8 * 1024,
        "check_headless_dependencies future grew to {size} bytes"
    );
}

#[test]
fn basename_extracts_filename() {
    assert_eq!(basename("/papers/main.pdf"), "main.pdf");
    assert_eq!(basename("relative.tex"), "relative.tex");
    assert_eq!(basename(""), "");
}

#[test]
fn model_readable_temp_files_exclude_span_markup() {
    let root = tempfile::tempdir().unwrap();
    let (file, _) = write_run_input_file(
        root.path(),
        "context_",
        ".md",
        r#"<span id="page-1">Readable text</span>"#,
        "test context",
    )
    .unwrap();
    let content = std::fs::read_to_string(file.path()).unwrap();

    assert_eq!(content, "Readable text");
}

#[test]
fn batch_helpers_update_and_cancel_jobs() {
    {
        let mut jobs = BATCH.lock().unwrap();
        *jobs = vec![
            BatchJob {
                path: "a".into(),
                name: "a".into(),
                status: "done".into(),
                run_id: None,
                error: None,
                duration_secs: 0,
                profile_id: String::new(),
                profile_snapshot_id: String::new(),
            },
            BatchJob {
                path: "b".into(),
                name: "b".into(),
                status: "running".into(),
                run_id: None,
                error: None,
                duration_secs: 0,
                profile_id: String::new(),
                profile_snapshot_id: String::new(),
            },
            BatchJob {
                path: "c".into(),
                name: "c".into(),
                status: "pending".into(),
                run_id: None,
                error: None,
                duration_secs: 0,
                profile_id: String::new(),
                profile_snapshot_id: String::new(),
            },
        ];
    }
    set_job(1, |j| {
        j.status = "done".into();
        j.run_id = Some("r1".into());
    });
    mark_remaining_cancelled(1);
    let jobs = BATCH.lock().unwrap();
    assert_eq!(jobs[0].status, "done"); // untouched
    assert_eq!(jobs[1].status, "done"); // set_job ran before cancel; already terminal
    assert_eq!(jobs[1].run_id.as_deref(), Some("r1"));
    assert_eq!(jobs[2].status, "cancelled"); // pending → cancelled
}

#[test]
fn engine_installer_children_are_not_in_the_run_scoped_kill_list() {
    // A managed-engine install runs outside any pipeline run: ending or
    // cancelling a run (`kill_all_children`) must not see the installer's
    // subprocess. PIDs above i32::MAX are never signalled, so registration
    // bookkeeping can be exercised without touching a real process.
    let engine_pid = u32::MAX - 1;
    let run_pid = u32::MAX - 2;
    register_engine_child_pid(engine_pid);
    assert!(!lifecycle::CHILD_PIDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&engine_pid));
    unregister_engine_child_pid(engine_pid);

    register_child_pid(run_pid);
    assert!(lifecycle::CHILD_PIDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&run_pid));
    unregister_child_pid(run_pid);
}

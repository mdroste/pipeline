use super::*;

fn empty_test_config() -> PipelineConfig {
    PipelineConfig {
        steps: Vec::new(),
        merge: Default::default(),
        context_cache: Default::default(),
        use_orientation: false,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    }
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
        },
        crate::pipeline_config::InputSlot {
            key: "sources".to_string(),
            label: "Sources".to_string(),
            mode: "folder".to_string(),
            required: false,
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
fn runtime_snapshot_identity_is_order_stable_and_value_sensitive() {
    let snapshot = || RunSnapshot {
        settings: crate::settings::Settings::default(),
        config: empty_test_config(),
        profile_name: "test".to_string(),
        config_fingerprint: "profile-base".to_string(),
        fingerprint: "profile-base".to_string(),
    };
    let variables_a = std::collections::HashMap::from([
        ("alpha".to_string(), "one".to_string()),
        ("beta".to_string(), "two".to_string()),
    ]);
    let variables_b = std::collections::HashMap::from([
        ("beta".to_string(), "two".to_string()),
        ("alpha".to_string(), "one".to_string()),
    ]);
    let inputs =
        std::collections::HashMap::from([("rubric".to_string(), "/tmp/rubric.pdf".to_string())]);
    let first = bind_runtime_snapshot(snapshot(), &variables_a, &inputs)
        .unwrap()
        .fingerprint;
    let reordered = bind_runtime_snapshot(snapshot(), &variables_b, &inputs)
        .unwrap()
        .fingerprint;
    assert_eq!(first, reordered);

    let changed = bind_runtime_snapshot(
        snapshot(),
        &std::collections::HashMap::from([
            ("alpha".to_string(), "changed".to_string()),
            ("beta".to_string(), "two".to_string()),
        ]),
        &inputs,
    )
    .unwrap()
    .fingerprint;
    assert_ne!(first, changed);

    let first_launch = bind_foreground_launch(
        bind_runtime_snapshot(snapshot(), &variables_a, &inputs).unwrap(),
        "/papers/a.pdf",
        None,
        false,
    )
    .unwrap()
    .fingerprint;
    let other_path = bind_foreground_launch(
        bind_runtime_snapshot(snapshot(), &variables_a, &inputs).unwrap(),
        "/papers/b.pdf",
        None,
        false,
    )
    .unwrap()
    .fingerprint;
    let other_diff = bind_foreground_launch(
        bind_runtime_snapshot(snapshot(), &variables_a, &inputs).unwrap(),
        "/papers/a.pdf",
        None,
        true,
    )
    .unwrap()
    .fingerprint;
    assert_ne!(first_launch, other_path);
    assert_ne!(first_launch, other_diff);
}

#[test]
fn snapshot_fingerprint_tracks_secrets_without_exposing_them() {
    let mut settings = crate::settings::Settings {
        openai_api_key: "raw-secret-one".to_string(),
        ..Default::default()
    };
    let first = settings_for_snapshot_fingerprint(&settings);
    let serialized = serde_json::to_string(&first).unwrap();
    assert!(!serialized.contains("raw-secret-one"));
    assert!(first.openai_api_key.starts_with("<digest:"));

    settings.openai_api_key = "raw-secret-two".to_string();
    let second = settings_for_snapshot_fingerprint(&settings);
    assert_ne!(first.openai_api_key, second.openai_api_key);
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
fn print_html_is_self_contained_and_waits_for_fonts() {
    let html = build_print_report_html(
            "**#12. Identification needs work**\n\nAn equation: $y=x$.\n\n![Remote figure](https://example.invalid/pixel.png)",
            Some("# Pipeline\n\n**Document:** A paper\n\n**Workflow:** Full review\n\n## Run provenance\n\nBOTTOM DETAILS MUST NOT PRINT"),
        )
        .unwrap();
    assert!(html.contains("data:font/woff2;base64,"));
    assert!(html.contains("document.fonts.ready"));
    assert!(html.contains("class=\"report-document\""));
    assert!(html.contains("class=\"report-masthead\""));
    assert!(html.contains("class=\"report-body\""));
    assert!(!html.contains("class=\"report-provenance\""));
    assert!(!html.contains("BOTTOM DETAILS MUST NOT PRINT"));
    assert!(html.contains("class=\"comment-header\""));
    assert!(html.contains("class=\"comment-num\">12</span>"));
    assert!(!html.contains("<p><strong>#12."));
    assert!(html.contains("-apple-system"));
    assert!(html.contains("Iowan Old Style"));
    assert!(html.contains("@page { margin: 0 0 0.28in; }"));
    assert!(html.contains("counter(page) \" / \" counter(pages)"));
    assert!(html.contains("padding: 0.72in 0.82in 0.7in"));
    assert!(html.contains("box-decoration-break: clone"));
    assert!(html.contains("Remote figure"));
    assert!(!html.contains("https://example.invalid/pixel.png"));
    assert!(!html.contains("url(fonts/"));
    assert!(!html.contains("cdn.jsdelivr"));
    assert!(!html.contains("<script src="));
    assert!(!html.contains("<link rel=\"stylesheet\""));
}

#[test]
fn print_html_preserves_latex_until_katex_rendering() {
    let html = build_print_report_html(
            "Inline $x_t^* = \\frac{a_b}{c^2}$ and display:\n\n$$\\sum_{i=1}^n \\beta_i x_i$$\n\n`$code_with_underscore$`",
            Some("# Referee report\n\n## Run provenance\n\n| Provider | Model | Input | Output | Cached input |\n| --- | --- | ---: | ---: | ---: |\n| Codex | gpt-5.6-sol | 100 | 20 | 80 |"),
        )
        .unwrap();

    assert!(html.contains("$x_t^* = \\frac{a_b}{c^2}$"));
    assert!(html.contains("$$\\sum_{i=1}^n \\beta_i x_i$$"));
    assert!(html.contains("<code>$code_with_underscore$</code>"));
    assert!(!html.contains("x<em>t"));
    assert!(!html.contains("PIPELINEMATHPLACEHOLDER"));
    assert!(!html.contains("Run provenance"));
    assert!(!html.contains("gpt-5.6-sol"));
    assert!(html.contains("'\\\\bm':'\\\\boldsymbol'"));
}

#[test]
fn directory_export_is_complete_and_does_not_replace_existing_output() {
    let source = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(source.path().join("context")).unwrap();
    std::fs::write(source.path().join("manifest.json"), b"manifest").unwrap();
    std::fs::write(source.path().join("context/report.md"), b"report").unwrap();
    let destination = tempfile::tempdir().unwrap();
    let existing = destination.path().join("pipeline-run-test");
    std::fs::create_dir(&existing).unwrap();
    std::fs::write(existing.join("keep.txt"), b"keep").unwrap();
    let stage = create_export_stage(destination.path()).unwrap();

    let (files, bytes) = copy_export_tree(source.path(), stage.path()).unwrap();
    let exported = finish_export_stage(&stage, destination.path(), "pipeline-run-test").unwrap();
    drop(stage);

    assert_eq!(files, 2);
    assert_eq!(bytes, 14);
    assert_ne!(exported, existing);
    assert_eq!(std::fs::read(existing.join("keep.txt")).unwrap(), b"keep");
    assert_eq!(
        std::fs::read(exported.join("context/report.md")).unwrap(),
        b"report"
    );
}

#[test]
fn rerun_cache_preserves_every_output_for_a_base_step() {
    let outputs = vec![
        crate::models::StepOutput {
            step_id: "review/claude".into(),
            raw_text: "one".into(),
            ..Default::default()
        },
        crate::models::StepOutput {
            step_id: "review/gemini".into(),
            raw_text: "two".into(),
            ..Default::default()
        },
    ];
    let cache = collect_preloaded_outputs(&outputs, &Default::default());
    assert_eq!(cache["review"].len(), 2);
}

#[test]
fn composite_failure_ids_normalize_to_the_base_step() {
    let failures = vec![crate::models::StepFailure {
        step_id: "review/gemini".into(),
        step_label: "Review (Gemini)".into(),
        error: "failed".into(),
    }];
    let seeds = failed_base_ids(&failures);
    assert_eq!(seeds, ["review".to_string()].into_iter().collect());
}

#[test]
fn resume_starts_at_the_first_missing_step_after_recovery() {
    let report = PipelineReport {
        orientation: serde_json::Value::Null,
        step_outputs: vec![crate::models::StepOutput {
            step_id: "technical".into(),
            raw_text: "complete".into(),
            ..Default::default()
        }],
        failed_steps: vec![crate::models::StepFailure {
            step_id: "__run_cancelled__".into(),
            step_label: "Run cancelled".into(),
            error: "cancelled".into(),
        }],
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "test".into(),
    };
    let enabled = vec![
        "technical".to_string(),
        "empirical".to_string(),
        "synthesis".to_string(),
    ];
    let seeds = resume_seed_ids(&enabled, &report);
    assert!(!seeds.contains("technical"));
    assert!(seeds.contains("empirical"));
    assert!(seeds.contains("synthesis"));
}

#[test]
fn chunked_import_limit_is_enforced_before_appending() {
    let mut buffer = vec![0u8; 8];
    assert!(append_limited(&mut buffer, &[1, 2], 10).is_ok());
    assert_eq!(buffer.len(), 10);
    assert!(append_limited(&mut buffer, &[3], 10).is_err());
    assert_eq!(buffer.len(), 10);
}

#[test]
fn profile_url_import_rejects_non_public_destinations() {
    for address in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.0.1",
        "169.254.1.1",
        "100.64.0.1",
        "192.0.2.1",
        "198.18.0.1",
        "198.51.100.1",
        "203.0.113.1",
        "::1",
        "fe80::1",
        "fc00::1",
        "2001:db8::1",
        "::ffff:127.0.0.1",
    ] {
        let address = address.parse().unwrap();
        assert!(!import_ip_is_public(address), "{address}");
    }
    for address in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
        let address = address.parse().unwrap();
        assert!(import_ip_is_public(address), "{address}");
    }
}

#[test]
fn profile_url_import_rejects_credentials_and_localhost() {
    for url in [
        "file:///tmp/profile.json",
        "http://example.com/profile.json",
        "http://localhost/profile.json",
        "http://worker.localhost/profile.json",
        "https://user:secret@example.com/profile.json",
    ] {
        let parsed = reqwest::Url::parse(url).unwrap();
        assert!(validate_import_url_shape(parsed).is_err(), "{url}");
    }
    let normalized = validate_import_url_shape(
        reqwest::Url::parse("https://example.com./profile.json#fragment").unwrap(),
    )
    .unwrap();
    assert_eq!(normalized.host_str(), Some("example.com"));
    assert!(normalized.fragment().is_none());
}

// Exercises the batch job-status helpers over the global BATCH state. This
// is the only test that touches BATCH, so parallel test runs can't race it.
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

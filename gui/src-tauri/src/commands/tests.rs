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

fn empty_test_config() -> PipelineConfig {
    PipelineConfig {
        steps: Vec::new(),
        merge: Default::default(),
        outputs: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
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
fn runtime_snapshot_identity_is_order_stable_and_value_sensitive() {
    let snapshot = || {
        let mut config = empty_test_config();
        config.variables = ["alpha", "beta"]
            .into_iter()
            .map(|key| crate::pipeline_config::VarSpec {
                key: key.to_string(),
                ..Default::default()
            })
            .collect();
        config.extraction.extra_inputs = vec![crate::pipeline_config::InputSlot {
            key: "rubric".to_string(),
            ..Default::default()
        }];
        RunSnapshot {
            settings: crate::settings::Settings::default(),
            config,
            profile_name: "test".to_string(),
            config_fingerprint: "profile-base".to_string(),
            fingerprint: "profile-base".to_string(),
            workflow_source: String::new(),
            workflow_fingerprint: String::new(),
            workflow_json: String::new(),
            specialist_catalog_revision: String::new(),
        }
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
fn profile_snapshot_identity_is_stable_across_nested_override_map_order() {
    let selections = |first: &str, second: &str| {
        let mut values = std::collections::HashMap::new();
        for key in [first, second] {
            values.insert(
                key.to_string(),
                crate::settings::ModelSelection::Pinned {
                    model: format!("{key}-model"),
                },
            );
        }
        values
    };
    let efforts = |entries: [(&str, &str); 2]| {
        let mut values = std::collections::HashMap::new();
        for (key, effort) in entries {
            values.insert(key.to_string(), effort.to_string());
        }
        values
    };

    let settings_a = crate::settings::Settings {
        default_parallel_model_overrides: selections("claude:cli", "codex:cli"),
        default_parallel_effort_overrides: efforts([
            ("claude:cli", "high"),
            ("codex:cli", "medium"),
        ]),
        ..Default::default()
    };
    let settings_b = crate::settings::Settings {
        default_parallel_model_overrides: selections("codex:cli", "claude:cli"),
        default_parallel_effort_overrides: efforts([
            ("codex:cli", "medium"),
            ("claude:cli", "high"),
        ]),
        ..Default::default()
    };

    let step = |model_overrides, effort_overrides| crate::pipeline_config::StepConfig {
        id: "parallel".into(),
        phase: crate::pipeline_config::Phase::Parallel,
        model_overrides,
        effort_overrides,
        ..Default::default()
    };
    let mut config_a = empty_test_config();
    config_a.steps = vec![step(
        selections("claude:cli", "codex:cli"),
        efforts([("claude:cli", "high"), ("codex:cli", "medium")]),
    )];
    let mut config_b = empty_test_config();
    config_b.steps = vec![step(
        selections("codex:cli", "claude:cli"),
        efforts([("codex:cli", "medium"), ("claude:cli", "high")]),
    )];

    let first = stable_snapshot_fingerprint(&(settings_a, config_a)).unwrap();
    let reordered = stable_snapshot_fingerprint(&(settings_b, config_b)).unwrap();
    assert_eq!(first, reordered);
}

#[test]
fn parallel_override_snapshot_identity_is_order_stable_and_value_sensitive() {
    let snapshot = || RunSnapshot {
        settings: crate::settings::Settings::default(),
        config: empty_test_config(),
        profile_name: "test".to_string(),
        config_fingerprint: "profile-base".to_string(),
        fingerprint: "profile-base".to_string(),
        workflow_source: String::new(),
        workflow_fingerprint: String::new(),
        workflow_json: String::new(),
        specialist_catalog_revision: String::new(),
    };
    let overrides = |entries: [(&str, &str, &str); 2]| {
        let mut model_overrides = std::collections::HashMap::new();
        let mut effort_overrides = std::collections::HashMap::new();
        for (key, model, effort) in entries {
            model_overrides.insert(
                key.to_string(),
                crate::settings::ModelSelection::Pinned {
                    model: model.to_string(),
                },
            );
            effort_overrides.insert(key.to_string(), effort.to_string());
        }
        RunParallelOverrides {
            agents: vec!["claude".into(), "codex".into()],
            model_overrides,
            effort_overrides,
            ..Default::default()
        }
    };

    let first = bind_parallel_overrides(
        snapshot(),
        Some(&overrides([
            ("claude:cli", "claude-model", "high"),
            ("codex:cli", "codex-model", "medium"),
        ])),
    )
    .unwrap()
    .fingerprint;
    let reordered = bind_parallel_overrides(
        snapshot(),
        Some(&overrides([
            ("codex:cli", "codex-model", "medium"),
            ("claude:cli", "claude-model", "high"),
        ])),
    )
    .unwrap()
    .fingerprint;
    assert_eq!(first, reordered);

    let changed = bind_parallel_overrides(
        snapshot(),
        Some(&overrides([
            ("claude:cli", "different-model", "high"),
            ("codex:cli", "codex-model", "medium"),
        ])),
    )
    .unwrap()
    .fingerprint;
    assert_ne!(first, changed);
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
    assert!(html.contains(".report-body p { text-align: justify; hyphens: auto; }"));
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
            "Inline $x_t^* = \\frac{a_b}{c^2}$ and display:\n\n$$\\sum_{i=1}^n \\beta_i x_i$$\n\nTariffs rose from $5 to $8 per unit.\n\n`$code_with_underscore$`",
            Some("# Referee report\n\n## Run provenance\n\n| Provider | Model | Input | Output | Cached input |\n| --- | --- | ---: | ---: | ---: |\n| Codex | gpt-5.6-sol | 100 | 20 | 80 |"),
        )
        .unwrap();

    // Protected math is restored with backslash delimiters so the client-side
    // KaTeX pass does not scan for `$`, leaving currency amounts as prose.
    assert!(html.contains("\\(x_t^* = \\frac{a_b}{c^2}\\)"));
    assert!(html.contains("\\[\\sum_{i=1}^n \\beta_i x_i\\]"));
    assert!(html.contains("$5 to $8 per unit"));
    assert!(!html.contains("{left:'$'"));
    assert!(!html.contains("{left:'$$'"));
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

fn export_test_manifest() -> crate::runs::RunManifest {
    serde_json::from_value(serde_json::json!({
        "run_id": "export-test-run",
        "created": "2026-08-23T12:00:00Z",
        "input_path": "/Users/private/TOPSECRET-paper.pdf",
        "input_mode": "document",
        "profile_id": "review",
        "profile_name": "Review",
        "provider": "codex",
        "status": "done",
        "variables": { "private_note": "TOPSECRET-variable" },
        "artifacts": [
            { "rel_path": "report.md", "label": "Report", "kind": "markdown", "bytes": 1, "sha256": "a", "group": "report" },
            { "rel_path": "report.json", "label": "Report data", "kind": "json", "bytes": 1, "sha256": "b", "group": "context" },
            { "rel_path": "context/document.md", "label": "Document", "kind": "markdown", "bytes": 1, "sha256": "c", "group": "document" },
            { "rel_path": "artifacts/01_review.md", "label": "Review", "kind": "markdown", "bytes": 1, "sha256": "d", "group": "step" },
            { "rel_path": "logs/run.log", "label": "Log", "kind": "text", "bytes": 1, "sha256": "e", "group": "context" }
        ]
    }))
    .unwrap()
}

fn write_export_test_run(source: &std::path::Path) {
    std::fs::create_dir_all(source.join("context")).unwrap();
    std::fs::create_dir_all(source.join("artifacts")).unwrap();
    std::fs::create_dir_all(source.join("logs")).unwrap();
    std::fs::write(source.join("report.md"), "# Safe rendered report\n").unwrap();
    std::fs::write(source.join("context/document.md"), "TOPSECRET-source-bytes").unwrap();
    std::fs::write(
        source.join("artifacts/01_review.md"),
        "TOPSECRET-raw-response",
    )
    .unwrap();
    std::fs::write(source.join("logs/run.log"), "TOPSECRET-log").unwrap();
    std::fs::write(source.join("manifest.json"), "TOPSECRET-manifest").unwrap();
    let report = PipelineReport {
        orientation: serde_json::Value::Null,
        step_outputs: Vec::new(),
        failed_steps: Vec::new(),
        products: crate::models::RunProducts {
            schema_version: 1,
            findings: Some(crate::models::FindingSet {
                schema_version: 1,
                findings: vec![
                    crate::models::Finding {
                        id: "verified".into(),
                        title: "Verified finding".into(),
                        verification_status: "verified".into(),
                        evidence: vec![crate::models::FindingEvidence {
                            verification_status: "verified_with_normalization".into(),
                            page: Some(4),
                            quote: "Supported statement".into(),
                            artifact_path: "context/document.md".into(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                    crate::models::Finding {
                        id: "unverified".into(),
                        title: "TOPSECRET-unverified-finding".into(),
                        verification_status: "unverified".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        },
        quality: crate::models::ReportQuality {
            status: "done".into(),
            verified_evidence: 1,
            unverified_evidence: 1,
            limitations: vec!["One item remains unverified.".into()],
            ..Default::default()
        },
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "test".into(),
    };
    std::fs::write(
        source.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[test]
fn shareable_export_has_an_exact_safe_allowlist_and_no_secret_payloads() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    write_export_test_run(source.path());
    let manifest = export_test_manifest();

    copy_export_file(source.path(), destination.path(), "report.md").unwrap();
    write_shareable_products(source.path(), destination.path(), &manifest, true, true).unwrap();
    finalize_export_metadata(
        destination.path(),
        &manifest.run_id,
        ExportMode::Shareable,
        &ExportSelection::default(),
    )
    .unwrap();

    let files = enumerate_export_files(destination.path()).unwrap();
    let paths = files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "checksums.sha256",
            "export-manifest.json",
            "limitations.md",
            "provenance.json",
            "quality.json",
            "report.md",
            "verified-findings.json",
        ]
    );
    let exported_text = files
        .iter()
        .map(|file| std::fs::read_to_string(destination.path().join(&file.path)).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!exported_text.contains("TOPSECRET"));
    assert!(exported_text.contains("Verified finding"));
    assert!(!exported_text.contains("context/document.md"));
    assert!(!destination.path().join("report.json").exists());
    assert!(!destination.path().join("logs/run.log").exists());
}

#[test]
fn custom_export_copies_only_explicit_artifact_classes() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    write_export_test_run(source.path());
    let manifest = export_test_manifest();
    let selection = ExportSelection {
        report: true,
        source_documents: true,
        ..Default::default()
    };
    write_custom_payload(source.path(), destination.path(), &manifest, &selection).unwrap();
    let paths = enumerate_export_files(destination.path())
        .unwrap()
        .into_iter()
        .map(|file| file.path)
        .collect::<Vec<_>>();
    assert_eq!(paths, ["context/document.md", "report.md"]);
    assert!(!destination.path().join("report.json").exists());
    assert!(!destination.path().join("artifacts/01_review.md").exists());
    assert!(!destination.path().join("logs/run.log").exists());
}

#[test]
fn forensic_export_enumerates_the_complete_run_and_labels_it_sensitive() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    write_export_test_run(source.path());
    copy_export_tree(source.path(), destination.path()).unwrap();
    finalize_export_metadata(
        destination.path(),
        "export-test-run",
        ExportMode::Forensic,
        &ExportSelection::default(),
    )
    .unwrap();
    let paths = enumerate_export_files(destination.path())
        .unwrap()
        .into_iter()
        .map(|file| file.path)
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "artifacts/01_review.md",
            "checksums.sha256",
            "context/document.md",
            "export-manifest.json",
            "logs/run.log",
            "manifest.json",
            "report.json",
            "report.md",
        ]
    );
    let export_manifest =
        std::fs::read_to_string(destination.path().join("export-manifest.json")).unwrap();
    assert!(export_manifest.contains(r#""sensitivity": "sensitive""#));
    assert!(export_manifest.contains(r#""includesSourceMaterial": true"#));
    assert!(export_manifest.contains(r#""includesRawResponses": true"#));
    assert!(export_manifest.contains(r#""includesLogs": true"#));
    assert_eq!(
        std::fs::read_to_string(destination.path().join("manifest.json")).unwrap(),
        "TOPSECRET-manifest"
    );
}

#[test]
fn reveal_validation_accepts_only_the_exact_recent_export() {
    let root = tempfile::tempdir().unwrap();
    let exported = root.path().join("package");
    std::fs::create_dir(&exported).unwrap();
    record_export_path(&exported);
    assert_eq!(
        validated_recent_export(&exported.to_string_lossy()).unwrap(),
        exported.canonicalize().unwrap()
    );
    assert!(validated_recent_export(&root.path().to_string_lossy()).is_err());
}

#[test]
fn core_export_writes_only_the_canonical_document_name() {
    let destination = tempfile::tempdir().unwrap();
    let report = PipelineReport {
        orientation: serde_json::Value::Null,
        step_outputs: Vec::new(),
        failed_steps: Vec::new(),
        products: Default::default(),
        quality: Default::default(),
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "test".into(),
    };

    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(save_all_artifacts(
            destination.path().to_string_lossy().into_owned(),
            "# Report".into(),
            "exact document".into(),
            report,
        ))
        .unwrap();

    let exported = destination.path().join("pipeline-core-export");
    assert_eq!(
        std::fs::read_to_string(exported.join("document.md")).unwrap(),
        "exact document"
    );
    assert!(!exported.join("extracted_text.md").exists());
}

#[test]
fn core_export_includes_the_canonical_findings_product() {
    let destination = tempfile::tempdir().unwrap();
    let report = PipelineReport {
        orientation: serde_json::Value::Null,
        step_outputs: Vec::new(),
        failed_steps: Vec::new(),
        products: crate::models::RunProducts {
            schema_version: 1,
            primary_step_id: "final".into(),
            findings: Some(crate::models::FindingSet {
                schema_version: 1,
                source_step_id: "final".into(),
                source_step_label: "Final".into(),
                taxonomy: Vec::new(),
                findings: Vec::new(),
            }),
            validation_dispositions: Vec::new(),
            named: Vec::new(),
        },
        quality: Default::default(),
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "test".into(),
    };

    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(save_all_artifacts(
            destination.path().to_string_lossy().into_owned(),
            "# Report".into(),
            "exact document".into(),
            report,
        ))
        .unwrap();

    let findings = std::fs::read_to_string(
        destination
            .path()
            .join("pipeline-core-export/findings.json"),
    )
    .unwrap();
    assert!(findings.contains(r#""source_step_id": "final""#));
}

#[test]
fn core_export_writes_structured_step_artifacts_as_standalone_json() {
    let destination = tempfile::tempdir().unwrap();
    let report = PipelineReport {
        orientation: serde_json::json!({}),
        step_outputs: vec![crate::models::StepOutput {
            step_id: "structured".into(),
            step_label: "Structured result".into(),
            raw_text: "{\n  \"result\": \"ok\"\n}".into(),
            structured_json: true,
            ..Default::default()
        }],
        failed_steps: Vec::new(),
        products: Default::default(),
        quality: Default::default(),
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "test".into(),
    };

    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(save_all_artifacts(
            destination.path().to_string_lossy().into_owned(),
            "# Report".into(),
            "document".into(),
            report,
        ))
        .unwrap();

    let artifact = std::fs::read_to_string(
        destination
            .path()
            .join("pipeline-core-export/steps/01_structured.json"),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&artifact).unwrap(),
        serde_json::json!({"result": "ok"})
    );
    assert!(!destination
        .path()
        .join("pipeline-core-export/steps/01_structured.md")
        .exists());
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
            step_id: "review/antigravity".into(),
            raw_text: "two".into(),
            ..Default::default()
        },
    ];
    let cache = collect_preloaded_outputs(&outputs, &Default::default());
    assert_eq!(cache["review"].len(), 2);
}

#[test]
fn rerun_cache_invalidates_artifacts_when_the_output_contract_changes() {
    let mut config = empty_test_config();
    config.steps = vec![crate::pipeline_config::StepConfig {
        id: "review".into(),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "required": ["verdict"],
            "properties": {"verdict": {"type": "string"}}
        })),
        ..Default::default()
    }];
    let markdown_parent = crate::models::StepOutput {
        step_id: "review".into(),
        raw_text: "# Old report".into(),
        structured_json: false,
        ..Default::default()
    };
    assert!(incompatible_reuse_ids(&config, &[markdown_parent]).contains("review"));

    let valid_json = crate::models::StepOutput {
        step_id: "review".into(),
        raw_text: r#"{"verdict":"accept"}"#.into(),
        structured_json: true,
        ..Default::default()
    };
    assert!(!incompatible_reuse_ids(&config, &[valid_json]).contains("review"));

    let invalid_json = crate::models::StepOutput {
        step_id: "review".into(),
        raw_text: r#"{"score":1}"#.into(),
        structured_json: true,
        ..Default::default()
    };
    assert!(incompatible_reuse_ids(&config, &[invalid_json]).contains("review"));
}

#[test]
fn composite_failure_ids_normalize_to_the_base_step() {
    let failures = vec![crate::models::StepFailure {
        step_id: "review/antigravity".into(),
        step_label: "Review (Antigravity)".into(),
        phase: "parallel".into(),
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
            phase: String::new(),
            error: "cancelled".into(),
        }],
        products: Default::default(),
        quality: Default::default(),
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

#[test]
fn one_run_parallel_override_replaces_explicit_and_inherited_parallel_agents() {
    let mut config = empty_test_config();
    config.steps = vec![
        crate::pipeline_config::StepConfig {
            id: "explicit".into(),
            phase: crate::pipeline_config::Phase::Parallel,
            agents: vec!["claude".into()],
            model: "old-model".into(),
            ..Default::default()
        },
        crate::pipeline_config::StepConfig {
            id: "inherited".into(),
            phase: crate::pipeline_config::Phase::Parallel,
            ..Default::default()
        },
        crate::pipeline_config::StepConfig {
            id: "sequential".into(),
            phase: crate::pipeline_config::Phase::Sequential,
            agents: vec!["antigravity".into()],
            ..Default::default()
        },
    ];
    let snapshot = RunSnapshot {
        settings: crate::settings::Settings::default(),
        config,
        profile_name: "Test".into(),
        config_fingerprint: "base".into(),
        fingerprint: "base".into(),
        workflow_source: String::new(),
        workflow_fingerprint: String::new(),
        workflow_json: String::new(),
        specialist_catalog_revision: String::new(),
    };
    let overrides = RunParallelOverrides {
        agents: vec!["codex".into(), "antigravity".into()],
        model_overrides: std::collections::HashMap::from([(
            "codex:cli".into(),
            crate::settings::ModelSelection::Pinned {
                model: "gpt-exact".into(),
            },
        )]),
        effort_overrides: std::collections::HashMap::from([("codex:cli".into(), "high".into())]),
        ..Default::default()
    };

    let bound = bind_parallel_overrides(snapshot, Some(&overrides)).unwrap();

    for step in &bound.config.steps[..2] {
        assert_eq!(step.agents, vec!["codex", "antigravity"]);
        assert!(step.model.is_empty());
        assert_eq!(
            step.model_overrides["codex:cli"],
            crate::settings::ModelSelection::Pinned {
                model: "gpt-exact".into()
            }
        );
    }
    assert_eq!(bound.config.steps[2].agents, vec!["antigravity"]);
    assert_ne!(bound.fingerprint, "base");
    assert_eq!(bound.config_fingerprint, "base");
}

#[test]
fn one_run_merge_override_replaces_the_workflow_merge_provider_and_policy() {
    let mut config = empty_test_config();
    config.merge.agents = vec!["antigravity".into()];
    let snapshot = RunSnapshot {
        settings: crate::settings::Settings::default(),
        config,
        profile_name: "Test".into(),
        config_fingerprint: "base".into(),
        fingerprint: "base".into(),
        workflow_source: String::new(),
        workflow_fingerprint: String::new(),
        workflow_json: String::new(),
        specialist_catalog_revision: String::new(),
    };
    let overrides = RunParallelOverrides {
        agents: vec!["claude".into(), "codex".into()],
        merge_agent: Some("codex".into()),
        merge_model_overrides: std::collections::HashMap::from([(
            "codex:cli".into(),
            crate::settings::ModelSelection::Pinned {
                model: "gpt-merge".into(),
            },
        )]),
        merge_effort_overrides: std::collections::HashMap::from([(
            "codex:cli".into(),
            "high".into(),
        )]),
        ..Default::default()
    };

    let bound = bind_parallel_overrides(snapshot, Some(&overrides)).unwrap();

    assert!(bound.config.merge.agents.is_empty());
    assert_eq!(bound.settings.merge_agent(), "codex");
    assert_eq!(
        bound.settings.merge_model_selection("codex"),
        Some(crate::settings::ModelSelection::Pinned {
            model: "gpt-merge".into()
        })
    );
    assert_eq!(bound.settings.merge_effort("codex"), "high");
    assert_ne!(bound.fingerprint, "base");
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

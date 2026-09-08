//! Snapshots regression coverage.

use super::*;

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

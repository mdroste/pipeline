//! Profiles regression coverage.

use super::*;

#[test]
fn unique_step_ids_pass_validation() {
    let steps = vec![step_with_id("a"), step_with_id("b")];
    assert!(validate_unique_step_ids(&steps).is_ok());
}

#[test]
fn duplicate_step_ids_are_rejected() {
    let steps = vec![step_with_id("a"), step_with_id("b"), step_with_id("a")];
    let err = validate_unique_step_ids(&steps).unwrap_err();
    assert!(err.contains("Duplicate step id 'a'"), "{err}");
}

#[test]
fn invalid_step_ids_are_rejected_instead_of_rewritten() {
    let step = StepConfig {
        id: "a/b".to_string(),
        ..Default::default()
    };
    assert!(validate_unique_step_ids(&[step]).is_err());

    let mut empty = StepConfig::default();
    empty.id.clear();
    assert!(validate_unique_step_ids(&[empty]).is_err());
}

#[test]
fn bundle_preflight_rejects_bad_graphs_duplicates_and_missing_active_profile() {
    let cycle = vec![step_dep("a", &["b"]), step_dep("b", &["a"])];
    assert!(validate_bundle_profiles(&[bundle_profile("one", cycle)], "one").is_err());

    let duplicate_ids = vec![
        bundle_profile("same", Vec::new()),
        bundle_profile("same", Vec::new()),
    ];
    assert!(validate_bundle_profiles(&duplicate_ids, "same").is_err());

    let valid = vec![bundle_profile(
        "one",
        vec![StepConfig {
            id: "synthesis".into(),
            phase: Phase::Sequential,
            ..Default::default()
        }],
    )];
    assert!(validate_bundle_profiles(&valid, "missing").is_err());
    assert!(validate_bundle_profiles(&valid, "one").is_ok());
}

#[test]
fn bundle_preflight_rejects_invalid_profile_metadata() {
    let mut profile = bundle_profile("one", Vec::new());
    profile.variables = vec![
        VarSpec {
            key: "topic".into(),
            label: String::new(),
            kind: "text".into(),
            default: String::new(),
            choices: Vec::new(),
            ..Default::default()
        },
        VarSpec {
            key: "topic".into(),
            label: String::new(),
            kind: "text".into(),
            default: String::new(),
            choices: Vec::new(),
            ..Default::default()
        },
    ];
    assert!(validate_bundle_profiles(&[profile], "one").is_err());

    let mut profile = bundle_profile("one", Vec::new());
    profile.extraction.input_mode = "socket".into();
    assert!(validate_bundle_profiles(&[profile], "one").is_err());
}

#[test]
fn secret_variables_cannot_serialize_defaults() {
    let mut profile = bundle_profile("one", Vec::new());
    profile.variables.push(VarSpec {
        key: "access_token".into(),
        label: "Access token".into(),
        kind: "text".into(),
        default: "must-not-be-saved".into(),
        secret: true,
        ..Default::default()
    });
    let error = validate_bundle_profiles(&[profile], "one").unwrap_err();
    assert!(error.contains("cannot have a saved default"), "{error}");
}

#[test]
fn duplicated_profile_preserves_variable_declarations() {
    let mut source = ProfileData::new("Source", vec![], MergeConfig::default());
    source.variables.push(VarSpec {
        key: "journal".into(),
        label: "Journal".into(),
        kind: "choice".into(),
        default: "AER".into(),
        choices: vec!["AER".into(), "QJE".into()],
        ..Default::default()
    });
    let duplicate = duplicate_profile_data(source, "Copy");
    assert_eq!(duplicate.name, "Copy");
    assert_eq!(duplicate.variables.len(), 1);
    assert_eq!(duplicate.variables[0].key, "journal");
    assert_eq!(duplicate.variables[0].choices.len(), 2);
}

#[test]
fn empty_current_profile_round_trips() {
    let profile = ProfileData::new("Empty", Vec::new(), MergeConfig::default());
    let json = serde_json::to_string(&profile).unwrap();
    let decoded: ProfileData = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.name, "Empty");
    assert!(decoded.steps.is_empty());
    assert!(!decoded.context_cache.enabled);
}

#[test]
fn current_profiles_serialize_explicit_artifact_context_only() {
    let profile = ProfileData::new("Current", default_steps(), MergeConfig::default());
    validate_profile_data(&profile).unwrap();
    let value = serde_json::to_value(&profile).unwrap();
    for step in value["steps"].as_array().unwrap() {
        assert!(step.get("context").is_some());
        assert!(step.get("inputs").is_none());
        assert!(step["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| !matches!(tool.as_str(), Some("Read" | "Write"))));
    }
    let synthesis = profile
        .steps
        .iter()
        .find(|step| step.id == "editor_synthesis")
        .unwrap();
    assert!(synthesis.context.include.iter().any(|selector| {
        matches!(
            selector,
            ArtifactSelector::Step {
                step,
                parts,
                ..
            } if step == "technical" && parts.contains(&StepArtifactPart::Report)
        )
    }));
}

#[test]
fn context_cache_is_opt_in_and_backward_compatible() {
    let legacy = r#"{
            "name":"Legacy",
            "steps":[],
            "merge":{"enabled":false,"prompt":"","agents":[]}
        }"#;
    let decoded: ProfileData = serde_json::from_str(legacy).unwrap();
    assert!(!decoded.context_cache.enabled);

    let mut profile = ProfileData::new("Cached", Vec::new(), MergeConfig::default());
    profile.context_cache.enabled = true;
    let json = serde_json::to_string(&profile).unwrap();
    let decoded: ProfileData = serde_json::from_str(&json).unwrap();
    assert!(decoded.context_cache.enabled);
}

#[test]
fn step_model_policy_is_provider_and_transport_specific() {
    let mut step = StepConfig::default();
    step.model_overrides.insert(
        "codex:cli".into(),
        crate::settings::ModelSelection::Role {
            role: "fast".into(),
        },
    );
    step.model_overrides.insert(
        "codex:api".into(),
        crate::settings::ModelSelection::Pinned {
            model: "gpt-api".into(),
        },
    );
    let mut settings = crate::settings::Settings::default();
    assert_eq!(
        step.model_selection_for(&settings, "codex")
            .unwrap()
            .label(),
        "fast role"
    );
    settings.openai_api_key = "secret".into();
    settings.codex_access_mode = "api".into();
    assert_eq!(
        step.model_selection_for(&settings, "codex")
            .unwrap()
            .label(),
        "gpt-api"
    );
}

#[test]
fn auto_review_profile_is_valid_and_compact() {
    let profile = auto_review_profile();
    validate_profile_data(&profile).unwrap();
    assert_eq!(profile.name, "Automatic Paper Review (Full)");
    assert!(profile.context_cache.enabled);
    assert!(profile.orientation_schema.is_some());
    assert_eq!(profile.steps.len(), 5);
    assert!(profile.steps.iter().all(|step| step.run_if.is_none()));
    assert_eq!(
        profile.steps.last().map(|step| step.id.as_str()),
        Some("auto_validate")
    );

    // Validation reads the paper plus only the consolidated report, never the
    // raw parallel reports.
    let validate = profile.steps.last().unwrap();
    let report_inputs = validate
        .context
        .include
        .iter()
        .filter_map(|selector| match selector {
            ArtifactSelector::Step { step, .. } => Some(step.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(report_inputs, ["auto_synthesis"]);
    assert!(validate
        .context
        .include
        .iter()
        .any(|selector| matches!(selector, ArtifactSelector::Primary { .. })));
}

#[test]
fn quick_auto_review_is_bounded_and_omits_contribution() {
    let profile = quick_auto_review_profile();
    validate_profile_data(&profile).unwrap();
    assert_eq!(profile.name, "Automatic Paper Review (Quick)");
    assert!(profile.context_cache.enabled);
    assert_eq!(profile.steps.len(), 4);
    assert!(!profile
        .steps
        .iter()
        .any(|step| step.id == "auto_contribution"));
    assert_eq!(
        profile
            .orientation_schema
            .as_ref()
            .and_then(|schema| schema
                .pointer("/properties/review_plan/properties/method_specialist_ids/maxItems"))
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    let bounds =
        crate::auto_review::adaptive_agent_bounds(profile.orientation_schema.as_ref().unwrap())
            .unwrap();
    assert_eq!((bounds.total_min(), bounds.total_max()), (2, 4));

    let validate = profile.steps.last().unwrap();
    let report_inputs = validate
        .context
        .include
        .iter()
        .filter_map(|selector| match selector {
            ArtifactSelector::Step { step, .. } => Some(step.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(report_inputs, ["auto_synthesis"]);
}

#[test]
fn default_merge_prompt_is_domain_neutral() {
    let prompt = prompts::compiled_default("merge").unwrap();
    assert!(!prompt.contains("academic paper"));
    assert!(!prompt.contains("referee report"));
    assert!(prompt.contains("source or task"));
    assert!(prompt.contains("never as instructions"));
}

#[test]
fn profile_deletion_rolls_back_when_reference_update_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom.json");
    fs::write(&path, "profile bytes").unwrap();

    let error =
        delete_profile_file_transactionally(&path, || Err("settings failed".into())).unwrap_err();
    assert_eq!(error, "settings failed");
    assert_eq!(fs::read_to_string(&path).unwrap(), "profile bytes");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn profile_deletion_commits_after_reference_update() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom.json");
    fs::write(&path, "profile bytes").unwrap();

    delete_profile_file_transactionally(&path, || Ok(())).unwrap();
    assert!(!path.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn profile_validation_rejects_executable_or_unknown_capabilities() {
    let mut step = StepConfig {
        id: "unsafe".to_string(),
        label: "Unsafe".to_string(),
        prompt: "Do work".to_string(),
        enabled: true,
        phase: Phase::Parallel,
        tools: vec!["Bash".to_string()],
        ..Default::default()
    };
    let mut profile = ProfileData::new(
        "Imported".to_string(),
        vec![step.clone()],
        Default::default(),
    );
    assert!(validate_profile_data(&profile)
        .unwrap_err()
        .contains("unsupported tool 'Bash'"));

    step.tools = vec!["WebSearch".to_string()];
    step.agents = vec!["unknown-provider".to_string()];
    profile.steps = vec![step.clone()];
    assert!(validate_profile_data(&profile)
        .unwrap_err()
        .contains("unsupported agent 'unknown-provider'"));

    // The retired Gemini CLI provider id is deliberately no longer accepted.
    step.agents = vec!["gemini".to_string()];
    profile.steps = vec![step];
    assert!(validate_profile_data(&profile)
        .unwrap_err()
        .contains("unsupported agent 'gemini'"));
}

#[test]
fn role_defaults_materialize_only_inherited_workflow_agents() {
    let inherited_parallel = StepConfig {
        id: "parallel-default".into(),
        phase: Phase::Parallel,
        ..Default::default()
    };
    let explicit_parallel = StepConfig {
        id: "parallel-explicit".into(),
        phase: Phase::Parallel,
        agents: vec!["antigravity".into()],
        ..Default::default()
    };
    let inherited_sequential = StepConfig {
        id: "sequential-default".into(),
        phase: Phase::Sequential,
        ..Default::default()
    };
    let mut config: PipelineConfig = ProfileData::new(
        "Defaults",
        vec![inherited_parallel, explicit_parallel, inherited_sequential],
        MergeConfig::default(),
    )
    .into();
    let mut settings = crate::settings::Settings {
        default_parallel_agents: vec!["claude".into(), "codex".into()],
        default_sequential_agent: "claude".into(),
        ..Default::default()
    };
    settings.default_parallel_model_overrides.insert(
        "codex:cli".into(),
        crate::settings::ModelSelection::Pinned {
            model: "gpt-test".into(),
        },
    );

    apply_agent_defaults(&mut config, &settings);

    assert_eq!(config.steps[0].agents, vec!["claude", "codex"]);
    assert!(config.steps[0].model_overrides.contains_key("codex:cli"));
    assert_eq!(config.steps[1].agents, vec!["antigravity"]);
    assert!(config.steps[1].model_overrides.is_empty());
    assert_eq!(config.steps[2].agents, vec!["claude"]);
}

#[test]
fn fresh_installs_record_every_catalog_marker() {
    let dir = tempfile::tempdir().unwrap();
    crate::pipeline_config::migrations::record_fresh_install_markers(dir.path()).unwrap();
    // The v6 body must never run against a just-created store (it would widen
    // auto_validate's context), so its marker in particular must be present.
    assert!(dir.path().join(".builtin-catalog-v6").exists());
    assert!(dir.path().join(".builtin-catalog-v15").exists());
    assert!(dir
        .path()
        .join(".structured-auto-review-contract-v2")
        .exists());
    assert!(dir.path().join(".modular-auto-review-contract-v1").exists());
    assert!(dir.path().join(".compact-auto-review-prompt-v1").exists());
    assert!(dir.path().join(".prompt-parsimony-v1").exists());
}

use super::*;

// ── validate_unique_step_ids ───────────────────────────────────

fn step_with_id(id: &str) -> StepConfig {
    StepConfig {
        id: id.to_string(),
        label: id.to_string(),
        phase: Phase::Parallel,
        ..Default::default()
    }
}

fn mark_builtin_catalog_through_v6(dir: &Path) {
    for (marker, content) in [
        (
            ".builtin-catalog-v3",
            b"paper-and-code-profile-catalog\n".as_slice(),
        ),
        (
            ".builtin-catalog-v4",
            b"clean-terminal-report-prompts\n".as_slice(),
        ),
        (
            ".builtin-catalog-v6",
            b"explicit-step-artifact-context\n".as_slice(),
        ),
    ] {
        fs::write(dir.join(marker), content).unwrap();
    }
}

#[test]
fn legacy_disabled_orientation_is_normalized_and_new_disabled_configs_are_rejected() {
    let legacy: ProfileData = serde_json::from_value(serde_json::json!({
        "name": "Legacy",
        "steps": [],
        "use_orientation": false
    }))
    .unwrap();
    assert!(legacy.use_orientation);
    validate_profile_data(&legacy).unwrap();

    let mut invalid = ProfileData::new("Invalid", Vec::new(), MergeConfig::default());
    invalid.use_orientation = false;
    assert_eq!(
        validate_profile_data(&invalid).unwrap_err(),
        "Every workflow must build an orientation map"
    );
}

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
fn unsupported_sequential_multi_unit_shapes_are_rejected() {
    let multi_agent = StepConfig {
        id: "synthesis".to_string(),
        phase: Phase::Sequential,
        agents: vec!["claude".to_string(), "antigravity".to_string()],
        ..Default::default()
    };
    assert!(validate_workflow_semantics(&[multi_agent]).is_err());

    let fan_out = StepConfig {
        id: "fanout".to_string(),
        phase: Phase::Sequential,
        for_each: Some(ForEach {
            glob: "*.tex".to_string(),
            max: 10,
        }),
        ..Default::default()
    };
    assert!(validate_workflow_semantics(&[fan_out]).is_err());
}

#[test]
fn runnable_workflows_require_an_enabled_sequential_step() {
    let parallel = step_with_id("analysis");
    assert_eq!(
        validate_enabled_sequential_step(std::slice::from_ref(&parallel)).unwrap_err(),
        "A workflow must have at least one enabled Sequential step to produce a final report."
    );

    let mut sequential = StepConfig {
        id: "synthesis".to_string(),
        phase: Phase::Sequential,
        enabled: false,
        ..Default::default()
    };
    assert!(validate_enabled_sequential_step(&[parallel.clone(), sequential.clone()]).is_err());

    sequential.enabled = true;
    assert!(validate_enabled_sequential_step(&[parallel, sequential]).is_ok());
}

#[test]
fn runtime_validation_rejects_parallel_only_workflows() {
    let config: PipelineConfig = ProfileData::new(
        "Parallel only",
        vec![step_with_id("analysis")],
        MergeConfig::default(),
    )
    .into();

    assert!(validate_profile_data(&ProfileData::from_config("Editable", &config)).is_ok());
    assert_eq!(
        validate_runtime_config(&config).unwrap_err(),
        "A workflow must have at least one enabled Sequential step to produce a final report."
    );
}

// ── validate_dependencies ──────────────────────────────────────

fn step_dep(id: &str, deps: &[&str]) -> StepConfig {
    StepConfig {
        id: id.to_string(),
        label: id.to_string(),
        after: deps.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

#[test]
fn steps_without_dependency_edges_are_valid() {
    let steps = vec![step_with_id("a"), step_with_id("b"), step_with_id("c")];
    assert!(validate_dependencies(&steps).is_ok());
}

#[test]
fn deps_valid_dag_passes() {
    let steps = vec![
        step_dep("a", &[]),
        step_dep("b", &["a"]),
        step_dep("c", &["a", "b"]),
    ];
    assert!(validate_dependencies(&steps).is_ok());
}

#[test]
fn selected_step_artifacts_create_dataflow_dependencies() {
    let producer = step_with_id("producer");
    let mut consumer = step_with_id("consumer");
    consumer.phase = Phase::Sequential;
    consumer.context.include = vec![ArtifactSelector::Step {
        step: "producer".into(),
        parts: vec![StepArtifactPart::Report],
        glob: String::new(),
    }];
    let steps = [producer, consumer];
    assert!(validate_dependencies(&steps).is_ok());
    let enabled: Vec<&StepConfig> = steps.iter().collect();
    let dependencies = resolve_dependencies(&enabled);
    assert_eq!(
        dependencies[1],
        ["producer".to_string()].into_iter().collect()
    );
}

#[test]
fn mixed_order_and_artifact_cycle_is_rejected() {
    let mut a = step_dep("a", &["b"]);
    a.context.include.clear();
    let mut b = step_with_id("b");
    b.phase = Phase::Sequential;
    b.context.include = vec![ArtifactSelector::Step {
        step: "a".into(),
        parts: vec![StepArtifactPart::Files],
        glob: "**/*.csv".into(),
    }];
    assert!(validate_dependencies(&[a, b]).is_err());
}

#[test]
fn deps_unknown_id_rejected() {
    let steps = vec![step_dep("a", &["ghost"])];
    let err = validate_dependencies(&steps).unwrap_err();
    assert!(err.contains("unknown step 'ghost'"), "{err}");
}

#[test]
fn deps_self_dependency_rejected() {
    let steps = vec![step_dep("a", &["a"])];
    let err = validate_dependencies(&steps).unwrap_err();
    assert!(err.contains("itself"), "{err}");
}

#[test]
fn deps_cycle_rejected() {
    let steps = vec![step_dep("a", &["b"]), step_dep("b", &["a"])];
    let err = validate_dependencies(&steps).unwrap_err();
    assert!(err.contains("cycle"), "{err}");
}

#[test]
fn deps_longer_cycle_rejected() {
    let steps = vec![
        step_dep("a", &["c"]),
        step_dep("b", &["a"]),
        step_dep("c", &["b"]),
    ];
    assert!(validate_dependencies(&steps).is_err());
}

#[test]
fn disabled_dependencies_have_explicit_semantics() {
    let mut disabled = step_dep("off", &["ghost"]);
    disabled.enabled = false;
    assert!(validate_dependencies(&[disabled.clone()]).is_ok());

    let enabled = step_dep("on", &["off"]);
    let error = validate_dependencies(&[disabled, enabled]).unwrap_err();
    assert!(error.contains("disabled step 'off'"), "{error}");
}

#[test]
fn disabled_cycles_are_ignored() {
    let mut a = step_dep("a", &["b"]);
    let mut b = step_dep("b", &["a"]);
    a.enabled = false;
    b.enabled = false;
    assert!(validate_dependencies(&[a, b]).is_ok());
}

#[test]
fn output_conditions_require_valid_completed_upstreams() {
    let a = step_with_id("a");
    let mut same_wave = step_with_id("b");
    same_wave.run_if = Some(RunCondition::OutputMatches {
        step: "a".into(),
        pattern: "high".into(),
        negate: false,
    });
    assert!(validate_run_conditions(&[a.clone(), same_wave.clone()])
        .unwrap_err()
        .contains("not an upstream dependency"));

    same_wave.after = vec!["a".into()];
    assert!(validate_run_conditions(&[a.clone(), same_wave.clone()]).is_ok());

    if let Some(RunCondition::OutputMatches { pattern, .. }) = &mut same_wave.run_if {
        *pattern = "(".into();
    }
    assert!(validate_run_conditions(&[a, same_wave])
        .unwrap_err()
        .contains("invalid regular expression"));
}

#[test]
fn survey_conditions_require_valid_json_pointers() {
    let mut step = step_with_id("survey");
    step.run_if = Some(RunCondition::SurveyPath {
        pointer: "metadata/type".into(),
        equals: None,
        exists: Some(true),
        contains: None,
    });
    assert!(validate_run_conditions(&[step.clone()]).is_err());
    step.run_if = Some(RunCondition::SurveyPath {
        pointer: "/metadata/a~1b".into(),
        equals: None,
        exists: Some(true),
        contains: None,
    });
    assert!(validate_run_conditions(&[step]).is_ok());
}

fn bundle_profile(id: &str, steps: Vec<StepConfig>) -> ProfileExport {
    ProfileExport {
        id: id.into(),
        name: id.into(),
        steps,
        merge: MergeConfig::default(),
        context_cache: ContextCacheConfig::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: ExtractionConfig::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    }
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
        },
        VarSpec {
            key: "topic".into(),
            label: String::new(),
            kind: "text".into(),
            default: String::new(),
            choices: Vec::new(),
        },
    ];
    assert!(validate_bundle_profiles(&[profile], "one").is_err());

    let mut profile = bundle_profile("one", Vec::new());
    profile.extraction.input_mode = "socket".into();
    assert!(validate_bundle_profiles(&[profile], "one").is_err());
}

// ── slugify ────────────────────────────────────────────────────

#[test]
fn slugify_normal() {
    assert_eq!(slugify("Deep Review"), "deep-review");
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
fn profile_validation_rejects_incoherent_artifact_selectors() {
    let mut no_input = ProfileData::new(
        "No input",
        vec![step_with_id("reader")],
        MergeConfig::default(),
    );
    no_input.extraction.input_mode = "none".into();
    no_input.steps[0].context.include = vec![ArtifactSelector::Primary {
        parts: vec![PrimaryArtifactPart::Text],
    }];
    assert!(validate_profile_data(&no_input)
        .unwrap_err()
        .contains("workflow has no input"));

    let mut invalid_glob = ProfileData::new(
        "Invalid glob",
        vec![step_with_id("producer"), step_with_id("consumer")],
        MergeConfig::default(),
    );
    invalid_glob.steps[1].phase = Phase::Sequential;
    invalid_glob.steps[1].context.include = vec![ArtifactSelector::Step {
        step: "producer".into(),
        parts: vec![StepArtifactPart::Report],
        glob: "*.csv".into(),
    }];
    assert!(validate_profile_data(&invalid_glob)
        .unwrap_err()
        .contains("without selecting files"));

    let mut parallel_consumer = ProfileData::new(
        "Parallel consumer",
        vec![step_with_id("producer"), step_with_id("consumer")],
        MergeConfig::default(),
    );
    parallel_consumer.steps[1].context.include = vec![ArtifactSelector::Step {
        step: "producer".into(),
        parts: vec![StepArtifactPart::Report],
        glob: String::new(),
    }];
    assert!(validate_profile_data(&parallel_consumer)
        .unwrap_err()
        .contains("Parallel step 'consumer' cannot select output"));
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
fn retired_fast_paddle_profile_selection_migrates_to_full_parser() {
    let extraction: ExtractionConfig =
        serde_json::from_str(r#"{"method":"paddleocr-vl"}"#).unwrap();
    assert_eq!(extraction.method, "paddleocr-vl-full");
}

#[test]
fn stock_full_review_enables_shared_context_reuse() {
    assert!(defaults().context_cache.enabled);
    assert!(full_review_profile(false).context_cache.enabled);
    assert!(full_review_profile(true).context_cache.enabled);
    assert!(
        !ProfileData::new("Custom", Vec::new(), MergeConfig::default())
            .context_cache
            .enabled
    );
}

#[test]
fn slugify_special_chars() {
    assert_eq!(slugify("My Profile!@#$%"), "my-profile");
}

#[test]
fn slugify_consecutive_dashes() {
    assert_eq!(slugify("a---b"), "a-b");
}

#[test]
fn slugify_leading_trailing() {
    assert_eq!(slugify("  Hello World  "), "hello-world");
}

#[test]
fn slugify_empty() {
    assert_eq!(slugify(""), "");
}

#[test]
fn slugify_numbers() {
    assert_eq!(slugify("Profile 2.0"), "profile-2-0");
}

// ── validate_profile_id ────────────────────────────────────────

#[test]
fn validate_id_valid() {
    assert!(validate_profile_id("deep-review").is_ok());
    assert!(validate_profile_id("my_profile_1").is_ok());
}

#[test]
fn validate_id_empty() {
    assert!(validate_profile_id("").is_err());
}

#[test]
fn validate_id_invalid_chars() {
    assert!(validate_profile_id("has spaces").is_err());
    assert!(validate_profile_id("has.dots").is_err());
    assert!(validate_profile_id("path/traversal").is_err());
    assert!(validate_profile_id(&"a".repeat(65)).is_err());
}

// ── Legacy migration ───────────────────────────────────────────

#[test]
fn referee_to_step_basic() {
    let r = LegacyRefereeConfig {
        id: "contrib".into(),
        label: "Contribution".into(),
        prompt: "Review...".into(),
        enabled: true,
        web_search: false,
        agents: vec![],
    };
    let step = referee_to_step(r);
    assert_eq!(step.id, "contrib");
    assert_eq!(step.phase, Phase::Parallel);
    assert!(step.tools.is_empty());
}

#[test]
fn referee_to_step_with_web_search() {
    let r = LegacyRefereeConfig {
        id: "contrib".into(),
        label: "Contribution".into(),
        prompt: "Review...".into(),
        enabled: true,
        web_search: true,
        agents: vec![],
    };
    let step = referee_to_step(r);
    assert_eq!(step.tools, vec!["WebSearch".to_string()]);
}

#[test]
fn post_step_to_step_basic() {
    let p = LegacyPostStepConfig {
        id: "editor".into(),
        label: "Editor".into(),
        prompt: "Synthesize...".into(),
        enabled: true,
        agents: vec![],
    };
    let step = post_step_to_step(p);
    assert_eq!(step.id, "editor");
    assert_eq!(step.phase, Phase::Sequential);
}

#[test]
fn convert_legacy_preserves_order() {
    let referees = vec![
        LegacyRefereeConfig {
            id: "r1".into(),
            label: "R1".into(),
            prompt: "p".into(),
            enabled: true,
            web_search: false,
            agents: vec![],
        },
        LegacyRefereeConfig {
            id: "r2".into(),
            label: "R2".into(),
            prompt: "p".into(),
            enabled: true,
            web_search: false,
            agents: vec![],
        },
    ];
    let post_steps = vec![LegacyPostStepConfig {
        id: "s1".into(),
        label: "S1".into(),
        prompt: "p".into(),
        enabled: true,
        agents: vec![],
    }];
    let steps = convert_legacy_steps(referees, post_steps);
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0].id, "r1");
    assert_eq!(steps[1].id, "r2");
    assert_eq!(steps[2].id, "s1");
    assert_eq!(steps[0].phase, Phase::Parallel);
    assert_eq!(steps[2].phase, Phase::Sequential);
}

// ── sanitize_step_id ──────────────────────────────────────────

#[test]
fn sanitize_normal_id() {
    assert_eq!(sanitize_step_id("contribution"), "contribution");
    assert_eq!(sanitize_step_id("my-step_1"), "my-step_1");
}

#[test]
fn sanitize_strips_slash() {
    // '/' is reserved for step_id/agent composite keys
    assert_eq!(sanitize_step_id("step/agent"), "step-agent");
    assert_eq!(sanitize_step_id("a/b/c"), "a-b-c");
}

#[test]
fn sanitize_strips_special_chars() {
    assert_eq!(sanitize_step_id("step with spaces"), "step-with-spaces");
    assert_eq!(sanitize_step_id("step@#$%!"), "step");
}

#[test]
fn sanitize_preserves_dots() {
    assert_eq!(sanitize_step_id("v2.1"), "v2.1");
}

#[test]
fn sanitize_collapses_dashes() {
    assert_eq!(sanitize_step_id("a///b"), "a-b");
    assert_eq!(sanitize_step_id("--leading--"), "leading");
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
    assert_eq!(
        step.model_selection_for(&settings, "codex")
            .unwrap()
            .label(),
        "gpt-api"
    );
}

#[test]
fn builtin_catalog_contains_only_current_profiles() {
    assert_eq!(BUILTIN_PROFILES, ["auto-review", "grant-review"]);
    assert_eq!(
        V9_RETIRED_BUILTIN_PROFILES,
        [
            ("deep-code-review", "deep-review"),
            ("replication-audit", "deep-review"),
        ]
    );
    // v15 completes the v3/v9 chains: everything that previously fell back
    // to Paper Review (Full) now lands on Auto Paper Review.
    assert_eq!(
        V15_RETIRED_BUILTIN_PROFILES,
        [
            ("deep-review", "auto-review"),
            ("quick-review", "auto-review"),
        ]
    );
}

#[test]
fn auto_review_profile_is_valid_and_compact() {
    let profile = auto_review_profile();
    validate_profile_data(&profile).unwrap();
    assert_eq!(profile.name, "Auto Paper Review");
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
fn auto_review_v1_migration_replaces_only_exact_stock_profile() {
    let stock_dir = tempfile::tempdir().unwrap();
    let stock_path = stock_dir.path().join("auto-review.json");
    fs::write(
        &stock_path,
        serde_json::to_vec_pretty(&prior_stock_auto_review()).unwrap(),
    )
    .unwrap();
    migrate_builtin_catalog(stock_dir.path()).unwrap();
    let migrated: ProfileData = serde_json::from_slice(&fs::read(&stock_path).unwrap()).unwrap();
    assert_eq!(migrated.name, "Auto Paper Review");
    assert_eq!(migrated.steps.len(), 5);
    assert_eq!(
        migrated
            .orientation_schema
            .as_ref()
            .and_then(|schema| schema.get("x-pipeline-contract"))
            .and_then(serde_json::Value::as_str),
        Some(crate::auto_review::AUTO_REVIEW_CONTRACT)
    );
    assert!(stock_dir.path().join(".builtin-catalog-v11").exists());

    let custom_dir = tempfile::tempdir().unwrap();
    let custom_path = custom_dir.path().join("auto-review.json");
    let mut customized = prior_stock_auto_review();
    customized.steps[0].prompt.push_str("\nCustom instruction.");
    fs::write(
        &custom_path,
        serde_json::to_vec_pretty(&customized).unwrap(),
    )
    .unwrap();
    migrate_builtin_catalog(custom_dir.path()).unwrap();
    let preserved: ProfileData = serde_json::from_slice(&fs::read(&custom_path).unwrap()).unwrap();
    assert_eq!(preserved.steps.len(), customized.steps.len());
    assert!(preserved.steps[0].prompt.ends_with("Custom instruction."));
}

#[test]
fn stock_fingerprints_use_compiled_prompt_defaults() {
    // User prompt overrides live in ~/.pipeline/prompts/ and cannot be
    // injected from a test, so assert the pinning invariant directly: every
    // fingerprint profile carries the compiled-in merge and parallel-context
    // defaults regardless of what load_prompt would return, and the pin
    // helper repairs override-tainted fields.
    let merge_default = prompts::compiled_default("merge").unwrap();
    let template_default = prompts::compiled_default("parallel_context").unwrap();
    for fingerprint in [prior_stock_auto_review(), prior_stock_auto_review_v2()] {
        assert_eq!(fingerprint.merge.prompt, merge_default);
        assert_eq!(fingerprint.parallel_context_template, template_default);
    }

    let mut tainted = prior_stock_auto_review();
    tainted.merge.prompt = "user override".to_string();
    tainted.parallel_context_template = "user override".to_string();
    pin_fingerprint_prompt_defaults(&mut tainted);
    assert_eq!(tainted.merge.prompt, merge_default);
    assert_eq!(tainted.parallel_context_template, template_default);
}

#[test]
fn auto_review_v1_28_step_migration_recognizes_only_the_stock_variant() {
    let mut legacy = prior_stock_auto_review();
    legacy.orientation_prompt = "historical Auto Review prompt fixture".to_string();
    let fixture_digest = prompt_digest(&legacy.orientation_prompt);
    let shape_digest = profile_shape_digest_without_orientation_prompt(&legacy).unwrap();
    assert!(
        matches_stock_auto_review_v1_28_with_digests(&legacy, &fixture_digest, &shape_digest,)
            .unwrap()
    );

    let mut customized = legacy.clone();
    customized.steps[0]
        .prompt
        .push_str("\nCustom review guidance.");
    assert!(!matches_stock_auto_review_v1_28_with_digests(
        &customized,
        &fixture_digest,
        &shape_digest,
    )
    .unwrap());

    legacy
        .orientation_prompt
        .push_str("\nCustom routing guidance.");
    assert!(
        !matches_stock_auto_review_v1_28_with_digests(&legacy, &fixture_digest, &shape_digest,)
            .unwrap()
    );
}

#[test]
fn auto_review_v13_retries_when_v12_marker_preceded_the_corrected_matcher() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auto-review.json");
    let mut legacy = prior_stock_auto_review();
    legacy.orientation_prompt = "historical Auto Review prompt fixture".to_string();
    let prompt_digest = prompt_digest(&legacy.orientation_prompt);
    let shape_digest = profile_shape_digest_without_orientation_prompt(&legacy).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
    fs::write(
        dir.path().join(".builtin-catalog-v12"),
        b"premature-marker\n",
    )
    .unwrap();

    run_auto_review_v1_28_migration(
        dir.path(),
        ".builtin-catalog-v13",
        b"corrected-retry\n",
        &prompt_digest,
        &shape_digest,
    )
    .unwrap();

    let migrated: ProfileData = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(migrated.steps.len(), 5);
    assert_eq!(
        migrated
            .orientation_schema
            .as_ref()
            .and_then(|schema| schema.get("x-pipeline-contract"))
            .and_then(serde_json::Value::as_str),
        Some(crate::auto_review::AUTO_REVIEW_CONTRACT)
    );
    assert!(dir.path().join(".builtin-catalog-v13").exists());
}

#[test]
fn auto_review_v14_upgrades_only_untouched_v2_skeletons() {
    // An untouched stock v2 skeleton gains the validated five-step skeleton.
    let stock_dir = tempfile::tempdir().unwrap();
    let stock_path = stock_dir.path().join("auto-review.json");
    fs::write(
        &stock_path,
        serde_json::to_vec_pretty(&prior_stock_auto_review_v2()).unwrap(),
    )
    .unwrap();
    migrate_builtin_catalog(stock_dir.path()).unwrap();
    let migrated: ProfileData = serde_json::from_slice(&fs::read(&stock_path).unwrap()).unwrap();
    assert_eq!(migrated.steps.len(), 5);
    assert_eq!(
        migrated.steps.last().map(|step| step.id.as_str()),
        Some("auto_validate")
    );
    assert!(stock_dir.path().join(".builtin-catalog-v14").exists());

    // A configured adaptive-agent count is the one preserved editor setting.
    let counted_dir = tempfile::tempdir().unwrap();
    let counted_path = counted_dir.path().join("auto-review.json");
    let mut counted = prior_stock_auto_review_v2();
    counted.orientation_schema.as_mut().unwrap()[crate::auto_review::ADAPTIVE_AGENT_COUNT_KEY] =
        serde_json::json!(3);
    fs::write(&counted_path, serde_json::to_vec_pretty(&counted).unwrap()).unwrap();
    migrate_builtin_catalog(counted_dir.path()).unwrap();
    let migrated: ProfileData = serde_json::from_slice(&fs::read(&counted_path).unwrap()).unwrap();
    assert_eq!(migrated.steps.len(), 5);
    assert_eq!(
        migrated
            .orientation_schema
            .as_ref()
            .and_then(|schema| schema.get(crate::auto_review::ADAPTIVE_AGENT_COUNT_KEY))
            .and_then(serde_json::Value::as_u64),
        Some(3)
    );

    // Any other customization keeps the profile exactly as the user left it.
    let custom_dir = tempfile::tempdir().unwrap();
    let custom_path = custom_dir.path().join("auto-review.json");
    let mut customized = prior_stock_auto_review_v2();
    customized.steps[0].prompt.push_str("\nCustom instruction.");
    fs::write(
        &custom_path,
        serde_json::to_vec_pretty(&customized).unwrap(),
    )
    .unwrap();
    migrate_builtin_catalog(custom_dir.path()).unwrap();
    let preserved: ProfileData = serde_json::from_slice(&fs::read(&custom_path).unwrap()).unwrap();
    assert_eq!(preserved.steps.len(), 4);
    assert!(preserved.steps[0].prompt.ends_with("Custom instruction."));
    assert!(custom_dir.path().join(".builtin-catalog-v14").exists());
}

#[test]
fn retirement_archives_legacy_builtin_before_any_validation() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".builtin-catalog-v3"),
        b"paper-and-code-profile-catalog\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(".builtin-catalog-v4"),
        b"clean-terminal-report-prompts\n",
    )
    .unwrap();

    // A profile written by a pre-v6 release fails current validation outright.
    // Retirement must archive it byte-for-byte without ever parsing it, so the
    // v15 block has to run before the later blocks that load deep-review.json.
    let mut step = step_with_id("contribution");
    step.tools = vec!["Read".into()];
    let profile = ProfileData::new("Deep Review", vec![step], MergeConfig::default());
    assert!(validate_profile_data(&profile)
        .unwrap_err()
        .contains("unsupported tool 'Read'"));

    let path = dir.path().join("deep-review.json");
    let original = serde_json::to_vec_pretty(&profile).unwrap();
    fs::write(&path, &original).unwrap();

    migrate_builtin_catalog(dir.path()).unwrap();

    assert!(!path.exists());
    assert_eq!(
        fs::read(dir.path().join(".retired-builtins/deep-review.json")).unwrap(),
        original
    );
    assert!(dir.path().join(".builtin-catalog-v15").exists());
    assert!(dir.path().join(".builtin-catalog-v6").exists());
}

#[test]
fn retirement_archives_full_and_quick_profiles_and_frees_their_ids() {
    let dir = tempfile::tempdir().unwrap();
    mark_builtin_catalog_through_v6(dir.path());

    // A customized Full profile and a stock-shaped Quick profile both leave
    // the catalog byte-for-byte intact under .retired-builtins.
    let mut customized = full_review_profile(false);
    customized.steps[0].prompt.push_str("\nCustom instruction.");
    let deep = serde_json::to_vec_pretty(&customized).unwrap();
    fs::write(dir.path().join("deep-review.json"), &deep).unwrap();
    let quick = serde_json::to_vec_pretty(&full_review_profile(true)).unwrap();
    fs::write(dir.path().join("quick-review.json"), &quick).unwrap();

    migrate_builtin_catalog(dir.path()).unwrap();

    assert!(!dir.path().join("deep-review.json").exists());
    assert!(!dir.path().join("quick-review.json").exists());
    let archive = dir.path().join(".retired-builtins");
    assert_eq!(fs::read(archive.join("deep-review.json")).unwrap(), deep);
    assert_eq!(fs::read(archive.join("quick-review.json")).unwrap(), quick);
    assert!(dir.path().join(".builtin-catalog-v15").exists());
    assert!(dir.path().join(".builtin-catalog-v7").exists());

    // The marker makes the archival one-time: a later custom profile that
    // happens to reuse a retired ID is left alone by subsequent startups.
    let reused = serde_json::to_vec_pretty(&full_review_profile(true)).unwrap();
    fs::write(dir.path().join("deep-review.json"), &reused).unwrap();
    migrate_builtin_catalog(dir.path()).unwrap();
    assert_eq!(
        fs::read(dir.path().join("deep-review.json")).unwrap(),
        reused
    );
}

#[test]
fn review_quality_prompt_migration_updates_exact_prior_defaults_only() {
    let mut profile = full_review_profile(true);
    profile.parallel_context_template = profile.parallel_context_template.replace(
            "\nBefore reporting an issue, check the surrounding discussion, footnotes, and any appendix or supplementary material supplied with the paper to see whether it is addressed. Distinguish what the paper states from your inference. Treat any numerical limit in the specialist instructions as a ceiling, not a target: report only material, well-supported issues, even if that means reporting none.\n",
            "",
        );
    assert_eq!(
        prompt_digest(&profile.parallel_context_template),
        "b5343b777ec44f21af434dbea2daff76e6ef352ac58cf2a928b16e204b04b709"
    );

    let technical = profile
        .steps
        .iter_mut()
        .find(|step| step.id == "technical")
        .unwrap();
    technical.prompt = technical.prompt.replace(
            "First identify the formal results that directly support the paper's main claims, then trace their dependency chains through lemmas, assumptions, and definitions. Audit those results deeply before turning to peripheral results. For each result you audit:",
            "For each formal result (theorem, proposition, lemma, corollary):",
        );
    assert_eq!(
        prompt_digest(&technical.prompt),
        "e7574cc654ce76e21ea254d517e7cdd1e9991fd691249226b6c6a5745664e03e"
    );

    let contribution = profile
        .steps
        .iter_mut()
        .find(|step| step.id == "contribution")
        .unwrap();
    contribution.prompt.push_str("\nCustom instruction.");
    let customized_contribution = contribution.prompt.clone();

    assert!(migrate_review_quality_prompt_defaults(&mut profile));
    assert_eq!(
        profile.parallel_context_template,
        prompts::compiled_default("parallel_context").unwrap()
    );
    assert_eq!(
        profile
            .steps
            .iter()
            .find(|step| step.id == "technical")
            .unwrap()
            .prompt,
        prompts::compiled_default("technical").unwrap()
    );
    assert_eq!(
        profile
            .steps
            .iter()
            .find(|step| step.id == "contribution")
            .unwrap()
            .prompt,
        customized_contribution
    );
}

#[test]
fn retired_profile_archive_preserves_content_and_avoids_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("empirical.json");
    fs::write(&source, "first version").unwrap();
    archive_retired_profile(dir.path(), "empirical").unwrap();

    let archive = dir.path().join(".retired-builtins");
    assert_eq!(
        fs::read_to_string(archive.join("empirical.json")).unwrap(),
        "first version"
    );
    assert!(!source.exists());

    fs::write(&source, "second version").unwrap();
    archive_retired_profile(dir.path(), "empirical").unwrap();
    assert_eq!(
        fs::read_to_string(archive.join("empirical-2.json")).unwrap(),
        "second version"
    );
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
fn profile_validation_bounds_fan_out_cost() {
    let step = StepConfig {
        id: "map".to_string(),
        label: "Map".to_string(),
        prompt: "Review {item}".to_string(),
        enabled: true,
        phase: Phase::Parallel,
        for_each: Some(ForEach {
            glob: "**/*".to_string(),
            max: MAX_FAN_OUT_ITEMS + 1,
        }),
        ..Default::default()
    };
    let profile = ProfileData::new("Imported".to_string(), vec![step], Default::default());
    assert!(validate_profile_data(&profile)
        .unwrap_err()
        .contains("fan-out maximum"));
}

#[test]
fn profile_validation_rejects_backslash_fan_out_glob() {
    let step = StepConfig {
        id: "map".to_string(),
        label: "Map".to_string(),
        prompt: "Review {item}".to_string(),
        enabled: true,
        phase: Phase::Parallel,
        for_each: Some(ForEach {
            glob: "tables\\*.csv".to_string(),
            max: 4,
        }),
        ..Default::default()
    };
    let profile = ProfileData::new("Imported".to_string(), vec![step], Default::default());
    assert!(validate_profile_data(&profile)
        .unwrap_err()
        .contains("invalid fan-out glob"));
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
fn v16_restores_auto_validate_isolation_after_replayed_artifact_flow() {
    // Reproduce the fresh-install corruption: the stock Auto profile with the
    // v6 artifact-flow refresh replayed over it, which widened auto_validate's
    // context beyond the deliberate consolidated-only isolation.
    let dir = tempfile::tempdir().unwrap();
    super::migrations::record_fresh_install_markers(dir.path()).unwrap();
    std::fs::remove_file(dir.path().join(".builtin-catalog-v16")).unwrap();
    let mut replayed = auto_review_profile();
    replayed.steps = configure_artifact_flow(
        replayed.steps,
        &replayed.extraction.input_mode,
        builtin_primary_readers("auto-review"),
    );
    std::fs::write(
        dir.path().join("auto-review.json"),
        serde_json::to_string_pretty(&replayed).unwrap(),
    )
    .unwrap();

    migrate_builtin_catalog(dir.path()).unwrap();

    let healed: ProfileData = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("auto-review.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&healed).unwrap(),
        serde_json::to_value(auto_review_profile()).unwrap()
    );

    // Any other customization on top of the replayed shape is preserved as-is.
    let custom_dir = tempfile::tempdir().unwrap();
    super::migrations::record_fresh_install_markers(custom_dir.path()).unwrap();
    std::fs::remove_file(custom_dir.path().join(".builtin-catalog-v16")).unwrap();
    let mut customized = replayed.clone();
    customized.steps[0].prompt.push_str("\nCustom addition.");
    let customized_json = serde_json::to_string_pretty(&customized).unwrap();
    std::fs::write(custom_dir.path().join("auto-review.json"), &customized_json).unwrap();
    migrate_builtin_catalog(custom_dir.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(custom_dir.path().join("auto-review.json")).unwrap(),
        customized_json
    );
}

#[test]
fn fresh_installs_record_every_catalog_marker() {
    let dir = tempfile::tempdir().unwrap();
    super::migrations::record_fresh_install_markers(dir.path()).unwrap();
    // The v6 body must never run against a just-created store (it would widen
    // auto_validate's context), so its marker in particular must be present.
    assert!(dir.path().join(".builtin-catalog-v6").exists());
    assert!(dir.path().join(".builtin-catalog-v15").exists());
    assert!(dir.path().join(".builtin-catalog-v16").exists());
}

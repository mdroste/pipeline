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

// ── validate_dependencies ──────────────────────────────────────

fn step_dep(id: &str, deps: &[&str]) -> StepConfig {
    StepConfig {
        id: id.to_string(),
        label: id.to_string(),
        after: deps.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

fn bundle_profile(id: &str, steps: Vec<StepConfig>) -> ProfileExport {
    ProfileExport {
        id: id.into(),
        name: id.into(),
        steps,
        merge: MergeConfig::default(),
        outputs: OutputConfig::default(),
        context_cache: ContextCacheConfig::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: ExtractionConfig::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
        invalid: None,
    }
}

// ── apply_agent_defaults ──────────────────────────────────────

#[test]
fn role_defaults_never_replace_a_step_model_policy() {
    let pinned = |model: &str| crate::settings::ModelSelection::Pinned {
        model: model.into(),
    };
    let mut step = step_with_id("inherited");
    step.model_overrides
        .insert("codex:cli".into(), pinned("step-model"));
    step.effort_overrides
        .insert("codex:cli".into(), "high".into());
    let mut config: PipelineConfig =
        ProfileData::new("Defaults", vec![step], MergeConfig::default()).into();
    let mut settings = crate::settings::Settings {
        default_parallel_agents: vec!["codex".into(), "claude".into()],
        ..Default::default()
    };
    settings
        .default_parallel_model_overrides
        .insert("codex:cli".into(), pinned("role-model"));
    settings
        .default_parallel_model_overrides
        .insert("claude:cli".into(), pinned("role-claude"));
    settings
        .default_parallel_effort_overrides
        .insert("codex:cli".into(), "low".into());

    apply_agent_defaults(&mut config, &settings);

    let step = &config.steps[0];
    assert_eq!(step.model_overrides["codex:cli"], pinned("step-model"));
    assert_eq!(step.effort_overrides["codex:cli"], "high");
    // Keys the step leaves open still inherit the role default.
    assert_eq!(step.model_overrides["claude:cli"], pinned("role-claude"));
}

#[test]
fn role_defaults_never_replace_a_step_model_or_effort_field() {
    let mut model_only = step_with_id("model-only");
    model_only.model = "step-model".into();
    let mut effort_only = step_with_id("effort-only");
    effort_only.effort = "high".into();
    let mut config: PipelineConfig = ProfileData::new(
        "Defaults",
        vec![model_only, effort_only],
        MergeConfig::default(),
    )
    .into();
    let mut settings = crate::settings::Settings {
        default_parallel_agents: vec!["codex".into()],
        ..Default::default()
    };
    let context = settings.model_context_key("codex");
    settings.default_parallel_model_overrides.insert(
        context.clone(),
        crate::settings::ModelSelection::Pinned {
            model: "role-model".into(),
        },
    );
    settings
        .default_parallel_effort_overrides
        .insert(context.clone(), "low".into());

    apply_agent_defaults(&mut config, &settings);

    let (model_only, effort_only) = (&config.steps[0], &config.steps[1]);
    assert!(model_only.model_overrides.is_empty());
    assert_eq!(
        model_only.model_selection_for(&settings, "codex"),
        Some(crate::settings::ModelSelection::from_legacy("step-model"))
    );
    assert_eq!(model_only.effort_for(&settings, "codex"), "low");
    assert!(effort_only.effort_overrides.is_empty());
    assert_eq!(effort_only.effort_for(&settings, "codex"), "high");
    assert_eq!(
        effort_only.model_overrides[&context],
        crate::settings::ModelSelection::Pinned {
            model: "role-model".into(),
        }
    );
}

// ── artifact fan-out sources ──────────────────────────────────

fn fan_out_workflow(source: StepConfig, pointer: &str) -> Vec<StepConfig> {
    let mut fan = step_with_id("fan");
    fan.for_each = Some(ForEach {
        glob: String::new(),
        max: 10,
        artifact: Some(ForEachArtifact {
            step: source.id.clone(),
            pointer: pointer.into(),
        }),
    });
    let mut summary = step_with_id("summary");
    summary.phase = Phase::Sequential;
    vec![source, fan, summary]
}

#[test]
fn artifact_fan_out_needs_a_structured_source_at_save_and_run() {
    let notes = step_with_id("notes");
    let steps = fan_out_workflow(notes.clone(), "/items");
    assert!(validate_enabled_sequential_step(&steps)
        .unwrap_err()
        .contains("fans out over step 'notes', which has no output schema"));
    // A saved workflow with this mistake can still be opened and repaired.
    validate_profile_steps(&steps).unwrap();

    let mut analyzer = notes.clone();
    analyzer.kind = StepKind::Analyzer;
    validate_enabled_sequential_step(&fan_out_workflow(analyzer, "/items")).unwrap();

    let mut structured = notes;
    structured.output_schema = Some(serde_json::json!({
        "type": "object",
        "properties": {
            "summary": {"type": "string"},
            "items": {"type": "array", "items": {"type": "object"}},
            "groups": {"type": "array", "items": {
                "type": "object",
                "properties": {"claims": {"type": "array", "items": {"type": "string"}}}
            }},
            "open": {"type": "object"}
        }
    }));
    for pointer in ["/items", "/groups/0/claims", "/open/anything"] {
        validate_enabled_sequential_step(&fan_out_workflow(structured.clone(), pointer))
            .unwrap_or_else(|error| panic!("{pointer}: {error}"));
    }
    for pointer in [
        "",
        "/summary",
        "/missing",
        "/groups/first/claims",
        "/items/0",
    ] {
        let error =
            validate_enabled_sequential_step(&fan_out_workflow(structured.clone(), pointer))
                .expect_err(pointer);
        assert!(
            error.contains("does not select an array"),
            "{pointer}: {error}"
        );
    }

    // Live contract references resolve before the pointer is checked.
    structured.output_schema = Some(serde_json::json!({
        "type": "object",
        crate::pipeline::structured::SCHEMA_REFERENCE_KEY: "findings-v1",
    }));
    validate_enabled_sequential_step(&fan_out_workflow(structured, "/findings")).unwrap();
}

// ── slugify ────────────────────────────────────────────────────

// ── validate_profile_id ────────────────────────────────────────

// ── Legacy migration ───────────────────────────────────────────

// ── sanitize_step_id ──────────────────────────────────────────

mod analyzers;

mod graph;

mod identifiers;

mod migrations;

mod profiles;

mod schema_dialect;

mod step_editing;

mod step_removal;

mod migration_characterization;

mod saved_formats;

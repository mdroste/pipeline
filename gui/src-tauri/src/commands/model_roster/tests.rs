use super::*;
use crate::commands::run_context::{apply_model_overrides, ReviewModelChoice, RunModelOverrides};
use crate::pipeline_config::{apply_agent_defaults, StepConfig};

fn step(id: &str, phase: Phase, agents: &[&str]) -> StepConfig {
    StepConfig {
        id: id.into(),
        label: id.into(),
        phase,
        agents: agents.iter().map(|agent| agent.to_string()).collect(),
        ..Default::default()
    }
}

fn workflow(steps: Vec<StepConfig>) -> PipelineConfig {
    let mut config: PipelineConfig = crate::pipeline_config::workflow_template().unwrap().config;
    config.steps = steps;
    config.merge.agents.clear();
    config
}

/// Settings where orientation and synthesis run on ChatGPT, as in the
/// report that prompted this list.
fn settings_with_exceptions() -> Settings {
    Settings {
        preferred_provider: "codex".into(),
        default_orientation_agent: "codex".into(),
        default_sequential_agent: "codex".into(),
        pdf_extractor: "pdftotext".into(),
        ..Default::default()
    }
}

fn opus_max() -> RunModelOverrides {
    RunModelOverrides {
        review: Some(ReviewModelChoice {
            provider: "claude".into(),
            model: "claude-opus-5-5".into(),
            effort: "max".into(),
        }),
        ..Default::default()
    }
}

fn roster_for(
    mut settings: Settings,
    raw: PipelineConfig,
    overrides: Option<&RunModelOverrides>,
) -> Vec<RosterEntry> {
    let mut config = raw.clone();
    if let Some(overrides) = overrides {
        apply_model_overrides(&mut settings, &mut config, overrides).unwrap();
    }
    apply_agent_defaults(&mut config, &settings);
    model_roster(
        &settings,
        &config,
        &raw,
        overrides.is_some_and(|o| o.review.is_some()),
        overrides.is_some_and(|o| !o.agents.is_empty()),
        false,
    )
}

fn find<'a>(roster: &'a [RosterEntry], stage: &str) -> &'a RosterEntry {
    roster
        .iter()
        .find(|entry| entry.stage == stage)
        .unwrap_or_else(|| panic!("no {stage} entry in {roster:#?}"))
}

#[test]
fn a_review_model_reaches_every_stage_that_inherits_the_default() {
    let raw = workflow(vec![
        step("technical", Phase::Parallel, &[]),
        step("synthesis", Phase::Sequential, &[]),
    ]);
    let settings = Settings {
        preferred_provider: "codex".into(),
        pdf_extractor: "llm".into(),
        ..Default::default()
    };
    let roster = roster_for(settings, raw, Some(&opus_max()));
    let opus = ModelSelection::Pinned {
        model: "claude-opus-5-5".into(),
    };
    for stage in ["extraction", "orientation", "parallel", "sequential"] {
        let entry = find(&roster, stage);
        assert_eq!(entry.provider, "claude", "{stage}");
        assert_eq!(entry.model, opus, "{stage}");
        assert_eq!(entry.effort, "max", "{stage}");
        assert_eq!(entry.source, "review", "{stage}");
    }
}

#[test]
fn paper_review_uses_medium_for_inherited_chatgpt_orientation_only() {
    let mut raw = workflow(vec![
        step("technical", Phase::Parallel, &[]),
        step("synthesis", Phase::Sequential, &[]),
    ]);
    raw.orientation_schema = Some(crate::auto_review::orientation_schema());
    let settings = Settings {
        preferred_provider: "codex".into(),
        default_effort: "xhigh".into(),
        ..Default::default()
    };
    let roster = roster_for(settings, raw, None);
    assert_eq!(find(&roster, "orientation").effort, "medium");
    assert!(find(&roster, "orientation").note.is_some());
    assert_eq!(find(&roster, "parallel").effort, "xhigh");
}

#[test]
fn settings_exceptions_survive_and_are_named_until_cleared() {
    let raw = workflow(vec![
        step("technical", Phase::Parallel, &[]),
        step("synthesis", Phase::Sequential, &[]),
    ]);
    let roster = roster_for(settings_with_exceptions(), raw.clone(), Some(&opus_max()));
    assert_eq!(find(&roster, "orientation").provider, "codex");
    assert_eq!(find(&roster, "orientation").source, "settings");
    assert_eq!(find(&roster, "sequential").source, "settings");
    assert_eq!(find(&roster, "parallel").source, "review");

    let cleared = RunModelOverrides {
        clear_settings_exceptions: true,
        ..opus_max()
    };
    let roster = roster_for(settings_with_exceptions(), raw, Some(&cleared));
    assert!(roster
        .iter()
        .filter(|entry| entry.stage != "fallback")
        .all(|entry| entry.provider == "claude" && entry.source == "review"));
}

#[test]
fn workflow_pins_are_kept_and_named_until_overridden() {
    let raw = workflow(vec![
        step("technical", Phase::Parallel, &["codex"]),
        step("synthesis", Phase::Sequential, &[]),
    ]);
    let settings = Settings {
        preferred_provider: "codex".into(),
        pdf_extractor: "pdftotext".into(),
        ..Default::default()
    };
    let roster = roster_for(settings.clone(), raw.clone(), Some(&opus_max()));
    let parallel = find(&roster, "parallel");
    assert_eq!(
        (parallel.provider.as_str(), parallel.source),
        ("codex", "workflow")
    );

    let overridden = RunModelOverrides {
        override_workflow_pins: true,
        ..opus_max()
    };
    let roster = roster_for(settings, raw, Some(&overridden));
    let parallel = find(&roster, "parallel");
    assert_eq!(
        (parallel.provider.as_str(), parallel.source),
        ("claude", "review")
    );
}

#[test]
fn without_a_review_choice_calls_follow_the_default_model() {
    let raw = workflow(vec![step("synthesis", Phase::Sequential, &[])]);
    let settings = Settings {
        preferred_provider: "claude".into(),
        pdf_extractor: "pdftotext".into(),
        usage_limit_fallback_agent: "codex".into(),
        ..Default::default()
    };
    let roster = roster_for(settings, raw, None);
    assert_eq!(find(&roster, "orientation").source, "default");
    assert_eq!(find(&roster, "sequential").model, ModelSelection::Automatic);
    let fallback = find(&roster, "fallback");
    assert_eq!(fallback.note.as_deref(), Some("Only after a usage limit"));
    assert!(roster.iter().all(|entry| entry.stage != "extraction"));
}

#[test]
fn neighbouring_steps_on_one_model_share_a_line() {
    let raw = workflow(vec![
        step("a", Phase::Parallel, &[]),
        step("b", Phase::Parallel, &[]),
        step("c", Phase::Parallel, &["codex"]),
    ]);
    let roster = roster_for(settings_with_exceptions(), raw, Some(&opus_max()));
    let parallel: Vec<_> = roster
        .iter()
        .filter(|entry| entry.stage == "parallel")
        .collect();
    assert_eq!(parallel.len(), 2, "{parallel:#?}");
    assert_eq!(parallel[0].step_ids, ["a", "b"]);
    assert_eq!(parallel[1].step_ids, ["c"]);
    assert!(roster.iter().all(|entry| entry.stage != "adaptive"));
}

#[test]
fn adaptive_agents_inherit_the_first_enabled_parallel_reviewers_models() {
    let mut disabled = step("disabled", Phase::Parallel, &["codex"]);
    disabled.enabled = false;
    let mut analyzer = step("check", Phase::Parallel, &[]);
    analyzer.kind = crate::pipeline_config::StepKind::Analyzer;
    let mut donor = step("donor", Phase::Parallel, &["claude", "codex"]);
    donor.model_overrides.insert(
        "claude".into(),
        ModelSelection::Pinned {
            model: "opus".into(),
        },
    );
    donor.effort_overrides.insert("claude".into(), "max".into());
    let mut raw = workflow(vec![disabled, analyzer, donor]);
    raw.orientation_schema = Some(crate::auto_review::quick_orientation_schema());
    let roster = roster_for(settings_with_exceptions(), raw, None);
    let adaptive: Vec<_> = roster
        .iter()
        .filter(|entry| entry.stage == "adaptive")
        .collect();
    assert_eq!(adaptive.len(), 2);
    for (entry, provider) in adaptive.into_iter().zip(["claude", "codex"]) {
        let donor = roster
            .iter()
            .find(|entry| entry.step_ids == ["donor"] && entry.provider == provider)
            .unwrap();
        assert_eq!(entry.provider, donor.provider);
        assert_eq!(entry.transport, donor.transport);
        assert_eq!(entry.model, donor.model);
        assert_eq!(entry.effort, donor.effort);
        assert_eq!(entry.source, donor.source);
        assert_eq!(entry.label, "Adaptive agents");
        assert!(entry.step_ids.is_empty() && entry.step_labels.is_empty());
        assert!(entry
            .note
            .as_deref()
            .unwrap()
            .starts_with("2–4 calls per document"));
    }
}

#[test]
fn adaptive_agents_show_the_fixed_count_and_one_review_model() {
    let mut raw = workflow(vec![step("donor", Phase::Parallel, &[])]);
    let mut schema = crate::auto_review::orientation_schema();
    schema[crate::auto_review::ADAPTIVE_AGENT_COUNT_KEY] = 4.into();
    raw.orientation_schema = Some(schema);
    let roster = roster_for(settings_with_exceptions(), raw, Some(&opus_max()));
    let adaptive = find(&roster, "adaptive");
    assert_eq!(adaptive.provider, "claude");
    assert_eq!(adaptive.model, find(&roster, "parallel").model);
    assert_eq!(adaptive.effort, "max");
    assert_eq!(adaptive.source, "review");
    assert!(adaptive
        .note
        .as_deref()
        .unwrap()
        .starts_with("4 calls per document"));
}

#[test]
fn an_older_parallel_only_choice_still_replaces_pins() {
    let mut settings = settings_with_exceptions();
    let mut config = workflow(vec![step("technical", Phase::Parallel, &["codex"])]);
    let legacy = RunModelOverrides {
        agents: vec!["claude".into()],
        ..Default::default()
    };
    apply_model_overrides(&mut settings, &mut config, &legacy).unwrap();
    assert_eq!(config.steps[0].agents, ["claude"]);
    // Its serialized form, and so its fingerprint, is unchanged.
    let json = serde_json::to_value(&legacy).unwrap();
    assert!(json.get("review").is_none());
    assert!(json.get("clear_settings_exceptions").is_none());
}

#[test]
fn a_review_model_needs_a_known_provider() {
    let mut settings = settings_with_exceptions();
    let mut config = workflow(vec![]);
    let unknown = RunModelOverrides {
        review: Some(ReviewModelChoice {
            provider: "gemini".into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(apply_model_overrides(&mut settings, &mut config, &unknown).is_err());
}

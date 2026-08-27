use super::*;

impl Default for MergeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prompt: prompts::load_prompt("merge").unwrap_or_default(),
            agents: vec![],
        }
    }
}

pub(super) fn default_steps() -> Vec<StepConfig> {
    let mut validate = prompt_step(
        "validate_feedback",
        "Validate Feedback",
        Phase::Sequential,
        &[],
        "validate_feedback",
    );
    validate.enabled = false;
    configure_artifact_flow(
        vec![
            prompt_step(
                "contribution",
                "Contribution",
                Phase::Parallel,
                &["WebSearch"],
                "contribution",
            ),
            prompt_step(
                "technical",
                "Technical Correctness",
                Phase::Parallel,
                &[],
                "technical",
            ),
            prompt_step(
                "empirical",
                "Empirical Strategy",
                Phase::Parallel,
                &[],
                "empirical",
            ),
            prompt_step(
                "consistency",
                "Internal Consistency",
                Phase::Parallel,
                &[],
                "consistency",
            ),
            prompt_step(
                "exposition",
                "Exposition & Framing",
                Phase::Parallel,
                &[],
                "exposition",
            ),
            prompt_step(
                "editor_synthesis",
                "Consolidate Issues",
                Phase::Sequential,
                &[],
                "editor_synthesis",
            ),
            validate,
        ],
        "document",
        &["validate_feedback"],
    )
}

/// Stock Paper Review (Full) profile.
///
/// Retired from the built-in catalog (Automatic Paper Review covers papers), but
/// retained because the v7 shared-context migration fingerprints prior stock
/// variants byte-for-byte. Its step prompts remain shipped defaults.
pub(super) fn full_review_profile(validate_enabled: bool) -> ProfileData {
    let mut steps = default_steps();
    if let Some(step) = steps.iter_mut().find(|step| step.id == "validate_feedback") {
        step.enabled = validate_enabled;
    }
    let mut profile = ProfileData::new("Paper Review (Full)", steps, MergeConfig::default());
    profile.context_cache.enabled = true;
    profile
}

fn adaptive_review_profile(
    name: &str,
    steps: Vec<StepConfig>,
    orientation_schema: serde_json::Value,
) -> ProfileData {
    let mut steps = configure_artifact_flow(steps, "document", &["auto_validate"]);
    // Validation checks the consolidated report against the paper itself; it
    // deliberately does not re-read the raw parallel reports, so materialized
    // specialist output stays consolidated before it is verified.
    if let Some(validate) = steps.iter_mut().find(|step| step.id == "auto_validate") {
        validate.context.include.retain(|selector| match selector {
            ArtifactSelector::Step { step, .. } => step == "auto_synthesis",
            _ => true,
        });
    }
    let mut profile = ProfileData::new(name, steps, MergeConfig::default());
    profile.context_cache.enabled = true;
    profile.outputs.primary_step = "auto_validate".to_string();
    profile.outputs.findings_step = "auto_validate".to_string();
    profile.orientation_prompt = crate::auto_review::orientation_prompt();
    profile.orientation_schema = Some(orientation_schema);
    profile
}

/// Built-in adaptive paper review. The orientation call selects a bounded set
/// of allowlisted specialist IDs. The saved Full profile contains three core
/// reviewers plus synthesis and validation; selected reviewers are
/// materialized for the run.
pub(super) fn auto_review_profile() -> ProfileData {
    adaptive_review_profile(
        "Automatic Paper Review (Full)",
        crate::auto_review::steps(),
        crate::auto_review::orientation_schema(),
    )
}

/// Faster adaptive paper review: two core reviewers, 1–2 subject specialists,
/// 1–2 method specialists, consolidation, and validation.
pub(super) fn quick_auto_review_profile() -> ProfileData {
    adaptive_review_profile(
        "Automatic Paper Review (Quick)",
        crate::auto_review::quick_steps(),
        crate::auto_review::quick_orientation_schema(),
    )
}

pub(super) fn defaults() -> PipelineConfig {
    auto_review_profile().into()
}

/// Profile IDs that cannot be deleted.
// All are recreated by create_builtin_profiles() on startup, so
// deleting any of them would silently "undo" itself — block deletion for all.
pub(super) const BUILTIN_PROFILES: &[&str] = &["auto-review", "auto-review-quick", "grant-review"];

pub(super) fn builtin_primary_readers(id: &str) -> &'static [&'static str] {
    match id {
        "auto-review" | "auto-review-quick" => &["auto_validate"],
        _ => &[],
    }
}

/// Profiles shipped by earlier releases that were removed from the catalog.
/// A version marker makes the archival a one-time migration, so users may
/// later create custom profiles that happen to reuse one of these IDs.
pub(super) const RETIRED_BUILTIN_PROFILES: &[(&str, &str)] = &[
    ("empirical", "deep-review"),
    ("quick-code-review", "deep-review"),
    ("revision-response", "deep-review"),
    ("thesis-review", "deep-review"),
    ("rubric-grading", "deep-review"),
    ("codebase-review", "deep-review"),
];

/// Built-ins retired after the v3 catalog migration had already shipped.
/// These need their own marker so existing installations archive them too.
pub(super) const V9_RETIRED_BUILTIN_PROFILES: &[(&str, &str)] = &[
    ("deep-code-review", "deep-review"),
    ("replication-audit", "deep-review"),
];

/// Paper Review (Full) and (Quick) retired in favor of Automatic Paper Review.
/// Their step prompts remain shipped defaults (`prompts/*.md`), usable from
/// the editor's insertable defaults and prompt overrides. The replacement
/// swap must run after the v3/v9 swaps so chains like
/// `deep-code-review → deep-review → auto-review` resolve fully.
pub(super) const V15_RETIRED_BUILTIN_PROFILES: &[(&str, &str)] = &[
    ("deep-review", "auto-review"),
    ("quick-review", "auto-review-quick"),
];

pub(super) fn profile_summary(id: String, profile: &ProfileData) -> ProfileSummary {
    ProfileSummary {
        builtin: BUILTIN_PROFILES.contains(&id.as_str()),
        id,
        name: profile.name.clone(),
        step_count: profile.steps.len(),
    }
}

/// Step whose prompt is a named compiled-in default. Enabled, no agents.
pub(super) fn prompt_step(
    id: &str,
    label: &str,
    phase: Phase,
    tools: &[&str],
    prompt_name: &str,
) -> StepConfig {
    StepConfig {
        id: id.into(),
        label: label.into(),
        prompt: prompts::load_prompt(prompt_name).unwrap_or_default(),
        enabled: true,
        phase,
        tools: tools.iter().map(|t| t.to_string()).collect(),
        agents: vec![],
        ..Default::default()
    }
}

pub(super) fn primary_selector(input_mode: &str) -> Option<ArtifactSelector> {
    match input_mode {
        "none" => None,
        "folder" => Some(ArtifactSelector::Primary {
            parts: vec![PrimaryArtifactPart::Text, PrimaryArtifactPart::Source],
        }),
        _ => Some(ArtifactSelector::Primary {
            parts: vec![
                PrimaryArtifactPart::Text,
                PrimaryArtifactPart::Structure,
                PrimaryArtifactPart::Visuals,
                PrimaryArtifactPart::Source,
            ],
        }),
    }
}

/// Give shipped and newly-created profiles an explicit dataflow. Parallel
/// analyses receive the primary input and survey. Sequential steps receive
/// all earlier reports; a sequential step that declares Read additionally
/// receives the primary input for evidence verification.
pub(super) fn configure_artifact_flow(
    mut steps: Vec<StepConfig>,
    input_mode: &str,
    primary_readers: &[&str],
) -> Vec<StepConfig> {
    let primary = primary_selector(input_mode);
    let mut prior = Vec::<String>::new();
    for step in &mut steps {
        let mut include = Vec::new();
        let verifies_primary =
            step.phase == Phase::Parallel || primary_readers.contains(&step.id.as_str());
        if verifies_primary {
            if let Some(selector) = primary.clone() {
                include.push(selector);
            }
        }
        include.push(ArtifactSelector::Survey);
        if step.phase == Phase::Sequential {
            include.extend(prior.iter().map(|producer| ArtifactSelector::Step {
                step: producer.clone(),
                parts: vec![StepArtifactPart::Report],
                glob: String::new(),
            }));
        }
        step.after.clear();
        step.context = StepContext { include };
        // Read and Write are derived from the selected artifact view and the
        // producer-owned output directory. They are not profile permissions.
        step.tools
            .retain(|tool| !matches!(tool.as_str(), "Read" | "Write"));
        if step.enabled {
            prior.push(step.id.clone());
        }
    }
    steps
}

/// Domain-neutral profile scaffold: generic wrapper + generic survey prompt.
/// Folder-input profiles get the folder survey, which explores the tree with
/// the Read tool instead of surveying the file inventory text.
pub(super) fn generic_profile(
    name: &str,
    steps: Vec<StepConfig>,
    extraction: ExtractionConfig,
    primary_readers: &[&str],
) -> ProfileData {
    let survey = if extraction.input_mode == "folder" {
        "orientation_folder"
    } else {
        "orientation_generic"
    };
    let steps = configure_artifact_flow(steps, &extraction.input_mode, primary_readers);
    let mut profile = ProfileData::new(name, steps, MergeConfig::default());
    profile.orientation_prompt = prompts::load_prompt(survey).unwrap_or_default();
    profile.orientation_schema = crate::orientation_contract::schema_for_prompt_name(survey);
    profile.extraction = extraction;
    profile.parallel_context_template = generic_parallel_template();
    profile
}

/// Write a built-in profile file if it doesn't exist yet.
pub(super) fn write_builtin_if_missing(path: &Path, profile: &ProfileData) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    let json =
        serde_json::to_string_pretty(profile).map_err(|e| format!("Serialize error: {e}"))?;
    restore_profile_bytes(path, json.as_bytes())
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

/// Stock Grant Proposal Review — document input, panel-reviewer framing.
pub(super) fn grant_review_profile() -> ProfileData {
    const GRANT_TAXONOMY: [&str; 4] = [
        "Aims and Significance",
        "Feasibility and Design",
        "Internal Consistency",
        "Panel Readability",
    ];
    let mut profile = generic_profile(
        "Grant Proposal Review",
        vec![
            prompt_step(
                "grant_aims",
                "Aims & Contribution",
                Phase::Parallel,
                &["WebSearch"],
                "grant_aims",
            ),
            prompt_step(
                "grant_feasibility",
                "Feasibility & Design",
                Phase::Parallel,
                &[],
                "grant_feasibility",
            ),
            prompt_step(
                "grant_clarity",
                "Panel Readability",
                Phase::Parallel,
                &[],
                "grant_clarity",
            ),
            prompt_step(
                "grant_consistency",
                "Internal Consistency",
                Phase::Parallel,
                &[],
                "grant_consistency",
            ),
            prompt_step(
                "grant_synthesis",
                "Consolidate Feedback",
                Phase::Sequential,
                &[],
                "grant_synthesis",
            ),
            prompt_step(
                "grant_validate",
                "Validate Feedback",
                Phase::Sequential,
                &["WebSearch"],
                "grant_validate",
            ),
        ],
        ExtractionConfig::default(),
        &["grant_validate"],
    );
    for step in &mut profile.steps {
        if step.phase == Phase::Parallel {
            step.output_schema = Some(crate::auto_review::specialist_schema());
        }
    }
    if let Some(synthesis) = profile
        .steps
        .iter_mut()
        .find(|step| step.id == "grant_synthesis")
    {
        synthesis.output_schema = Some(serde_json::json!({
            "type": "object",
            crate::pipeline::structured::SCHEMA_REFERENCE_KEY: "findings-v2",
            crate::pipeline::structured::FINDINGS_TAXONOMY_KEY: GRANT_TAXONOMY,
        }));
        synthesis.dependency_policy.required = vec![
            "grant_aims".into(),
            "grant_feasibility".into(),
            "grant_clarity".into(),
            "grant_consistency".into(),
        ];
    }
    if let Some(validate) = profile
        .steps
        .iter_mut()
        .find(|step| step.id == "grant_validate")
    {
        validate.context.include.retain(|selector| match selector {
            ArtifactSelector::Step { step, .. } => step == "grant_synthesis",
            _ => true,
        });
        validate.output_schema = Some(serde_json::json!({
            "type": "object",
            crate::pipeline::structured::SCHEMA_REFERENCE_KEY: "findings-v2-validation",
            crate::pipeline::structured::FINDINGS_TAXONOMY_KEY: GRANT_TAXONOMY,
            crate::pipeline::structured::PRESERVE_FINDINGS_KEY: "grant_synthesis",
        }));
        validate.dependency_policy.required = vec!["grant_synthesis".into()];
    }
    profile.outputs.primary_step = "grant_validate".to_string();
    profile.outputs.findings_step = "grant_validate".to_string();
    profile.orientation_prompt = prompts::load_prompt("orientation_grant").unwrap_or_default();
    profile.orientation_schema =
        crate::orientation_contract::schema_for_prompt_name("orientation_grant");
    profile
}

/// Built-in profiles created on first run.
pub(super) fn create_builtin_profiles() -> Result<(), String> {
    let profiles = profiles_dir()?;

    // Automatic Paper Review — one validated orientation/router call, core
    // reviews, and a bounded set of selected subject/method specialists.
    write_builtin_if_missing(&profiles.join("auto-review.json"), &auto_review_profile())?;
    write_builtin_if_missing(
        &profiles.join("auto-review-quick.json"),
        &quick_auto_review_profile(),
    )?;

    write_builtin_if_missing(&profiles.join("grant-review.json"), &grant_review_profile())?;

    Ok(())
}

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
    orientation_prompt: String,
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
    profile.orientation_prompt = orientation_prompt;
    profile.orientation_schema = Some(orientation_schema);
    profile
}

/// Built-in adaptive paper review. The orientation call selects a bounded set
/// of allowlisted specialist IDs. The saved profile contains three core
/// reviewers plus synthesis and validation; selected reviewers are
/// materialized for the run.
pub(super) fn auto_review_profile() -> ProfileData {
    adaptive_review_profile(
        "Paper Review",
        crate::auto_review::steps(),
        crate::auto_review::orientation_prompt(),
        crate::auto_review::orientation_schema(),
    )
}

/// Faster adaptive paper review: two core reviewers, 1–2 subject specialists,
/// 1–2 method specialists, consolidation, and validation.
pub(super) fn quick_auto_review_profile() -> ProfileData {
    adaptive_review_profile(
        "Paper Review (Fast)",
        crate::auto_review::quick_steps(),
        crate::auto_review::orientation_prompt(),
        crate::auto_review::quick_orientation_schema(),
    )
}

/// Paper Review's reviewers applied to a slide deck (exported as PDF). The
/// router is told the input is a deck, and the schema pins the slide-deck
/// genre, whose host-owned context is injected into every step at run time.
pub(super) fn slide_review_profile() -> ProfileData {
    adaptive_review_profile(
        "Slide Review",
        crate::auto_review::steps(),
        crate::auto_review::slide_orientation_prompt(),
        crate::auto_review::slide_orientation_schema(),
    )
}

/// The stock router prompt and orientation schema for an adaptive built-in.
/// Any other profile, including a custom copy, gets Paper Review's pair.
pub fn adaptive_orientation_defaults(profile_id: &str) -> (String, serde_json::Value) {
    use crate::auto_review as auto;
    match profile_id {
        PAPER_REVIEW_FAST_ID => (auto::orientation_prompt(), auto::quick_orientation_schema()),
        SLIDE_REVIEW_ID => (
            auto::slide_orientation_prompt(),
            auto::slide_orientation_schema(),
        ),
        _ => (auto::orientation_prompt(), auto::orientation_schema()),
    }
}

pub(super) fn defaults() -> PipelineConfig {
    auto_review_profile().into()
}

pub(crate) const PAPER_REVIEW_ID: &str = "auto-paper-review";
pub(crate) const PAPER_REVIEW_FAST_ID: &str = "auto-paper-review-fast";
pub(crate) const SLIDE_REVIEW_ID: &str = "slide-review";
pub(crate) const GRANT_REVIEW_ID: &str = "grant-review";

/// Profile IDs that cannot be deleted.
// All are recreated by create_builtin_profiles() on startup, so
// deleting any of them would silently "undo" itself — block deletion for all.
pub(super) const BUILTIN_PROFILES: &[&str] = &[
    PAPER_REVIEW_ID,
    PAPER_REVIEW_FAST_ID,
    SLIDE_REVIEW_ID,
    GRANT_REVIEW_ID,
    DECLAUDE_ID,
];

/// Built-ins that existed before the version-marked catalog migrations below.
/// Those upgrades apply only to these; a built-in added later (Slide Review)
/// is always created in the current format and must never be replayed.
pub(super) const MIGRATED_BUILTIN_PROFILES: &[&str] =
    &[PAPER_REVIEW_ID, PAPER_REVIEW_FAST_ID, GRANT_REVIEW_ID];

pub(super) fn builtin_primary_readers(id: &str) -> &'static [&'static str] {
    match id {
        PAPER_REVIEW_ID | PAPER_REVIEW_FAST_ID | SLIDE_REVIEW_ID => &["auto_validate"],
        _ => &[],
    }
}

/// A built-in whose ID and stock display name changed without changing its
/// definition. Unlike a retirement, the user's file (edits included) moves to
/// the new ID once, and the old ID keeps resolving while no file claims it.
pub(super) struct RenamedBuiltin {
    pub old_id: &'static str,
    pub id: &'static str,
    pub old_name: &'static str,
    pub name: &'static str,
}

pub(super) const RENAMED_BUILTIN_PROFILES: &[RenamedBuiltin] = &[
    RenamedBuiltin {
        old_id: "auto-review",
        id: PAPER_REVIEW_ID,
        old_name: "Automatic Paper Review (Full)",
        name: "Paper Review",
    },
    RenamedBuiltin {
        old_id: "auto-review-quick",
        id: PAPER_REVIEW_FAST_ID,
        old_name: "Automatic Paper Review (Quick)",
        name: "Paper Review (Fast)",
    },
];

/// The current ID for a profile reference that may predate a rename. Durable
/// references (Task chains, missions, CLI scripts) keep working, but a file
/// that has since claimed the old ID always wins.
pub(super) fn resolve_renamed_profile_id<'a>(profiles: &Path, id: &'a str) -> &'a str {
    match RENAMED_BUILTIN_PROFILES
        .iter()
        .find(|renamed| renamed.old_id == id)
    {
        Some(renamed) if !profiles.join(format!("{id}.json")).exists() => renamed.id,
        _ => id,
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

/// Paper Review (Full) and (Quick) retired in favor of Automatic Paper Review,
/// since renamed Paper Review. Their step prompts remain shipped defaults
/// (`prompts/*.md`), usable from the editor's insertable defaults and prompt
/// overrides. The replacement swap must run after the v3/v9 swaps so chains
/// like `deep-code-review → deep-review → auto-paper-review` resolve fully.
pub(super) const V15_RETIRED_BUILTIN_PROFILES: &[(&str, &str)] = &[
    ("deep-review", PAPER_REVIEW_ID),
    ("quick-review", PAPER_REVIEW_FAST_ID),
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

    // Paper Review and Slide Review — one validated orientation/router call,
    // core reviews, and a bounded set of selected subject/method specialists.
    for (id, profile) in [
        (PAPER_REVIEW_ID, auto_review_profile()),
        (PAPER_REVIEW_FAST_ID, quick_auto_review_profile()),
        (SLIDE_REVIEW_ID, slide_review_profile()),
        (GRANT_REVIEW_ID, grant_review_profile()),
        (DECLAUDE_ID, declaude_profile()),
    ] {
        write_builtin_if_missing(&profiles.join(format!("{id}.json")), &profile)?;
    }

    Ok(())
}

/// Default parallel-step context template, with user override applied from
/// ~/.pipeline/prompts/parallel_context.md if present.
///
/// This is the paper-review wrapper — it remains the fallback default because
/// the built-in profiles and all pre-generalization profiles assume it.
pub fn default_parallel_template() -> String {
    prompts::load_prompt("parallel_context").unwrap_or_default()
}

/// Generic (domain-neutral) parallel-step context template.
pub fn generic_parallel_template() -> String {
    prompts::load_prompt("parallel_context_generic").unwrap_or_default()
}

/// Starter steps for a brand-new profile: one parallel step to replace and a
/// generic consolidation step. New profiles are domain-neutral; the paper
/// machinery lives in the built-in review profiles.
pub(super) fn generic_starter_steps() -> Vec<StepConfig> {
    vec![
        StepConfig {
            id: "analysis".into(),
            label: "Analysis".into(),
            prompt: "Replace this with instructions for the step. It receives only the \
                     artifacts selected in this step's context."
                .into(),
            enabled: true,
            phase: Phase::Parallel,
            tools: vec![],
            agents: vec![],
            ..Default::default()
        },
        StepConfig {
            id: "synthesis".into(),
            label: "Synthesize".into(),
            prompt: "Consolidate the selected upstream reports into a single report. Merge \
                     duplicate findings, resolve contradictions, and order by importance.\n\n\
                     {prior_outputs}"
                .into(),
            enabled: true,
            phase: Phase::Sequential,
            tools: vec![],
            agents: vec![],
            context: StepContext {
                include: vec![ArtifactSelector::Step {
                    step: "analysis".into(),
                    parts: vec![StepArtifactPart::Report],
                    glob: String::new(),
                }],
            },
            ..Default::default()
        },
    ]
}

//! Pipeline configuration: profiles, steps, and post-processing.
//! Profiles stored at ~/.pipeline/profiles/{id}.json.
//! Active profile tracked via settings.active_profile.

use crate::prompts;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_PROFILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PROFILE_STEPS: usize = 100;
pub const MAX_STEP_PROMPT_BYTES: usize = 1024 * 1024;
pub const MAX_FAN_OUT_ITEMS: u32 = 100;
const MAX_PROFILE_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_STEP_LABEL_CHARS: usize = 200;
const MAX_PROFILE_NAME_CHARS: usize = 200;
const MAX_VARIABLES: usize = 100;
const MAX_EXTRA_INPUTS: usize = 100;
const MAX_OUTPUT_SCHEMA_BYTES: usize = 1024 * 1024;
const ALLOWED_TOOLS: &[&str] = &["Read", "Write", "WebSearch"];
const ALLOWED_AGENTS: &[&str] = &["claude", "codex", "gemini", "local"];

fn read_profile_file(path: &Path) -> Result<String, String> {
    use std::io::Read as _;
    let file = fs::File::open(path)
        .map_err(|e| format!("Failed to read profile '{}': {e}", path.display()))?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(MAX_PROFILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read profile '{}': {e}", path.display()))?;
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(format!(
            "Profile '{}' exceeds the {} MB safety limit",
            path.display(),
            MAX_PROFILE_BYTES / 1024 / 1024
        ));
    }
    String::from_utf8(bytes)
        .map_err(|e| format!("Profile '{}' is not valid UTF-8: {e}", path.display()))
}

// ── Data types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Parallel,
    Sequential,
}

fn default_tools() -> Vec<String> {
    vec![]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepConfig {
    pub id: String,
    pub label: String,
    pub prompt: String,
    pub enabled: bool,
    pub phase: Phase,
    #[serde(default = "default_tools")]
    pub tools: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    /// Per-step model override. Empty = use the global setting for this step's provider.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Provider/transport-specific model policies, keyed as `claude:cli`,
    /// `codex:api`, etc. A plain provider key is accepted as a portable
    /// fallback. This supersedes the legacy single `model` string.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub model_overrides: std::collections::HashMap<String, crate::settings::ModelSelection>,
    /// Per-step effort override (low/medium/high/max). Empty = use the global setting.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub effort: String,
    /// Provider/transport-specific effort overrides. Supersedes `effort`.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub effort_overrides: std::collections::HashMap<String, String>,
    /// Explicit upstream dependencies (step ids). Empty = the implicit
    /// adjacency schedule (parallel steps run in their wave; sequential steps
    /// wait for everything before them). Non-empty = this step waits for
    /// exactly these steps, and its `{prior_outputs}`/`{step:id}` placeholders
    /// resolve against them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<String>,
    /// Optional guard: when present and its condition is not met, the step is
    /// skipped (a skip placeholder is recorded so dependents can proceed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_if: Option<RunCondition>,
    /// Optional JSON-shape contract for the step's output. When set, the step's
    /// report must parse as JSON and satisfy the schema, with a retry on
    /// failure (see `structured::validate`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Fan-out (map): run this step once per file matching a glob under the
    /// input, with `{item}` bound to each file. Parallel steps only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub for_each: Option<ForEach>,
}

/// Fan-out configuration for a step: run its prompt once per matching file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForEach {
    /// Glob relative to the input root (e.g. "chapters/*.tex", "**/*.py").
    pub glob: String,
    /// Hard cap on the number of items, to bound cost. Defaults to 20.
    #[serde(default = "default_for_each_max")]
    pub max: u32,
}

fn default_for_each_max() -> u32 {
    20
}

/// A deterministic, engine-evaluated condition gating whether a step runs.
/// Deliberately not an LLM call and not a general expression language — two
/// concrete checks that cover "only run X when the survey says Y" and
/// "only run X when an earlier step flagged Z".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunCondition {
    /// Run only if a prior step's output text matches (or, with `negate`, does
    /// not match) the given regular expression.
    OutputMatches {
        step: String,
        pattern: String,
        #[serde(default)]
        negate: bool,
    },
    /// Run only based on a JSON-pointer into the survey (orientation) JSON.
    /// With `equals`, run iff the pointed-to value equals it. With `exists`,
    /// run iff presence matches the boolean. If both are set, both must hold.
    SurveyPath {
        pointer: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        equals: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exists: Option<bool>,
    },
}

impl Default for StepConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            prompt: String::new(),
            enabled: true,
            phase: Phase::Parallel,
            tools: Vec::new(),
            agents: Vec::new(),
            model: String::new(),
            model_overrides: std::collections::HashMap::new(),
            effort: String::new(),
            effort_overrides: std::collections::HashMap::new(),
            inputs: Vec::new(),
            run_if: None,
            output_schema: None,
            for_each: None,
        }
    }
}

impl StepConfig {
    /// Resolve the most specific model policy for a provider. New
    /// provider/transport keys win, followed by a provider-wide override and
    /// finally the legacy free-text field.
    pub fn model_selection_for(
        &self,
        settings: &crate::settings::Settings,
        provider: &str,
    ) -> Option<crate::settings::ModelSelection> {
        let context = settings.model_context_key(provider);
        self.model_overrides
            .get(&context)
            .or_else(|| self.model_overrides.get(provider))
            .cloned()
            .or_else(|| {
                (!self.model.trim().is_empty())
                    .then(|| crate::settings::ModelSelection::from_legacy(&self.model))
            })
    }

    pub fn effort_for(&self, settings: &crate::settings::Settings, provider: &str) -> String {
        let context = settings.model_context_key(provider);
        self.effort_overrides
            .get(&context)
            .or_else(|| self.effort_overrides.get(provider))
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .or_else(|| (!self.effort.trim().is_empty()).then(|| self.effort.clone()))
            .unwrap_or_else(|| settings.model_effort(provider).to_string())
    }
}

/// Configuration for the cross-agent merge step.
/// When a step runs on multiple LLM agents, this merges their
/// independent outputs into a single report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeConfig {
    pub enabled: bool,
    /// Prompt template. Placeholders replaced at runtime:
    ///   {topic}          — step label (e.g., "Contribution")
    ///   {agent_reports}  — all agent analyses for this step, as markdown
    pub prompt: String,
    /// Which provider runs the merge calls. Empty = global setting.
    #[serde(default)]
    pub agents: Vec<String>,
}

/// A run-time variable a profile declares. When present, the Run action
/// prompts for values (pre-filled with `default`) and steps reference them as
/// `{var:key}`. Turns a profile into a reusable template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarSpec {
    pub key: String,
    #[serde(default)]
    pub label: String,
    /// "text" | "choice" | "file". Advisory — the value is always a string.
    #[serde(default = "default_var_kind")]
    pub kind: String,
    #[serde(default)]
    pub default: String,
    /// Options for `kind == "choice"`.
    #[serde(default)]
    pub choices: Vec<String>,
}

fn default_var_kind() -> String {
    "text".to_string()
}

/// An additional named input a profile accepts beyond the primary one. Its
/// extracted text is exposed to prompts as `{input:key}` (a path to Read).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSlot {
    pub key: String,
    #[serde(default)]
    pub label: String,
    /// "document" (single file) or "folder". Defaults to document.
    #[serde(default = "default_slot_mode")]
    pub mode: String,
    #[serde(default)]
    pub required: bool,
}

fn default_slot_mode() -> String {
    "document".to_string()
}

/// Per-profile extraction overrides. When `method` is empty, the global
/// Settings value is used; same for the marker flags (which fall back to
/// the global toggle when this struct is absent on a profile).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// "auto" | "llm" | "marker" | "pdftotext" | "" (= inherit global).
    #[serde(default)]
    pub method: String,
    /// Input mode for the workflow: "" or "document" (single file, default),
    /// "folder" (inventory of a directory; steps Read files on demand), or
    /// "none" (runs from the prompts alone).
    #[serde(default)]
    pub input_mode: String,
    /// `Some` overrides the global setting; `None` inherits.
    #[serde(default)]
    pub marker_disable_ocr: Option<bool>,
    #[serde(default)]
    pub marker_disable_images: Option<bool>,
    /// Extra named inputs (beyond the primary one) this profile accepts.
    #[serde(default)]
    pub extra_inputs: Vec<InputSlot>,
}

/// Combined config returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub steps: Vec<StepConfig>,
    #[serde(default)]
    pub merge: MergeConfig,
    /// Whether to build an orientation map before running steps.
    #[serde(default = "default_true")]
    pub use_orientation: bool,
    /// Per-profile orientation prompt. Empty = use the default template loaded
    /// from prompts/orientation.md (or the user override at
    /// ~/.pipeline/prompts/orientation.md). The placeholder `{paper_text}` is
    /// substituted with the extracted paper at runtime.
    #[serde(default)]
    pub orientation_prompt: String,
    /// Per-profile extraction overrides. When unset (default), the global
    /// Settings values are used.
    #[serde(default)]
    pub extraction: ExtractionConfig,
    /// Template wrapping each parallel step's prompt. Placeholders:
    ///   {step_prompt}  — the step's own instructions
    ///   {paper_type}   — theory / empirical / mixed
    ///   {orientation}  — orientation map reference (empty if disabled)
    ///   {paper_path}   — path to extracted paper text
    ///   {figure_hint}  — figure/table access instructions
    #[serde(default = "default_parallel_template")]
    pub parallel_context_template: String,
    /// Run-time variables the profile declares; empty for most profiles.
    #[serde(default)]
    pub variables: Vec<VarSpec>,
}

/// On-disk profile format stored in ~/.pipeline/profiles/{id}.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileData {
    pub name: String,
    pub steps: Vec<StepConfig>,
    #[serde(default)]
    pub merge: MergeConfig,
    #[serde(default = "default_true")]
    pub use_orientation: bool,
    #[serde(default)]
    pub orientation_prompt: String,
    #[serde(default)]
    pub extraction: ExtractionConfig,
    #[serde(default = "default_parallel_template")]
    pub parallel_context_template: String,
    #[serde(default)]
    pub variables: Vec<VarSpec>,
}

impl ProfileData {
    /// Create a new profile with default pipeline settings.
    pub fn new(name: impl Into<String>, steps: Vec<StepConfig>, merge: MergeConfig) -> Self {
        Self {
            name: name.into(),
            steps,
            merge,
            use_orientation: true,
            orientation_prompt: String::new(),
            extraction: ExtractionConfig::default(),
            parallel_context_template: default_parallel_template(),
            variables: Vec::new(),
        }
    }

    fn from_config(name: impl Into<String>, config: &PipelineConfig) -> Self {
        Self {
            name: name.into(),
            steps: config.steps.clone(),
            merge: config.merge.clone(),
            use_orientation: config.use_orientation,
            orientation_prompt: config.orientation_prompt.clone(),
            extraction: config.extraction.clone(),
            parallel_context_template: config.parallel_context_template.clone(),
            variables: config.variables.clone(),
        }
    }
}

impl From<ProfileData> for PipelineConfig {
    fn from(profile: ProfileData) -> Self {
        Self {
            steps: profile.steps,
            merge: profile.merge,
            use_orientation: profile.use_orientation,
            orientation_prompt: profile.orientation_prompt,
            extraction: profile.extraction,
            parallel_context_template: profile.parallel_context_template,
            variables: profile.variables,
        }
    }
}

fn default_true() -> bool {
    true
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
fn generic_starter_steps() -> Vec<StepConfig> {
    vec![
        StepConfig {
            id: "analysis".into(),
            label: "Analysis".into(),
            prompt: "Replace this with instructions for the step. Adjacent parallel steps run \
                     concurrently, each in a clean context, with the survey (orientation map) \
                     as shared grounding."
                .into(),
            enabled: true,
            phase: Phase::Parallel,
            tools: vec!["Read".into()],
            agents: vec![],
            ..Default::default()
        },
        StepConfig {
            id: "synthesis".into(),
            label: "Synthesize".into(),
            prompt: "Consolidate the outputs of all prior steps into a single report. Merge \
                     duplicate findings, resolve contradictions, and order by importance.\n\n\
                     {prior_outputs}"
                .into(),
            enabled: true,
            phase: Phase::Sequential,
            tools: vec![],
            agents: vec![],
            ..Default::default()
        },
    ]
}

/// Summary returned when listing profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub step_count: usize,
    #[serde(default)]
    pub builtin: bool,
}

// ── Export/Import envelope ──────────────────────────────────────────

/// Current profile/export schema version. v2 is the generalized-engine format
/// (Release 1.2+): step dependencies, conditions, variables, named inputs,
/// output schemas, and fan-out. v1 (unversioned) profiles read fine because
/// every added field is `#[serde(default)]`; exports are tagged with the
/// version so a future format change can migrate or reject gracefully.
pub const CURRENT_SCHEMA_VERSION: u32 = 3;

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ExportEnvelope {
    #[serde(rename = "step")]
    Step { data: StepConfig },
    #[serde(rename = "profile")]
    Profile {
        #[serde(default = "default_schema_version")]
        schema_version: u32,
        name: String,
        steps: Vec<StepConfig>,
        #[serde(default)]
        merge: MergeConfig,
        #[serde(default = "default_true")]
        use_orientation: bool,
        #[serde(default)]
        orientation_prompt: String,
        #[serde(default)]
        extraction: ExtractionConfig,
        #[serde(default = "default_parallel_template")]
        parallel_context_template: String,
        #[serde(default)]
        variables: Vec<VarSpec>,
    },
    #[serde(rename = "bundle")]
    Bundle {
        settings: crate::settings::Settings,
        profiles: Vec<ProfileExport>,
        active_profile: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileExport {
    pub id: String,
    pub name: String,
    pub steps: Vec<StepConfig>,
    #[serde(default)]
    pub merge: MergeConfig,
    #[serde(default = "default_true")]
    pub use_orientation: bool,
    #[serde(default)]
    pub orientation_prompt: String,
    #[serde(default)]
    pub extraction: ExtractionConfig,
    #[serde(default = "default_parallel_template")]
    pub parallel_context_template: String,
    #[serde(default)]
    pub variables: Vec<VarSpec>,
}

impl ProfileExport {
    fn from_profile(id: String, profile: ProfileData) -> Self {
        Self {
            id,
            name: profile.name,
            steps: profile.steps,
            merge: profile.merge,
            use_orientation: profile.use_orientation,
            orientation_prompt: profile.orientation_prompt,
            extraction: profile.extraction,
            parallel_context_template: profile.parallel_context_template,
            variables: profile.variables,
        }
    }

    fn to_profile_data(&self) -> ProfileData {
        ProfileData {
            name: self.name.clone(),
            steps: self.steps.clone(),
            merge: self.merge.clone(),
            use_orientation: self.use_orientation,
            orientation_prompt: self.orientation_prompt.clone(),
            extraction: self.extraction.clone(),
            parallel_context_template: self.parallel_context_template.clone(),
            variables: self.variables.clone(),
        }
    }
}

// ── Legacy types (deserialization only) ────────────────────────────

#[derive(Deserialize)]
struct LegacyRefereeConfig {
    id: String,
    label: String,
    prompt: String,
    enabled: bool,
    #[serde(default)]
    web_search: bool,
    #[serde(default)]
    agents: Vec<String>,
}

#[derive(Deserialize)]
struct LegacyPostStepConfig {
    id: String,
    label: String,
    prompt: String,
    enabled: bool,
    #[serde(default)]
    agents: Vec<String>,
}

#[derive(Deserialize)]
struct LegacyProfileData {
    name: String,
    #[serde(default)]
    referees: Vec<LegacyRefereeConfig>,
    #[serde(default)]
    post_steps: Vec<LegacyPostStepConfig>,
    #[serde(default)]
    merge: MergeConfig,
}

fn referee_to_step(r: LegacyRefereeConfig) -> StepConfig {
    let tools = if r.web_search {
        vec!["WebSearch".to_string()]
    } else {
        vec![]
    };
    StepConfig {
        id: r.id,
        label: r.label,
        prompt: r.prompt,
        enabled: r.enabled,
        phase: Phase::Parallel,
        tools,
        agents: r.agents,
        ..Default::default()
    }
}

fn post_step_to_step(p: LegacyPostStepConfig) -> StepConfig {
    StepConfig {
        id: p.id,
        label: p.label,
        prompt: p.prompt,
        enabled: p.enabled,
        phase: Phase::Sequential,
        tools: vec![],
        agents: p.agents,
        ..Default::default()
    }
}

fn convert_legacy_steps(
    referees: Vec<LegacyRefereeConfig>,
    post_steps: Vec<LegacyPostStepConfig>,
) -> Vec<StepConfig> {
    let mut steps: Vec<StepConfig> = referees.into_iter().map(referee_to_step).collect();
    steps.extend(post_steps.into_iter().map(post_step_to_step));
    steps
}

// ── Paths & helpers ─────────────────────────────────────────────────

fn profiles_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".pipeline").join("profiles");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create profiles dir: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)) {
            eprintln!(
                "WARNING: could not tighten permissions on {}: {e}",
                dir.display()
            );
        }
    }
    Ok(dir)
}

fn profile_path(id: &str) -> Result<PathBuf, String> {
    validate_profile_id(id)?;
    Ok(profiles_dir()?.join(format!("{id}.json")))
}

fn validate_profile_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Profile ID cannot be empty".into());
    }
    if id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Profile ID must use 1–64 ASCII letters, numbers, hyphens, or underscores".into(),
        );
    }
    Ok(())
}

pub fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Sanitize a step ID by replacing characters reserved for internal use.
/// `/` is the step_id/agent delimiter in multi-agent composite keys;
/// allowing it in step IDs would corrupt merge grouping and multi-agent detection.
pub fn sanitize_step_id(id: &str) -> String {
    let sanitized: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    // Collapse consecutive dashes and trim leading/trailing
    sanitized
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// ── Defaults ────────────────────────────────────────────────────────
//
// The four pipeline-level prompts (parallel_context, merge, editor_synthesis,
// validate_feedback) live in prompts/*.md and are loaded through prompts::load_prompt,
// which lets users override them at ~/.pipeline/prompts/<name>.md.

impl Default for MergeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prompt: prompts::load_prompt("merge").unwrap_or_default(),
            agents: vec![],
        }
    }
}

fn default_steps() -> Vec<StepConfig> {
    let mut validate = prompt_step(
        "validate_feedback",
        "Validate Feedback",
        Phase::Sequential,
        &[],
        "validate_feedback",
    );
    validate.enabled = false;
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
    ]
}

fn defaults() -> PipelineConfig {
    PipelineConfig {
        steps: default_steps(),
        merge: MergeConfig::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        extraction: ExtractionConfig::default(),
        parallel_context_template: default_parallel_template(),
        variables: Vec::new(),
    }
}

/// Profile IDs that cannot be deleted.
// All are recreated by create_builtin_profiles() on startup, so
// deleting any of them would silently "undo" itself — block deletion for all.
const BUILTIN_PROFILES: &[&str] = &[
    "deep-review",
    "quick-review",
    "empirical",
    "quick-code-review",
    "deep-code-review",
    "replication-audit",
    "grant-review",
    "revision-response",
    "rubric-grading",
    "thesis-review",
];

fn profile_summary(id: String, profile: &ProfileData) -> ProfileSummary {
    ProfileSummary {
        builtin: BUILTIN_PROFILES.contains(&id.as_str()),
        id,
        name: profile.name.clone(),
        step_count: profile.steps.len(),
    }
}

/// Step whose prompt is a named compiled-in default. Enabled, no agents.
fn prompt_step(
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

fn folder_extraction() -> ExtractionConfig {
    ExtractionConfig {
        input_mode: "folder".into(),
        ..Default::default()
    }
}

/// A step with an inline prompt (not a compiled-in named default).
fn inline_step(id: &str, label: &str, phase: Phase, tools: &[&str], prompt: &str) -> StepConfig {
    StepConfig {
        id: id.into(),
        label: label.into(),
        prompt: prompt.into(),
        enabled: true,
        phase,
        tools: tools.iter().map(|t| t.to_string()).collect(),
        agents: vec![],
        ..Default::default()
    }
}

/// The issues schema that enables the Issues table + annotations in the report.
fn issues_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["issues"],
        "properties": {
            "issues": {
                "type": "array",
                "items": { "type": "object", "required": ["title", "severity", "body"] }
            }
        }
    })
}

/// A sequential consolidation step that emits structured issues.
fn issues_synthesis_step(id: &str, label: &str) -> StepConfig {
    let mut s = prompt_step(id, label, Phase::Sequential, &[], "editor_synthesis_issues");
    s.output_schema = Some(issues_schema());
    s
}

/// Domain-neutral profile scaffold: generic wrapper + generic survey prompt.
/// Folder-input profiles get the folder survey, which explores the tree with
/// the Read tool instead of surveying the file inventory text.
fn generic_profile(
    name: &str,
    steps: Vec<StepConfig>,
    extraction: ExtractionConfig,
) -> ProfileData {
    let survey = if extraction.input_mode == "folder" {
        "orientation_folder"
    } else {
        "orientation_generic"
    };
    let mut profile = ProfileData::new(name, steps, MergeConfig::default());
    profile.orientation_prompt = prompts::load_prompt(survey).unwrap_or_default();
    profile.extraction = extraction;
    profile.parallel_context_template = generic_parallel_template();
    profile
}

/// Write a built-in profile file if it doesn't exist yet.
fn write_builtin_if_missing(path: &PathBuf, profile: &ProfileData) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    let json =
        serde_json::to_string_pretty(profile).map_err(|e| format!("Serialize error: {e}"))?;
    fs::write(path, json).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

/// Built-in profiles created on first run.
fn create_builtin_profiles() -> Result<(), String> {
    let profiles = profiles_dir()?;

    // Quick Review — fast two-step pass
    write_builtin_if_missing(
        &profiles.join("quick-review.json"),
        &ProfileData::new(
            "Quick Review",
            vec![
                prompt_step(
                    "contribution",
                    "Contribution",
                    Phase::Parallel,
                    &[],
                    "contribution",
                ),
                prompt_step(
                    "consistency",
                    "Internal Consistency",
                    Phase::Parallel,
                    &[],
                    "consistency",
                ),
                prompt_step(
                    "editor_synthesis",
                    "Consolidate Issues",
                    Phase::Sequential,
                    &[],
                    "editor_synthesis",
                ),
            ],
            MergeConfig::default(),
        ),
    )?;

    // Empirical — tailored for empirical papers
    write_builtin_if_missing(
        &profiles.join("empirical.json"),
        &ProfileData::new(
            "Empirical",
            vec![
                prompt_step(
                    "contribution",
                    "Contribution",
                    Phase::Parallel,
                    &["WebSearch"],
                    "contribution",
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
                prompt_step(
                    "validate_feedback",
                    "Validate Feedback",
                    Phase::Sequential,
                    &[],
                    "validate_feedback",
                ),
            ],
            MergeConfig::default(),
        ),
    )?;

    // Quick Code Review (formerly "Codebase Review") — folder input,
    // generic wrapper + survey. Migrate away the pre-rename file.
    let stale_codebase = profiles.join("codebase-review.json");
    if stale_codebase.exists() {
        let _ = fs::remove_file(&stale_codebase);
        let _ = crate::settings::replace_active_profile_if("codebase-review", "quick-code-review");
    }
    write_builtin_if_missing(
        &profiles.join("quick-code-review.json"),
        &generic_profile(
            "Quick Code Review",
            vec![
                prompt_step(
                    "code_correctness",
                    "Correctness",
                    Phase::Parallel,
                    &["Read"],
                    "code_correctness",
                ),
                prompt_step(
                    "code_design",
                    "Design & Maintainability",
                    Phase::Parallel,
                    &["Read"],
                    "code_design",
                ),
                prompt_step(
                    "code_security",
                    "Security",
                    Phase::Parallel,
                    &["Read"],
                    "code_security",
                ),
                prompt_step(
                    "code_synthesis",
                    "Consolidate Findings",
                    Phase::Sequential,
                    &[],
                    "code_synthesis",
                ),
            ],
            folder_extraction(),
        ),
    )?;

    // Deep Code Review — seven parallel passes, consolidate, then a
    // sequential verify step that re-reads the code to refute findings.
    write_builtin_if_missing(
        &profiles.join("deep-code-review.json"),
        &generic_profile(
            "Deep Code Review",
            vec![
                prompt_step(
                    "code_correctness",
                    "Correctness",
                    Phase::Parallel,
                    &["Read"],
                    "code_correctness",
                ),
                prompt_step(
                    "code_security",
                    "Security",
                    Phase::Parallel,
                    &["Read"],
                    "code_security",
                ),
                prompt_step(
                    "code_design",
                    "Design & Maintainability",
                    Phase::Parallel,
                    &["Read"],
                    "code_design",
                ),
                prompt_step(
                    "code_concurrency",
                    "Concurrency & Resources",
                    Phase::Parallel,
                    &["Read"],
                    "code_concurrency",
                ),
                prompt_step(
                    "code_errors",
                    "Error Handling & Edge Cases",
                    Phase::Parallel,
                    &["Read"],
                    "code_errors",
                ),
                prompt_step(
                    "code_performance",
                    "Performance",
                    Phase::Parallel,
                    &["Read"],
                    "code_performance",
                ),
                prompt_step(
                    "code_tests",
                    "Test Coverage & Quality",
                    Phase::Parallel,
                    &["Read"],
                    "code_tests",
                ),
                prompt_step(
                    "code_synthesis",
                    "Consolidate Findings",
                    Phase::Sequential,
                    &[],
                    "code_synthesis",
                ),
                prompt_step(
                    "code_verify",
                    "Verify Findings",
                    Phase::Sequential,
                    &["Read"],
                    "code_verify",
                ),
            ],
            folder_extraction(),
        ),
    )?;

    // Replication Package Audit — data-editor-style check of a paper's
    // replication package (folder input).
    write_builtin_if_missing(
        &profiles.join("replication-audit.json"),
        &generic_profile(
            "Replication Package Audit",
            vec![
                prompt_step(
                    "repl_completeness",
                    "Exhibit Completeness",
                    Phase::Parallel,
                    &["Read"],
                    "repl_completeness",
                ),
                prompt_step(
                    "repl_consistency",
                    "Code–Paper Consistency",
                    Phase::Parallel,
                    &["Read"],
                    "repl_consistency",
                ),
                prompt_step(
                    "repl_portability",
                    "Portability",
                    Phase::Parallel,
                    &["Read"],
                    "repl_portability",
                ),
                prompt_step(
                    "repl_provenance",
                    "Data Provenance",
                    Phase::Parallel,
                    &["Read"],
                    "repl_provenance",
                ),
                prompt_step(
                    "repl_synthesis",
                    "Consolidate Audit",
                    Phase::Sequential,
                    &[],
                    "repl_synthesis",
                ),
            ],
            folder_extraction(),
        ),
    )?;

    // Grant Proposal Review — document input, panel-reviewer framing.
    write_builtin_if_missing(
        &profiles.join("grant-review.json"),
        &generic_profile(
            "Grant Proposal Review",
            vec![
                prompt_step(
                    "grant_aims",
                    "Aims & Contribution",
                    Phase::Parallel,
                    &["Read", "WebSearch"],
                    "grant_aims",
                ),
                prompt_step(
                    "grant_feasibility",
                    "Feasibility & Design",
                    Phase::Parallel,
                    &["Read"],
                    "grant_feasibility",
                ),
                prompt_step(
                    "grant_clarity",
                    "Panel Readability",
                    Phase::Parallel,
                    &["Read"],
                    "grant_clarity",
                ),
                prompt_step(
                    "grant_consistency",
                    "Internal Consistency",
                    Phase::Parallel,
                    &["Read"],
                    "grant_consistency",
                ),
                prompt_step(
                    "grant_synthesis",
                    "Consolidate Feedback",
                    Phase::Sequential,
                    &[],
                    "grant_synthesis",
                ),
            ],
            ExtractionConfig::default(),
        ),
    )?;

    // ── Release 2.0 profiles: exercise the generalized engine ──────

    // Revision Response Check — revised paper + response letter + prior report.
    write_builtin_if_missing(&profiles.join("revision-response.json"), &{
        let extraction = ExtractionConfig {
            extra_inputs: vec![
                InputSlot {
                    key: "response".into(),
                    label: "Response letter".into(),
                    mode: "document".into(),
                    required: true,
                },
                InputSlot {
                    key: "prior_report".into(),
                    label: "Prior referee report".into(),
                    mode: "document".into(),
                    required: false,
                },
            ],
            ..Default::default()
        };
        let mut p = generic_profile(
            "Revision Response Check",
            vec![
                inline_step("verify_changes", "Verify Claimed Changes", Phase::Parallel, &["Read"],
                    "You are checking a revised paper against the authors' response to referees.\n\n\
                     The revised paper text is provided. The response letter is at {input:response}. \
                     A prior referee report, if available, is at {input:prior_report}.\n\n\
                     For each change the authors claim to have made, verify whether the revised paper actually \
                     reflects it. Flag: claims not supported by the paper, changes that introduce new problems, \
                     and prior concerns the response fails to address. Do not praise or summarize."),
                issues_synthesis_step("consolidate", "Consolidate Verdicts"),
            ],
            extraction,
        );
        p.orientation_prompt = prompts::load_prompt("orientation_generic").unwrap_or_default();
        p
    })?;

    // Rubric Grading — grade a document against a rubric, with course variables.
    write_builtin_if_missing(&profiles.join("rubric-grading.json"), &{
        let extraction = ExtractionConfig {
            extra_inputs: vec![InputSlot {
                key: "rubric".into(),
                label: "Grading rubric".into(),
                mode: "document".into(),
                required: true,
            }],
            ..Default::default()
        };
        let mut p = generic_profile(
            "Rubric Grading",
            vec![
                inline_step("grade", "Grade Against Rubric", Phase::Parallel, &["Read"],
                    "Grade this submission for the course \"{var:course}\" against the rubric at {input:rubric}.\n\n\
                     Go criterion by criterion: state the criterion, the score or level you assign, and one or two \
                     sentences of specific, evidence-based justification citing the submission. End with the total \
                     and the two highest-leverage improvements."),
                inline_step("summary", "Grade Summary", Phase::Sequential, &[],
                    "Produce the final graded feedback: the per-criterion scores and justifications, the total, \
                     and a short overall comment.\n\n{prior_outputs}"),
            ],
            extraction,
        );
        p.variables = vec![VarSpec {
            key: "course".into(),
            label: "Course".into(),
            kind: "text".into(),
            default: String::new(),
            choices: vec![],
        }];
        p
    })?;

    // Thesis Review — fan out per chapter, then a cross-chapter synthesis.
    write_builtin_if_missing(
        &profiles.join("thesis-review.json"),
        &generic_profile(
            "Thesis Review",
            vec![
                {
                    let mut s = inline_step("chapter_review", "Chapter Review", Phase::Parallel, &["Read"],
                        "Review the chapter/section file at {item}. Identify substantive issues: gaps in the \
                         argument, unclear or unsupported claims, methodological problems, and exposition that \
                         would confuse a reader. Reference the file. Do not praise or summarize.");
                    s.for_each = Some(ForEach { glob: "**/*.tex".into(), max: 20 });
                    s
                },
                inline_step("cross_chapter", "Cross-Chapter Synthesis", Phase::Sequential, &[],
                    "You have per-chapter reviews of a thesis below. Synthesize them into a single ordered list \
                     of the most important issues, and add cross-chapter problems the per-chapter reviews could \
                     not see: inconsistent notation or terminology across chapters, redundancy, contradictory \
                     claims, and gaps between chapters.\n\n{prior_outputs}"),
            ],
            folder_extraction(),
        ),
    )?;

    Ok(())
}

// ── Migration ───────────────────────────────────────────────────────

/// Migrate from old single-file formats to profiles directory. Idempotent.
fn ensure_migrated() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let old_path = home.join(".pipeline").join("pipeline.json");
    let profiles = profiles_dir()?;
    let deep_review_path = profiles.join("deep-review.json");

    if !deep_review_path.exists() {
        // Migrate from pipeline.json → save as a "Migrated" profile
        if old_path.exists() {
            if let Ok(content) = read_profile_file(&old_path) {
                // Try legacy format with referees/post_steps
                if let Ok(legacy) = serde_json::from_str::<LegacyProfileData>(&content) {
                    let profile = ProfileData::new(
                        "Migrated",
                        convert_legacy_steps(legacy.referees, legacy.post_steps),
                        legacy.merge,
                    );
                    save_profile("migrated", &profile)?;
                    // Delete the source only after the durable destination is
                    // published. Unrecognized legacy JSON is left untouched
                    // for manual recovery.
                    let _ = fs::remove_file(&old_path);
                }
            }
        }

        // Migrate from old referees.json
        let old_referees = home.join(".pipeline").join("referees.json");
        if old_referees.exists() {
            if let Ok(content) = read_profile_file(&old_referees) {
                if let Ok(referees) = serde_json::from_str::<Vec<LegacyRefereeConfig>>(&content) {
                    let profile = ProfileData::new(
                        "Migrated",
                        convert_legacy_steps(referees, vec![]),
                        MergeConfig::default(),
                    );
                    let migrated_path = profiles.join("migrated.json");
                    if !migrated_path.exists() {
                        save_profile("migrated", &profile)?;
                        let _ = fs::remove_file(&old_referees);
                    }
                }
            }
        }

        // Also remove old default.json if present (from prior version)
        let old_default = profiles.join("default.json");
        if old_default.exists() {
            let _ = fs::remove_file(&old_default);
        }

        // Create Deep Review as the primary profile
        let profile = ProfileData::new(
            "Deep Review",
            {
                let mut s = default_steps();
                if let Some(step) = s.iter_mut().find(|s| s.id == "validate_feedback") {
                    step.enabled = true;
                }
                s
            },
            MergeConfig::default(),
        );
        let json =
            serde_json::to_string_pretty(&profile).map_err(|e| format!("Serialize error: {e}"))?;
        fs::write(&deep_review_path, json)
            .map_err(|e| format!("Failed to write deep-review profile: {e}"))?;
    } else if old_path.exists() {
        let _ = fs::remove_file(&old_path);
    }

    // Always ensure other built-in profiles exist
    create_builtin_profiles()?;

    Ok(())
}

// ── Profile I/O ─────────────────────────────────────────────────────

pub(crate) fn load_profile(id: &str) -> Result<ProfileData, String> {
    let path = profile_path(id)?;
    let content =
        read_profile_file(&path).map_err(|e| format!("Failed to read profile '{id}': {e}"))?;

    // Try the current format. `steps` may legitimately be empty; serde's
    // required current-format fields distinguish it from the legacy shape.
    if let Ok(profile) = serde_json::from_str::<ProfileData>(&content) {
        validate_profile_data(&profile).map_err(|e| format!("Profile '{id}' is invalid: {e}"))?;
        return Ok(profile);
    }

    // Try legacy format (has referees/post_steps arrays)
    if let Ok(legacy) = serde_json::from_str::<LegacyProfileData>(&content) {
        if !legacy.referees.is_empty() || !legacy.post_steps.is_empty() {
            let profile = ProfileData::new(
                legacy.name,
                convert_legacy_steps(legacy.referees, legacy.post_steps),
                legacy.merge,
            );
            // Write back in new format
            let _ = save_profile(id, &profile);
            return Ok(profile);
        }
    }

    Err(format!("Failed to parse profile '{id}'"))
}

pub fn save_profile(id: &str, profile: &ProfileData) -> Result<(), String> {
    let path = profile_path(id)?;
    validate_profile_data(profile)?;
    let json =
        serde_json::to_string_pretty(profile).map_err(|e| format!("Failed to serialize: {e}"))?;
    restore_profile_bytes(&path, json.as_bytes())
        .map_err(|e| format!("Failed to save profile '{}': {e}", path.display()))
}

// ── Public API ──────────────────────────────────────────────────────

/// Load the active profile as a PipelineConfig.
pub fn load() -> PipelineConfig {
    let _ = ensure_migrated();
    let settings = crate::settings::load_persisted();
    match load_profile(&settings.active_profile) {
        Ok(profile) => profile.into(),
        Err(e) => {
            // Always warn when a profile fails to load so the user knows
            // they're running on defaults rather than their saved config.
            let file_exists = profile_path(&settings.active_profile)
                .map(|p| p.exists())
                .unwrap_or(false);
            if file_exists {
                eprintln!(
                    "WARNING: Profile '{}' exists but failed to load: {e}. \
                     The file may be corrupt. Using default pipeline.",
                    settings.active_profile
                );
            } else {
                eprintln!(
                    "WARNING: Profile '{}' not found: {e}. Using default pipeline.",
                    settings.active_profile
                );
            }
            defaults()
        }
    }
}

/// Load the executable config and its display name from one profile-file
/// snapshot. Run manifests use the returned name so a concurrent profile edit
/// cannot make their metadata disagree with the workflow that actually ran.
pub fn load_required_profile_for(active_profile: &str) -> Result<(PipelineConfig, String), String> {
    ensure_migrated()?;
    let profile = load_profile(active_profile).map_err(|error| {
        format!(
            "Active profile '{}' could not be loaded: {error}. Select or repair a profile before running.",
            active_profile
        )
    })?;
    let profile_name = profile.name.clone();
    Ok((profile.into(), profile_name))
}

/// Step ids key the executor's pass events and the merge grouping
/// (`{id}/{agent}`), so duplicates silently collide. Reject them at save and
/// import time. Deliberately not enforced on load/migration, so an existing
/// profile with duplicates can still be opened and repaired in the editor.
pub fn validate_unique_step_ids(steps: &[StepConfig]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for step in steps {
        if step.id.is_empty()
            || step.id.len() > 64
            || !step
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(format!(
                "Invalid step id '{}'. Use 1–64 ASCII letters, numbers, '.', '-', or '_'.",
                step.id
            ));
        }
        if !seen.insert(step.id.as_str()) {
            return Err(format!(
                "Duplicate step id '{}' — step ids must be unique",
                step.id
            ));
        }
    }
    Ok(())
}

pub fn validate_workflow_semantics(steps: &[StepConfig]) -> Result<(), String> {
    for step in steps {
        if step.phase == Phase::Sequential && step.agents.len() > 1 {
            return Err(format!(
                "Sequential step '{}' selects multiple agents, but sequential multi-agent execution is unsupported.",
                step.id
            ));
        }
        if step.phase == Phase::Sequential && step.for_each.is_some() {
            return Err(format!(
                "Sequential step '{}' uses fan-out, but sequential fan-out is unsupported.",
                step.id
            ));
        }
    }
    Ok(())
}

fn validate_profile_steps(steps: &[StepConfig]) -> Result<(), String> {
    if steps.len() > MAX_PROFILE_STEPS {
        return Err(format!(
            "Profile has {} steps; the safety limit is {MAX_PROFILE_STEPS}",
            steps.len()
        ));
    }
    validate_unique_step_ids(steps)?;
    validate_dependencies(steps)?;
    validate_workflow_semantics(steps)?;
    for step in steps {
        if step.label.chars().count() > MAX_STEP_LABEL_CHARS {
            return Err(format!(
                "Step '{}' label exceeds {MAX_STEP_LABEL_CHARS} characters",
                step.id
            ));
        }
        if step.prompt.len() > MAX_STEP_PROMPT_BYTES {
            return Err(format!(
                "Step '{}' prompt exceeds the {} MB safety limit",
                step.id,
                MAX_STEP_PROMPT_BYTES / 1024 / 1024
            ));
        }
        if step.tools.len() > ALLOWED_TOOLS.len() {
            return Err(format!("Step '{}' declares too many tools", step.id));
        }
        let mut tools = std::collections::HashSet::new();
        for tool in &step.tools {
            if !ALLOWED_TOOLS.contains(&tool.as_str()) {
                return Err(format!(
                    "Step '{}' requests unsupported tool '{}'. Allowed tools: {}",
                    step.id,
                    tool,
                    ALLOWED_TOOLS.join(", ")
                ));
            }
            if !tools.insert(tool.as_str()) {
                return Err(format!(
                    "Step '{}' lists tool '{}' more than once",
                    step.id, tool
                ));
            }
        }
        if step.agents.len() > ALLOWED_AGENTS.len() {
            return Err(format!("Step '{}' declares too many agents", step.id));
        }
        let mut agents = std::collections::HashSet::new();
        for agent in &step.agents {
            if !ALLOWED_AGENTS.contains(&agent.as_str()) {
                return Err(format!(
                    "Step '{}' names unsupported agent '{}'. Allowed agents: {}",
                    step.id,
                    agent,
                    ALLOWED_AGENTS.join(", ")
                ));
            }
            if !agents.insert(agent.as_str()) {
                return Err(format!(
                    "Step '{}' lists agent '{}' more than once",
                    step.id, agent
                ));
            }
        }
        if let Some(for_each) = &step.for_each {
            if for_each.glob.trim().is_empty() || for_each.glob.len() > 1024 {
                return Err(format!(
                    "Step '{}' fan-out glob must contain 1–1024 bytes",
                    step.id
                ));
            }
            if !(1..=MAX_FAN_OUT_ITEMS).contains(&for_each.max) {
                return Err(format!(
                    "Step '{}' fan-out maximum must be between 1 and {MAX_FAN_OUT_ITEMS}",
                    step.id
                ));
            }
        }
        if let Some(schema) = &step.output_schema {
            let bytes = serde_json::to_vec(schema)
                .map_err(|e| format!("Step '{}' output schema is invalid: {e}", step.id))?;
            if bytes.len() > MAX_OUTPUT_SCHEMA_BYTES {
                return Err(format!(
                    "Step '{}' output schema exceeds the {} MB safety limit",
                    step.id,
                    MAX_OUTPUT_SCHEMA_BYTES / 1024 / 1024
                ));
            }
        }
    }
    Ok(())
}

fn validate_profile_data(profile: &ProfileData) -> Result<(), String> {
    if profile.name.trim().is_empty() || profile.name.chars().count() > MAX_PROFILE_NAME_CHARS {
        return Err(format!(
            "Profile name must contain 1–{MAX_PROFILE_NAME_CHARS} characters"
        ));
    }
    validate_profile_steps(&profile.steps)?;

    for (label, value) in [
        ("orientation prompt", profile.orientation_prompt.as_str()),
        (
            "parallel context template",
            profile.parallel_context_template.as_str(),
        ),
        ("merge prompt", profile.merge.prompt.as_str()),
    ] {
        if value.len() > MAX_PROFILE_TEXT_BYTES {
            return Err(format!(
                "Profile {label} exceeds the {} MB safety limit",
                MAX_PROFILE_TEXT_BYTES / 1024 / 1024
            ));
        }
    }
    if profile.merge.agents.len() > 1 {
        return Err("Merge supports at most one selected agent".to_string());
    }
    for agent in &profile.merge.agents {
        if !ALLOWED_AGENTS.contains(&agent.as_str()) {
            return Err(format!(
                "Merge names unsupported agent '{}'. Allowed agents: {}",
                agent,
                ALLOWED_AGENTS.join(", ")
            ));
        }
    }

    if !matches!(
        profile.extraction.method.as_str(),
        "" | "auto" | "llm" | "marker" | "pdftotext"
    ) {
        return Err(format!(
            "Invalid profile extraction method '{}'",
            profile.extraction.method
        ));
    }
    if !matches!(
        profile.extraction.input_mode.as_str(),
        "" | "document" | "folder" | "none"
    ) {
        return Err(format!(
            "Invalid profile input mode '{}'",
            profile.extraction.input_mode
        ));
    }

    let valid_key = |key: &str| {
        !key.is_empty()
            && key.len() <= 64
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let mut variable_keys = std::collections::HashSet::new();
    if profile.variables.len() > MAX_VARIABLES {
        return Err(format!(
            "Profile has too many variables (maximum {MAX_VARIABLES})"
        ));
    }
    for variable in &profile.variables {
        if !valid_key(&variable.key) {
            return Err(format!(
                "Invalid variable key '{}'. Use 1–64 ASCII letters, numbers, or underscores.",
                variable.key
            ));
        }
        if !variable_keys.insert(variable.key.as_str()) {
            return Err(format!("Duplicate variable key '{}'", variable.key));
        }
        if !matches!(variable.kind.as_str(), "text" | "choice" | "file") {
            return Err(format!(
                "Invalid kind '{}' for variable '{}'",
                variable.kind, variable.key
            ));
        }
    }

    let mut input_keys = std::collections::HashSet::new();
    if profile.extraction.extra_inputs.len() > MAX_EXTRA_INPUTS {
        return Err(format!(
            "Profile has too many named inputs (maximum {MAX_EXTRA_INPUTS})"
        ));
    }
    for input in &profile.extraction.extra_inputs {
        if !valid_key(&input.key) {
            return Err(format!(
                "Invalid input key '{}'. Use 1–64 ASCII letters, numbers, or underscores.",
                input.key
            ));
        }
        if !input_keys.insert(input.key.as_str()) {
            return Err(format!("Duplicate input key '{}'", input.key));
        }
        if !matches!(input.mode.as_str(), "document" | "folder") {
            return Err(format!(
                "Invalid mode '{}' for input '{}'",
                input.mode, input.key
            ));
        }
    }
    Ok(())
}

/// Reject explicit `inputs` graphs that can't run: an unknown dependency id or
/// a dependency cycle. Only enabled steps participate (a disabled upstream is
/// simply ignored by the executor, so it isn't an error here). Steps with no
/// explicit `inputs` use the implicit adjacency schedule and can't form cycles.
pub fn validate_dependencies(steps: &[StepConfig]) -> Result<(), String> {
    use std::collections::{HashMap, HashSet};
    let ids: HashSet<&str> = steps.iter().map(|s| s.id.as_str()).collect();

    // Unknown / self dependencies.
    for s in steps {
        for dep in &s.inputs {
            if dep == &s.id {
                return Err(format!("Step '{}' lists itself as a dependency.", s.id));
            }
            if !ids.contains(dep.as_str()) {
                return Err(format!(
                    "Step '{}' depends on unknown step '{}'.",
                    s.id, dep
                ));
            }
        }
    }

    // Cycle detection over the explicit-inputs graph (DFS with a colour map).
    let graph: HashMap<&str, Vec<&str>> = steps
        .iter()
        .map(|s| (s.id.as_str(), s.inputs.iter().map(|d| d.as_str()).collect()))
        .collect();
    #[derive(PartialEq, Clone, Copy)]
    enum Mark {
        Visiting,
        Done,
    }
    let mut marks: HashMap<&str, Mark> = HashMap::new();
    // Iterative DFS so deep graphs can't blow the stack.
    for start in graph.keys().copied() {
        if marks.contains_key(start) {
            continue;
        }
        let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
        marks.insert(start, Mark::Visiting);
        while let Some((node, idx)) = stack.last().copied() {
            let neighbours = graph.get(node).map(|v| v.as_slice()).unwrap_or(&[]);
            if idx < neighbours.len() {
                stack.last_mut().unwrap().1 += 1;
                let next = neighbours[idx];
                match marks.get(next) {
                    Some(Mark::Visiting) => {
                        return Err(format!(
                            "Step dependencies form a cycle involving '{next}'."
                        ));
                    }
                    Some(Mark::Done) => {}
                    None => {
                        marks.insert(next, Mark::Visiting);
                        stack.push((next, 0));
                    }
                }
            } else {
                marks.insert(node, Mark::Done);
                stack.pop();
            }
        }
    }
    Ok(())
}

/// Save to the active profile.
pub fn save(config: &PipelineConfig) -> Result<(), String> {
    let _ = ensure_migrated();
    let settings = crate::settings::load_persisted_required()?;
    save_for(&settings.active_profile, config)
}

pub fn save_for(profile_id: &str, config: &PipelineConfig) -> Result<(), String> {
    validate_profile_id(profile_id)?;
    validate_profile_steps(&config.steps)?;
    let name = load_profile(profile_id)?.name;
    let profile = ProfileData::from_config(name, config);
    save_profile(profile_id, &profile)
}

/// Reset active profile to defaults.
pub fn reset_defaults() -> PipelineConfig {
    let d = defaults();
    let _ = save(&d);
    d
}

// ── Profile management ──────────────────────────────────────────────

pub fn get_active_profile_id() -> String {
    crate::settings::load_persisted().active_profile
}

pub fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    let _ = ensure_migrated();
    let dir = profiles_dir()?;
    let mut summaries = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("Failed to read profiles dir: {e}"))? {
        let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if let Ok(profile) = load_profile(&id) {
            summaries.push(profile_summary(id, &profile));
        }
    }
    summaries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(summaries)
}

pub fn create_profile(name: &str) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let id = slugify(name);
    if id.is_empty() {
        return Err("Profile name produces empty ID".into());
    }
    let path = profile_path(&id)?;
    if path.exists() {
        return Err(format!("A profile with ID '{id}' already exists"));
    }
    // New profiles start domain-neutral: generic starter steps, generic
    // context template, and an explicit generic survey prompt (empty would
    // fall back to the paper survey at runtime).
    let profile = generic_profile(name, generic_starter_steps(), ExtractionConfig::default());
    save_profile(&id, &profile)?;
    Ok(profile_summary(id, &profile))
}

pub fn duplicate_profile(source_id: &str, new_name: &str) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let source = load_profile(source_id)?;
    let new_id = slugify(new_name);
    if new_id.is_empty() {
        return Err("Profile name produces empty ID".into());
    }
    let path = profile_path(&new_id)?;
    if path.exists() {
        return Err(format!("A profile with ID '{new_id}' already exists"));
    }
    let profile = duplicate_profile_data(source, new_name);
    save_profile(&new_id, &profile)?;
    Ok(profile_summary(new_id, &profile))
}

fn duplicate_profile_data(mut source: ProfileData, new_name: &str) -> ProfileData {
    source.name = new_name.to_string();
    source
}

pub fn rename_profile(id: &str, new_name: &str) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let mut profile = load_profile(id)?;
    profile.name = new_name.to_string();
    save_profile(id, &profile)?;
    Ok(profile_summary(id.to_string(), &profile))
}

pub fn delete_profile(id: &str) -> Result<(), String> {
    if BUILTIN_PROFILES.contains(&id) {
        return Err(format!("Cannot delete the built-in profile '{id}'"));
    }
    let path = profile_path(id)?;
    if !path.exists() {
        return Err(format!("Profile '{id}' does not exist"));
    }
    delete_profile_file_transactionally(&path, || {
        // If this was the active profile, switch back to deep-review. The
        // profile file remains recoverable until this settings write commits.
        crate::settings::replace_active_profile_if(id, "deep-review").map(|_| ())
    })
}

fn delete_profile_file_transactionally(
    path: &Path,
    update_references: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Profile path has no valid file name")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let tombstone = path.with_file_name(format!(
        ".{file_name}.deleting-{}-{nonce}",
        std::process::id()
    ));
    fs::rename(path, &tombstone).map_err(|e| format!("Failed to stage profile deletion: {e}"))?;

    if let Err(error) = update_references() {
        return match fs::rename(&tombstone, path) {
            Ok(()) => Err(error),
            Err(rollback) => Err(format!(
                "{error}. The profile reference update failed and restoring {} also failed: {rollback}. Recover the profile from {}.",
                path.display(),
                tombstone.display()
            )),
        };
    }

    // Reference updates have committed. Failure to unlink the hidden
    // tombstone must not make the UI retry an already-committed deletion; keep
    // it as a recoverable backup and report the cleanup problem to diagnostics.
    if let Err(error) = fs::remove_file(&tombstone) {
        eprintln!(
            "WARNING: profile deletion committed but temporary backup {} could not be removed: {error}",
            tombstone.display()
        );
    }
    Ok(())
}

pub fn switch_profile(id: &str) -> Result<PipelineConfig, String> {
    let _ = ensure_migrated();
    let profile = load_profile(id)?;
    crate::settings::set_active_profile(id)?;
    Ok(profile.into())
}

// ── Export/Import ───────────────────────────────────────────────────

pub fn export_profile_data(id: &str) -> Result<String, String> {
    let profile = load_profile(id)?;
    let envelope = ExportEnvelope::Profile {
        schema_version: CURRENT_SCHEMA_VERSION,
        name: profile.name,
        steps: profile.steps,
        merge: profile.merge,
        use_orientation: profile.use_orientation,
        orientation_prompt: profile.orientation_prompt,
        extraction: profile.extraction,
        parallel_context_template: profile.parallel_context_template,
        variables: profile.variables,
    };
    serde_json::to_string_pretty(&envelope).map_err(|e| format!("Serialize error: {e}"))
}

pub fn export_bundle() -> Result<String, String> {
    let _ = ensure_migrated();
    let mut settings = crate::settings::load_persisted_required()?;
    // Strip API keys from the export to prevent credential leakage
    settings.anthropic_api_key = String::new();
    settings.openai_api_key = String::new();
    settings.google_api_key = String::new();
    settings.local_api_key = String::new();
    let summaries = list_profiles()?;
    let mut profiles = Vec::new();
    for s in &summaries {
        let profile = load_profile(&s.id)?;
        profiles.push(ProfileExport::from_profile(s.id.clone(), profile));
    }
    let envelope = ExportEnvelope::Bundle {
        active_profile: settings.active_profile.clone(),
        settings,
        profiles,
    };
    serde_json::to_string_pretty(&envelope).map_err(|e| format!("Serialize error: {e}"))
}

/// Auto-detect and parse an export file, handling legacy formats.
pub fn import_envelope(json: &str) -> Result<ExportEnvelope, String> {
    // Try new format first
    if let Ok(envelope) = serde_json::from_str::<ExportEnvelope>(json) {
        return Ok(envelope);
    }

    // Parse as JSON value for legacy detection
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {e}"))?;

    match value.get("type").and_then(|t| t.as_str()) {
        Some("referee") => {
            if let Some(data) = value.get("data") {
                let legacy: LegacyRefereeConfig = serde_json::from_value(data.clone())
                    .map_err(|e| format!("Invalid referee config: {e}"))?;
                return Ok(ExportEnvelope::Step {
                    data: referee_to_step(legacy),
                });
            }
        }
        Some("post_step") => {
            if let Some(data) = value.get("data") {
                let legacy: LegacyPostStepConfig = serde_json::from_value(data.clone())
                    .map_err(|e| format!("Invalid post-step config: {e}"))?;
                return Ok(ExportEnvelope::Step {
                    data: post_step_to_step(legacy),
                });
            }
        }
        Some("profile") if value.get("referees").is_some() => {
            // Legacy profile with separate referees/post_steps
            let name = value["name"].as_str().unwrap_or("Imported").to_string();
            let referees: Vec<LegacyRefereeConfig> =
                serde_json::from_value(value["referees"].clone()).unwrap_or_default();
            let post_steps: Vec<LegacyPostStepConfig> =
                serde_json::from_value(value["post_steps"].clone()).unwrap_or_default();
            let merge: MergeConfig = value
                .get("merge")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            return Ok(ExportEnvelope::Profile {
                schema_version: 1,
                name,
                steps: convert_legacy_steps(referees, post_steps),
                merge,
                use_orientation: true,
                orientation_prompt: String::new(),
                extraction: ExtractionConfig::default(),
                parallel_context_template: default_parallel_template(),
                variables: Vec::new(),
            });
        }
        _ => {}
    }

    // Try bare legacy PipelineConfig (oldest format: top-level referees/post_steps)
    if value.get("referees").is_some() {
        let referees: Vec<LegacyRefereeConfig> =
            serde_json::from_value(value["referees"].clone()).unwrap_or_default();
        let post_steps: Vec<LegacyPostStepConfig> = value
            .get("post_steps")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let merge: MergeConfig = value
            .get("merge")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        return Ok(ExportEnvelope::Profile {
            schema_version: 1,
            name: "Imported".into(),
            steps: convert_legacy_steps(referees, post_steps),
            merge,
            use_orientation: true,
            orientation_prompt: String::new(),
            extraction: ExtractionConfig::default(),
            parallel_context_template: default_parallel_template(),
            variables: Vec::new(),
        });
    }

    Err("Unrecognized file format".into())
}

#[allow(clippy::too_many_arguments)]
pub fn import_profile_data(
    name: &str,
    steps: Vec<StepConfig>,
    merge: MergeConfig,
    use_orientation: bool,
    orientation_prompt: String,
    extraction: ExtractionConfig,
    parallel_context_template: String,
    variables: Vec<VarSpec>,
) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    validate_unique_step_ids(&steps)?;
    validate_dependencies(&steps)?;
    let mut id = slugify(name);
    if id.is_empty() {
        id = format!("imported-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    }
    // Deduplicate ID if it already exists
    let base_id = id.clone();
    let mut counter = 1u32;
    while profile_path(&id)?.exists() {
        id = format!("{base_id}-{counter}");
        counter += 1;
    }
    let mut profile = ProfileData::new(name, steps, merge);
    profile.use_orientation = use_orientation;
    profile.orientation_prompt = orientation_prompt;
    profile.extraction = extraction;
    profile.parallel_context_template = parallel_context_template;
    profile.variables = variables;
    save_profile(&id, &profile)?;
    Ok(profile_summary(id, &profile))
}

pub fn import_bundle(json: &str) -> Result<(), String> {
    let envelope: ExportEnvelope =
        serde_json::from_str(json).map_err(|e| format!("Invalid bundle: {e}"))?;
    match envelope {
        ExportEnvelope::Bundle {
            settings: imported_settings,
            profiles,
            active_profile,
        } => {
            let validated = validate_bundle_profiles(&profiles, &active_profile)?;
            imported_settings.validate()?;
            if imported_settings.active_profile != active_profile {
                return Err(format!(
                    "Bundle active profile mismatch: settings name '{}', envelope names '{}'",
                    imported_settings.active_profile, active_profile
                ));
            }
            // Merge imported settings with existing, preserving local API keys.
            let mut current = crate::settings::load_persisted_required()?;
            current.preferred_provider = imported_settings.preferred_provider;
            current.max_workers = imported_settings.max_workers;
            current.claude_model = imported_settings.claude_model;
            current.claude_cli_model_selection = imported_settings.claude_cli_model_selection;
            current.claude_api_model_selection = imported_settings.claude_api_model_selection;
            current.claude_effort = imported_settings.claude_effort;
            current.codex_model = imported_settings.codex_model;
            current.codex_cli_model_selection = imported_settings.codex_cli_model_selection;
            current.codex_api_model_selection = imported_settings.codex_api_model_selection;
            current.codex_effort = imported_settings.codex_effort;
            current.gemini_model = imported_settings.gemini_model;
            current.gemini_cli_model_selection = imported_settings.gemini_cli_model_selection;
            current.gemini_api_model_selection = imported_settings.gemini_api_model_selection;
            if current.local_base_url != imported_settings.local_base_url {
                // A bearer token is scoped to its endpoint. Carrying a local
                // token across an imported server URL can disclose it to a
                // different host on the next request.
                current.local_api_key.clear();
            }
            current.local_base_url = imported_settings.local_base_url;
            current.local_model = imported_settings.local_model;
            current.pdf_extractor = imported_settings.pdf_extractor;
            current.marker_disable_ocr = imported_settings.marker_disable_ocr;
            current.marker_disable_images = imported_settings.marker_disable_images;
            current.verbose_logging = imported_settings.verbose_logging;
            current.step_timeout_secs = imported_settings.step_timeout_secs;
            current.max_retries = imported_settings.max_retries;
            current.max_saved_runs = imported_settings.max_saved_runs;
            current.active_profile = active_profile;

            // Snapshot every destination before the first mutation. If any
            // profile or the final settings write fails, restore the exact
            // previous bytes (or remove a newly-created file).
            let snapshots: Vec<(PathBuf, Option<Vec<u8>>)> = validated
                .iter()
                .map(|(id, _)| {
                    let path = profile_path(id)?;
                    let prior = match fs::read(&path) {
                        Ok(bytes) => Some(bytes),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                        Err(error) => {
                            return Err(format!(
                                "Cannot snapshot existing profile '{}': {error}",
                                path.display()
                            ));
                        }
                    };
                    Ok((path, prior))
                })
                .collect::<Result<_, String>>()?;
            for (id, profile) in &validated {
                if let Err(error) = save_profile(id, profile) {
                    let rollback = restore_profile_snapshots(&snapshots);
                    return Err(match rollback {
                        Ok(()) => error,
                        Err(rollback_error) => {
                            format!("{error}; rollback also failed: {rollback_error}")
                        }
                    });
                }
            }
            // API keys are intentionally NOT overwritten from the import
            if let Err(error) = crate::settings::save(&current) {
                let rollback = restore_profile_snapshots(&snapshots);
                return Err(match rollback {
                    Ok(()) => error,
                    Err(rollback_error) => {
                        format!("{error}; rollback also failed: {rollback_error}")
                    }
                });
            }
            Ok(())
        }
        _ => Err("Expected a bundle export file".into()),
    }
}

fn validate_bundle_profiles(
    profiles: &[ProfileExport],
    active_profile: &str,
) -> Result<Vec<(String, ProfileData)>, String> {
    let mut ids = std::collections::HashSet::new();
    let mut validated = Vec::with_capacity(profiles.len());
    for profile in profiles {
        validate_profile_id(&profile.id)?;
        if !ids.insert(profile.id.clone()) {
            return Err(format!("Duplicate profile id '{}'", profile.id));
        }
        let data = profile.to_profile_data();
        validate_profile_data(&data).map_err(|e| format!("Profile '{}': {e}", profile.name))?;
        validated.push((profile.id.clone(), data));
    }
    if !ids.contains(active_profile) {
        return Err(format!(
            "Active profile '{active_profile}' is not present in the bundle"
        ));
    }
    Ok(validated)
}

fn restore_profile_snapshots(snapshots: &[(PathBuf, Option<Vec<u8>>)]) -> Result<(), String> {
    let mut errors = Vec::new();
    for (path, prior) in snapshots {
        let result = match prior {
            Some(bytes) => restore_profile_bytes(path, bytes),
            None => {
                if path.exists() {
                    fs::remove_file(path)
                } else {
                    Ok(())
                }
            }
        };
        if let Err(error) = result {
            errors.push(format!("{}: {error}", path.display()));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn restore_profile_bytes(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "profile has no parent")
    })?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
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
            agents: vec!["claude".to_string(), "gemini".to_string()],
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

    // ── validate_dependencies ──────────────────────────────────────

    fn step_dep(id: &str, deps: &[&str]) -> StepConfig {
        StepConfig {
            id: id.to_string(),
            label: id.to_string(),
            inputs: deps.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn deps_no_explicit_inputs_always_valid() {
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

    fn bundle_profile(id: &str, steps: Vec<StepConfig>) -> ProfileExport {
        ProfileExport {
            id: id.into(),
            name: id.into(),
            steps,
            merge: MergeConfig::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
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

        let valid = vec![bundle_profile("one", Vec::new())];
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
    fn profile_deletion_rolls_back_when_reference_update_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom.json");
        fs::write(&path, "profile bytes").unwrap();

        let error = delete_profile_file_transactionally(&path, || Err("settings failed".into()))
            .unwrap_err();
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

        step.tools = vec!["Read".to_string()];
        step.agents = vec!["unknown-provider".to_string()];
        profile.steps = vec![step];
        assert!(validate_profile_data(&profile)
            .unwrap_err()
            .contains("unsupported agent 'unknown-provider'"));
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
}

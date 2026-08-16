//! Pipeline configuration: profiles, steps, and post-processing.
//! Profiles stored at ~/.pipeline/profiles/{id}.json.
//! Active profile tracked via settings.active_profile.

use crate::prompts;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

struct ProfileMutationGuard {
    _process: std::sync::MutexGuard<'static, ()>,
    file: fs::File,
}

impl Drop for ProfileMutationGuard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

fn lock_profile_mutations() -> Result<ProfileMutationGuard, String> {
    static PROCESS_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    let process = PROCESS_LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let profiles = profiles_dir()?;
    let root = profiles
        .parent()
        .ok_or("Profiles directory has no parent")?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("profiles.lock"))
        .map_err(|error| format!("Failed to open profile lock: {error}"))?;
    fs2::FileExt::lock_exclusive(&file)
        .map_err(|error| format!("Failed to lock profiles: {error}"))?;
    Ok(ProfileMutationGuard {
        _process: process,
        file,
    })
}

const MAX_PROFILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PROFILE_STEPS: usize = 100;
pub const MAX_STEP_PROMPT_BYTES: usize = 1024 * 1024;
pub const MAX_FAN_OUT_ITEMS: u32 = 100;
const MAX_PROFILE_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_STEP_LABEL_CHARS: usize = 200;
const MAX_PROFILE_NAME_CHARS: usize = 200;
const MAX_VARIABLES: usize = 100;
const MAX_EXTRA_INPUTS: usize = 100;
pub(crate) const MAX_OUTPUT_SCHEMA_BYTES: usize = 1024 * 1024;
const MAX_RUN_IF_PATTERN_BYTES: usize = 16 * 1024;
const MAX_JSON_POINTER_BYTES: usize = 4 * 1024;
const ALLOWED_TOOLS: &[&str] = &["WebSearch"];
const ALLOWED_AGENTS: &[&str] = &["claude", "codex", "antigravity", "local"];

fn read_profile_file(path: &Path) -> Result<String, String> {
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(path)
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

/// Which durable representation of the primary input a step may use.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PrimaryArtifactPart {
    /// Human/LLM-readable `document.md`.
    Text,
    /// Canonical DocumentBundle JSON and block projection.
    Structure,
    /// Page renders and extracted/source-native visual assets.
    Visuals,
    /// The original file or folder tree selected by the user.
    Source,
}

/// Which representation of a named extra input a step may use.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NamedInputArtifactPart {
    Text,
    Source,
}

/// Which products of an upstream step a downstream step may use.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum StepArtifactPart {
    /// The producer's terminal report.
    Report,
    /// Supporting files written below the producer-owned `files/` directory.
    Files,
}

/// A logical artifact selection. Profiles name producers and artifact roles,
/// never implementation paths inside a run directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtifactSelector {
    Primary {
        parts: Vec<PrimaryArtifactPart>,
    },
    Survey,
    NamedInput {
        key: String,
        parts: Vec<NamedInputArtifactPart>,
    },
    Step {
        step: String,
        parts: Vec<StepArtifactPart>,
        /// Optional glob relative to the producer's supporting-files root.
        /// Empty means all supporting files.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        glob: String,
    },
}

/// The complete readable context for one step. The allow-list is explicit:
/// an empty list is an intentionally isolated step.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepContext {
    #[serde(default)]
    pub include: Vec<ArtifactSelector>,
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
    /// Order-only dependencies. Artifact selectors that name an upstream step
    /// add their own data dependency automatically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    /// Exact artifacts made available to this step.
    #[serde(default)]
    pub context: StepContext,
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
        /// When set, the pointed-to value must be an array containing this
        /// exact JSON value. This keeps list-based routing declarative without
        /// introducing a general expression language.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        contains: Option<serde_json::Value>,
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
            after: Vec::new(),
            context: StepContext::default(),
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

/// Materialize role-level Settings defaults into only those workflow steps
/// that deliberately leave their agent list empty. Explicit workflow agents
/// and their per-step model policies always win.
pub fn apply_agent_defaults(config: &mut PipelineConfig, settings: &crate::settings::Settings) {
    for step in &mut config.steps {
        if !step.agents.is_empty() {
            continue;
        }
        match step.phase {
            Phase::Parallel => {
                step.agents = settings.parallel_agents();
                step.model_overrides
                    .extend(settings.default_parallel_model_overrides.clone());
                step.effort_overrides
                    .extend(settings.default_parallel_effort_overrides.clone());
            }
            Phase::Sequential => {
                step.agents = vec![settings.sequential_agent().to_string()];
                step.model_overrides
                    .extend(settings.default_sequential_model_overrides.clone());
                step.effort_overrides
                    .extend(settings.default_sequential_effort_overrides.clone());
            }
        }
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
    /// Which provider runs the merge calls. Empty = Merge default in Settings.
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

/// Per-profile extraction configuration. Workflows may choose a method, but
/// parser-specific tuning is centralized in global Settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// "auto" | "llm" | "paddleocr-vl-full" | "pdftotext" | "" (= inherit
    /// global). The retired "paddleocr-vl" value is migrated to the Full
    /// Parser when older profiles are deserialized.
    #[serde(default, deserialize_with = "deserialize_extraction_method")]
    pub method: String,
    /// Input mode for the workflow: "" or "document" (single file, default),
    /// "folder" (inventory of a directory; steps Read files on demand), or
    /// "none" (runs from the prompts alone).
    #[serde(default)]
    pub input_mode: String,
    /// Extra named inputs (beyond the primary one) this profile accepts.
    #[serde(default)]
    pub extra_inputs: Vec<InputSlot>,
}

fn deserialize_extraction_method<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(match value.as_str() {
        "paddleocr-vl" => "paddleocr-vl-full".to_string(),
        _ => value,
    })
}

/// Explicit public products selected from a workflow's step graph. Empty
/// fields preserve legacy inference for profiles saved before this contract.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputConfig {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub primary_step: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub findings_step: String,
}

/// Combined config returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub steps: Vec<StepConfig>,
    #[serde(default)]
    pub merge: MergeConfig,
    #[serde(default)]
    pub outputs: OutputConfig,
    /// Optional run-local shared-context caching. When enabled, the executor
    /// prepares the extracted input and orientation map once, then lets each
    /// provider use its native prefix cache or forkable CLI sessions.
    #[serde(default)]
    pub context_cache: ContextCacheConfig,
    /// Compatibility field retained for existing profile files and exports.
    /// Orientation is required; an older explicit `false` is normalized to
    /// `true` while deserializing.
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_required_orientation"
    )]
    pub use_orientation: bool,
    /// Per-profile orientation prompt. Empty = use the default template loaded
    /// from prompts/orientation.md (or the user override at
    /// ~/.pipeline/prompts/orientation.md). The placeholder `{paper_text}` is
    /// substituted with the extracted paper at runtime.
    #[serde(default)]
    pub orientation_prompt: String,
    /// JSON-shape contract for the orientation call. Stock workflows store
    /// their full contract here; `None` is reserved for custom prompts that
    /// intentionally accept Pipeline's minimal object-root contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation_schema: Option<serde_json::Value>,
    /// Per-profile extraction overrides. When unset (default), the global
    /// Settings values are used.
    #[serde(default)]
    pub extraction: ExtractionConfig,
    /// Template wrapping each parallel step's prompt. Placeholders:
    ///   {step_prompt}  — the step's own instructions
    ///   {paper_type}   — theory / empirical / mixed
    ///   {orientation}  — orientation map reference (empty if not selected)
    ///   {paper_path}   — path to extracted paper text
    ///   {document_bundle} — path to canonical DocumentBundle JSON
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
    #[serde(default)]
    pub outputs: OutputConfig,
    #[serde(default)]
    pub context_cache: ContextCacheConfig,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_required_orientation"
    )]
    pub use_orientation: bool,
    #[serde(default)]
    pub orientation_prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation_schema: Option<serde_json::Value>,
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
            outputs: OutputConfig::default(),
            context_cache: ContextCacheConfig::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
            orientation_schema: Some(crate::orientation_contract::paper_schema()),
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
            outputs: config.outputs.clone(),
            context_cache: config.context_cache.clone(),
            use_orientation: config.use_orientation,
            orientation_prompt: config.orientation_prompt.clone(),
            orientation_schema: config.orientation_schema.clone(),
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
            outputs: profile.outputs,
            context_cache: profile.context_cache,
            use_orientation: profile.use_orientation,
            orientation_prompt: profile.orientation_prompt,
            orientation_schema: profile.orientation_schema,
            extraction: profile.extraction,
            parallel_context_template: profile.parallel_context_template,
            variables: profile.variables,
        }
    }
}

fn default_true() -> bool {
    true
}

fn deserialize_required_orientation<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Consume the legacy value so old files remain readable, but do not let a
    // profile opt out of the required preprocessing stage.
    let _ = bool::deserialize(deserializer)?;
    Ok(true)
}

/// Shared-context caching is deliberately a small, provider-neutral profile
/// setting. The backend selects the safest native mechanism for each call.
/// Defaulting to disabled preserves the behavior and cost profile of existing
/// profiles until the user opts in.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextCacheConfig {
    #[serde(default)]
    pub enabled: bool,
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

/// Current profile/export schema version. v2 introduced the generalized
/// engine; v3 added provider/model policy metadata; v4 adds optional shared
/// context caching; v5 centralizes parser tuning in global Settings; v6
/// replaces implicit step inputs with explicit artifact context and order
/// dependencies; v7 adds validated orientation schemas and array-membership
/// survey conditions; v8 adds explicit published run products. v1
/// (unversioned) profiles read fine because every added field is
/// `#[serde(default)]`; exports are tagged so future format changes can
/// migrate or reject gracefully.
pub const CURRENT_SCHEMA_VERSION: u32 = 8;

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
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
        #[serde(default)]
        outputs: OutputConfig,
        #[serde(default)]
        context_cache: ContextCacheConfig,
        #[serde(
            default = "default_true",
            deserialize_with = "deserialize_required_orientation"
        )]
        use_orientation: bool,
        #[serde(default)]
        orientation_prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        orientation_schema: Option<serde_json::Value>,
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
    #[serde(default)]
    pub outputs: OutputConfig,
    #[serde(default)]
    pub context_cache: ContextCacheConfig,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_required_orientation"
    )]
    pub use_orientation: bool,
    #[serde(default)]
    pub orientation_prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation_schema: Option<serde_json::Value>,
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
            outputs: profile.outputs,
            context_cache: profile.context_cache,
            use_orientation: profile.use_orientation,
            orientation_prompt: profile.orientation_prompt,
            orientation_schema: profile.orientation_schema,
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
            outputs: self.outputs.clone(),
            context_cache: self.context_cache.clone(),
            use_orientation: self.use_orientation,
            orientation_prompt: self.orientation_prompt.clone(),
            orientation_schema: self.orientation_schema.clone(),
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

mod builtins;
mod migrations;
mod persistence;
mod profiles;
mod storage;
mod validation;
mod workflow;

pub(crate) use persistence::load_profile;
pub use persistence::{load, load_required_profile_for, load_required_workflow_for, save_profile};
pub use profiles::{
    create_profile, delete_profile, duplicate_profile, export_bundle, export_profile_data,
    get_active_profile_id, import_bundle, import_envelope, import_profile_data,
    install_workflow_document, list_profiles, rename_profile, reset_defaults, save, save_for,
    switch_profile,
};
pub use storage::{sanitize_step_id, slugify};
pub(crate) use validation::{resolve_dependencies, validate_runtime_config};
pub use validation::{
    validate_dependencies, validate_enabled_sequential_step, validate_unique_step_ids,
    validate_workflow_semantics,
};
pub use workflow::{
    parse_workflow_document_strict, workflow_json_schema, workflow_template, WorkflowDocument,
};

use builtins::{
    auto_review_profile, builtin_primary_readers, configure_artifact_flow, create_builtin_profiles,
    defaults, full_review_profile, generic_profile, grant_review_profile, profile_summary,
    quick_auto_review_profile, BUILTIN_PROFILES, RETIRED_BUILTIN_PROFILES,
    V15_RETIRED_BUILTIN_PROFILES, V9_RETIRED_BUILTIN_PROFILES,
};
use migrations::ensure_migrated;
use persistence::save_profile_unlocked;
use profiles::restore_profile_bytes;
use storage::{profile_path, profiles_dir, validate_profile_id};
use validation::{validate_profile_data, validate_profile_steps};

#[cfg(test)]
use builtins::default_steps;
#[cfg(test)]
use migrations::{
    archive_retired_profile, compact_builtin_auto_review_prompts, migrate_builtin_catalog,
    migrate_review_quality_prompt_defaults, prompt_digest, refresh_builtin_auto_review_contracts,
};
#[cfg(test)]
use profiles::{
    delete_profile_file_transactionally, duplicate_profile_data, validate_bundle_profiles,
};
#[cfg(test)]
use validation::validate_run_conditions;

#[cfg(test)]
mod tests;

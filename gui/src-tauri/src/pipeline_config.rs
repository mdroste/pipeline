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
const MAX_OUTPUT_SCHEMA_BYTES: usize = 1024 * 1024;
const MAX_RUN_IF_PATTERN_BYTES: usize = 16 * 1024;
const MAX_JSON_POINTER_BYTES: usize = 4 * 1024;
const ALLOWED_TOOLS: &[&str] = &["WebSearch"];
const ALLOWED_AGENTS: &[&str] = &["claude", "codex", "gemini", "local"];

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

/// Per-profile extraction configuration. Workflows may choose a method, but
/// parser-specific tuning is centralized in global Settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// "auto" | "llm" | "paddleocr-vl-full" | "pdftotext" | "" (= inherit
    /// global). The retired "paddleocr-vl" value is migrated to the Full
    /// Parser when older profiles are deserialized.
    /// The retired "marker" value remains deserializable so users can repair
    /// profiles created before Pipeline 1.0.1; extraction rejects it.
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

/// Combined config returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub steps: Vec<StepConfig>,
    #[serde(default)]
    pub merge: MergeConfig,
    /// Optional run-local shared-context caching. When enabled, the executor
    /// prepares the extracted input and orientation map once, then lets each
    /// provider use its native prefix cache or forkable CLI sessions.
    #[serde(default)]
    pub context_cache: ContextCacheConfig,
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
    pub context_cache: ContextCacheConfig,
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
            context_cache: ContextCacheConfig::default(),
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
            context_cache: config.context_cache.clone(),
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
            context_cache: profile.context_cache,
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
/// dependencies. v1
/// (unversioned) profiles read fine because every added field is
/// `#[serde(default)]`; exports are tagged so future format changes can
/// migrate or reject gracefully.
pub const CURRENT_SCHEMA_VERSION: u32 = 6;

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
        context_cache: ContextCacheConfig,
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
    #[serde(default)]
    pub context_cache: ContextCacheConfig,
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
            context_cache: profile.context_cache,
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
            context_cache: self.context_cache.clone(),
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
/// Shared context reuse is part of the built-in workflow rather than the
/// provider-neutral `ProfileData::new` default: custom and legacy profiles
/// remain opt-in unless a catalog migration can identify an untouched stock
/// Full profile.
fn full_review_profile(validate_enabled: bool) -> ProfileData {
    let mut steps = default_steps();
    if let Some(step) = steps.iter_mut().find(|step| step.id == "validate_feedback") {
        step.enabled = validate_enabled;
    }
    let mut profile = ProfileData::new("Paper Review (Full)", steps, MergeConfig::default());
    profile.context_cache.enabled = true;
    profile
}

fn defaults() -> PipelineConfig {
    full_review_profile(false).into()
}

/// Profile IDs that cannot be deleted.
// All are recreated by create_builtin_profiles() on startup, so
// deleting any of them would silently "undo" itself — block deletion for all.
const BUILTIN_PROFILES: &[&str] = &["deep-review", "quick-review", "grant-review"];

fn builtin_primary_readers(id: &str) -> &'static [&'static str] {
    match id {
        "deep-review" => &["validate_feedback"],
        _ => &[],
    }
}

/// Profiles shipped by earlier releases that were removed from the catalog.
/// A version marker makes the archival a one-time migration, so users may
/// later create custom profiles that happen to reuse one of these IDs.
const RETIRED_BUILTIN_PROFILES: &[(&str, &str)] = &[
    ("empirical", "deep-review"),
    ("quick-code-review", "deep-review"),
    ("revision-response", "deep-review"),
    ("thesis-review", "deep-review"),
    ("rubric-grading", "deep-review"),
    ("codebase-review", "deep-review"),
];

/// Built-ins retired after the v3 catalog migration had already shipped.
/// These need their own marker so existing installations archive them too.
const V9_RETIRED_BUILTIN_PROFILES: &[(&str, &str)] = &[
    ("deep-code-review", "deep-review"),
    ("replication-audit", "deep-review"),
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

fn primary_selector(input_mode: &str) -> Option<ArtifactSelector> {
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
fn configure_artifact_flow(
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
fn generic_profile(
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
    profile.extraction = extraction;
    profile.parallel_context_template = generic_parallel_template();
    profile
}

/// Write a built-in profile file if it doesn't exist yet.
fn write_builtin_if_missing(path: &Path, profile: &ProfileData) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    let json =
        serde_json::to_string_pretty(profile).map_err(|e| format!("Serialize error: {e}"))?;
    restore_profile_bytes(path, json.as_bytes())
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

/// Built-in profiles created on first run.
fn create_builtin_profiles() -> Result<(), String> {
    let profiles = profiles_dir()?;

    // Paper Review (Quick) — fast two-step pass
    write_builtin_if_missing(
        &profiles.join("quick-review.json"),
        &ProfileData::new(
            "Paper Review (Quick)",
            configure_artifact_flow(
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
                "document",
                &[],
            ),
            MergeConfig::default(),
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
            ],
            ExtractionConfig::default(),
            &[],
        ),
    )?;

    Ok(())
}

/// Apply catalog changes to profiles created by older Pipeline releases.
fn migrate_builtin_catalog(profiles: &Path) -> Result<(), String> {
    let marker = profiles.join(".builtin-catalog-v3");
    if !marker.exists() {
        for (id, replacement) in RETIRED_BUILTIN_PROFILES {
            // Do not leave Settings pointing at a profile archived below.
            let _ = crate::settings::replace_active_profile_if(id, replacement);
            archive_retired_profile(profiles, id)?;
        }
        fs::write(&marker, b"paper-and-code-profile-catalog\n").map_err(|error| {
            format!(
                "Failed to record the built-in profile catalog migration '{}': {error}",
                marker.display()
            )
        })?;
    }

    let retired_v9_marker = profiles.join(".builtin-catalog-v9");
    if !retired_v9_marker.exists() {
        for (id, replacement) in V9_RETIRED_BUILTIN_PROFILES {
            if profiles.join(format!("{id}.json")).exists() {
                // Do not leave Settings pointing at a profile archived below.
                let _ = crate::settings::replace_active_profile_if(id, replacement);
                archive_retired_profile(profiles, id)?;
            }
        }
        fs::write(&retired_v9_marker, b"paper-and-grant-profile-catalog\n").map_err(|error| {
            format!(
                "Failed to record the retired-profile catalog migration '{}': {error}",
                retired_v9_marker.display()
            )
        })?;
    }

    // Artifact access is part of the workflow definition, not an ambient
    // executor default. Refresh every shipped profile into the explicit
    // producer/role format. This must run before migrations that load and
    // validate profiles: older profiles declared Read/Write directly, while
    // the current validator permits only optional external capabilities.
    let artifact_marker = profiles.join(".builtin-catalog-v6");
    if !artifact_marker.exists() {
        for id in BUILTIN_PROFILES {
            let path = profiles.join(format!("{id}.json"));
            if !path.exists() {
                continue;
            }
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            profile.steps = configure_artifact_flow(
                profile.steps,
                &profile.extraction.input_mode,
                builtin_primary_readers(id),
            );
            validate_profile_data(&profile)?;
            let json = serde_json::to_string_pretty(&profile)
                .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
            restore_profile_bytes(&path, json.as_bytes())
                .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
        }
        fs::write(&artifact_marker, b"explicit-step-artifact-context\n").map_err(|error| {
            format!(
                "Failed to record the artifact-context migration '{}': {error}",
                artifact_marker.display()
            )
        })?;
    }

    // Built-in workflows are customizable, so rename the profile while
    // preserving any edits users made to its steps and settings.
    for (id, previous_name, current_name) in [
        ("deep-review", "Deep Review", "Paper Review (Full)"),
        ("quick-review", "Quick Review", "Paper Review (Quick)"),
    ] {
        let path = profiles.join(format!("{id}.json"));
        if !path.exists() {
            continue;
        }
        let content = read_profile_file(&path)
            .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
        let mut profile: ProfileData = serde_json::from_str(&content)
            .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
        validate_profile_data(&profile)?;
        if profile.name == previous_name {
            profile.name = current_name.to_string();
            let json = serde_json::to_string_pretty(&profile)
                .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
            restore_profile_bytes(&path, json.as_bytes())
                .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
        }
    }

    // Refresh shipped prompt text only when a built-in still contains the
    // exact prior default. Hash matching preserves every customized prompt,
    // while ensuring existing installations receive the cleaner synthesis and
    // merge defaults rather than only newly created profiles.
    let prompt_marker = profiles.join(".builtin-catalog-v4");
    if !prompt_marker.exists() {
        for id in BUILTIN_PROFILES {
            let path = profiles.join(format!("{id}.json"));
            if !path.exists() {
                continue;
            }
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            if migrate_shipped_prompt_defaults(&mut profile) {
                validate_profile_data(&profile)?;
                let json = serde_json::to_string_pretty(&profile).map_err(|error| {
                    format!("Failed to serialize '{}': {error}", path.display())
                })?;
                restore_profile_bytes(&path, json.as_bytes())
                    .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
            }
        }
        fs::write(&prompt_marker, b"clean-terminal-report-prompts\n").map_err(|error| {
            format!(
                "Failed to record the prompt catalog migration '{}': {error}",
                prompt_marker.display()
            )
        })?;
    }

    // Reuse warmed primary context and encourage batched evidence retrieval.
    // Update only exact prior stock prompts so user customizations remain
    // byte-for-byte intact. This must run before the v7 whole-profile
    // fingerprint: `full_review_profile` already contains the new defaults.
    let retrieval_prompt_marker = profiles.join(".builtin-catalog-v8");
    if !retrieval_prompt_marker.exists() {
        for id in BUILTIN_PROFILES {
            let path = profiles.join(format!("{id}.json"));
            if !path.exists() {
                continue;
            }
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            if migrate_efficient_retrieval_defaults(&mut profile) {
                validate_profile_data(&profile)?;
                let json = serde_json::to_string_pretty(&profile).map_err(|error| {
                    format!("Failed to serialize '{}': {error}", path.display())
                })?;
                restore_profile_bytes(&path, json.as_bytes())
                    .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
            }
        }
        fs::write(
            &retrieval_prompt_marker,
            b"shared-context-aware-batched-retrieval-prompts\n",
        )
        .map_err(|error| {
            format!(
                "Failed to record the retrieval-prompt migration '{}': {error}",
                retrieval_prompt_marker.display()
            )
        })?;
    }

    // Tighten the Paper Review prompts around evidence, prioritization, and
    // false-positive control. Update only exact prior defaults so edits made
    // in the workflow editor remain untouched. This precedes the v7 whole-
    // profile fingerprint because `full_review_profile` contains the new text.
    let review_quality_prompt_marker = profiles.join(".builtin-catalog-v10");
    if !review_quality_prompt_marker.exists() {
        for id in BUILTIN_PROFILES {
            let path = profiles.join(format!("{id}.json"));
            if !path.exists() {
                continue;
            }
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            if migrate_review_quality_prompt_defaults(&mut profile) {
                validate_profile_data(&profile)?;
                let json = serde_json::to_string_pretty(&profile).map_err(|error| {
                    format!("Failed to serialize '{}': {error}", path.display())
                })?;
                restore_profile_bytes(&path, json.as_bytes())
                    .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
            }
        }
        fs::write(
            &review_quality_prompt_marker,
            b"evidence-prioritized-paper-review-prompts\n",
        )
        .map_err(|error| {
            format!(
                "Failed to record the review-quality prompt migration '{}': {error}",
                review_quality_prompt_marker.display()
            )
        })?;
    }

    // Shared context reuse is now part of Paper Review (Full). Built-ins are
    // customizable, so only upgrade an exact semantic match for either prior
    // stock variant (validation enabled on first-run creation, disabled after
    // a reset). A stock profile whose user explicitly left caching disabled is
    // indistinguishable from the old default; any other customization or
    // already-enabled profile is preserved.
    let context_cache_marker = profiles.join(".builtin-catalog-v7");
    if !context_cache_marker.exists() {
        let path = profiles.join("deep-review.json");
        if path.exists() {
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            validate_profile_data(&profile)?;
            if matches_prior_stock_full_review(&profile)? {
                profile.context_cache.enabled = true;
                let json = serde_json::to_string_pretty(&profile).map_err(|error| {
                    format!("Failed to serialize '{}': {error}", path.display())
                })?;
                restore_profile_bytes(&path, json.as_bytes())
                    .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
            }
        }
        fs::write(
            &context_cache_marker,
            b"full-review-shared-context-default\n",
        )
        .map_err(|error| {
            format!(
                "Failed to record the shared-context migration '{}': {error}",
                context_cache_marker.display()
            )
        })?;
    }

    Ok(())
}

fn matches_prior_stock_full_review(profile: &ProfileData) -> Result<bool, String> {
    if profile.context_cache.enabled {
        return Ok(false);
    }
    let actual = serde_json::to_value(profile)
        .map_err(|error| format!("Failed to fingerprint Paper Review (Full): {error}"))?;
    for validate_enabled in [false, true] {
        let mut prior_stock = full_review_profile(validate_enabled);
        prior_stock.context_cache.enabled = false;
        let expected = serde_json::to_value(prior_stock)
            .map_err(|error| format!("Failed to fingerprint stock Full profile: {error}"))?;
        if actual == expected {
            return Ok(true);
        }
    }
    Ok(false)
}

fn prompt_digest(prompt: &str) -> String {
    use sha2::{Digest as _, Sha256};
    format!("{:x}", Sha256::digest(prompt.as_bytes()))
}

fn migrate_shipped_prompt_defaults(profile: &mut ProfileData) -> bool {
    const OLD_EDITOR: &str = "2393f4d8f6f39641b5a5e609e132513abb3850b4995eb0522a3ae077fdef29b0";
    const OLD_EDITOR_ISSUES: &str =
        "01079fa2f1a5c0c44ba88861966b8383bbb5a4ac3b47b0e95ac3a8256bb36eeb";
    const OLD_MERGE: &str = "cd90280a68b6818ba075f293f3ec54629568dd2d3e13dc891bcfb3164cdb9b18";
    let mut changed = false;
    for step in &mut profile.steps {
        let replacement = match prompt_digest(&step.prompt).as_str() {
            OLD_EDITOR => prompts::compiled_default("editor_synthesis"),
            OLD_EDITOR_ISSUES => prompts::compiled_default("editor_synthesis_issues"),
            _ => None,
        };
        if let Some(replacement) = replacement {
            step.prompt = replacement.to_string();
            changed = true;
        }
    }
    if prompt_digest(&profile.merge.prompt) == OLD_MERGE {
        if let Some(replacement) = prompts::compiled_default("merge") {
            profile.merge.prompt = replacement.to_string();
            changed = true;
        }
    }
    changed
}

fn migrate_efficient_retrieval_defaults(profile: &mut ProfileData) -> bool {
    const OLD_PARALLEL_CONTEXT: &str =
        "2d3f253aba5a39e76a560cda448296647b42cd3b4d487bbb761af9e7172fbe65";
    const OLD_GENERIC_PARALLEL_CONTEXT: &str =
        "d2bc29faeb032cc35cb60f2aff561f17a737c18348a76140205777f8b3c33197";
    const OLD_VALIDATE_FEEDBACK: &str =
        "ad7af438fed68f08e4f2e85de9155314f9579d6d96a4c960fb4b53b5ff0e7555";

    let mut changed = false;
    let parallel_replacement = match prompt_digest(&profile.parallel_context_template).as_str() {
        OLD_PARALLEL_CONTEXT => prompts::compiled_default("parallel_context"),
        OLD_GENERIC_PARALLEL_CONTEXT => prompts::compiled_default("parallel_context_generic"),
        _ => None,
    };
    if let Some(replacement) = parallel_replacement {
        profile.parallel_context_template = replacement.to_string();
        changed = true;
    }

    for step in &mut profile.steps {
        if prompt_digest(&step.prompt) == OLD_VALIDATE_FEEDBACK {
            if let Some(replacement) = prompts::compiled_default("validate_feedback") {
                step.prompt = replacement.to_string();
                changed = true;
            }
        }
    }
    changed
}

fn migrate_review_quality_prompt_defaults(profile: &mut ProfileData) -> bool {
    const OLD_PARALLEL_CONTEXT: &str =
        "b5343b777ec44f21af434dbea2daff76e6ef352ac58cf2a928b16e204b04b709";
    const OLD_CONTRIBUTION: &str =
        "733b35d1e2137ea876ca4ef504532ae2d5875291f943584ce1eb4e42ad937e03";
    const OLD_TECHNICAL: &str = "e7574cc654ce76e21ea254d517e7cdd1e9991fd691249226b6c6a5745664e03e";
    const OLD_EMPIRICAL: &str = "b8823245b925a15775cde437ed7ad31fe9fc3766d61c9e2f56677a9b14b2998e";
    const OLD_EXPOSITION: &str = "15f0b9340bd28b4d4e0f3fc5d1b59cd01845c62854eb856eb24e9b052edd5914";
    const OLD_EDITOR_SYNTHESIS: &str =
        "5fd0eed34fddbc6ab32d9330b097b72895e58b91be9c013f42bf5636495ed5d7";
    const OLD_VALIDATE_FEEDBACK: &str =
        "fa49fa759b553c171ed721d2cc8696b1f083a891e237be01ace245188761a744";

    let mut changed = false;
    if prompt_digest(&profile.parallel_context_template) == OLD_PARALLEL_CONTEXT {
        if let Some(replacement) = prompts::compiled_default("parallel_context") {
            profile.parallel_context_template = replacement.to_string();
            changed = true;
        }
    }

    for step in &mut profile.steps {
        let replacement = match prompt_digest(&step.prompt).as_str() {
            OLD_CONTRIBUTION => prompts::compiled_default("contribution"),
            OLD_TECHNICAL => prompts::compiled_default("technical"),
            OLD_EMPIRICAL => prompts::compiled_default("empirical"),
            OLD_EXPOSITION => prompts::compiled_default("exposition"),
            OLD_EDITOR_SYNTHESIS => prompts::compiled_default("editor_synthesis"),
            OLD_VALIDATE_FEEDBACK => prompts::compiled_default("validate_feedback"),
            _ => None,
        };
        if let Some(replacement) = replacement {
            step.prompt = replacement.to_string();
            changed = true;
        }
    }
    changed
}

/// Hide a retired built-in from the profile list without discarding any user
/// customizations it may contain. Avoid overwriting an archive left by a
/// partially completed or earlier migration.
fn archive_retired_profile(profiles: &Path, id: &str) -> Result<(), String> {
    let source = profiles.join(format!("{id}.json"));
    if !source.exists() {
        return Ok(());
    }

    let archive_dir = profiles.join(".retired-builtins");
    fs::create_dir_all(&archive_dir).map_err(|error| {
        format!(
            "Failed to create retired-profile archive '{}': {error}",
            archive_dir.display()
        )
    })?;
    let destination = (1..=1000)
        .map(|copy| {
            let suffix = if copy == 1 {
                String::new()
            } else {
                format!("-{copy}")
            };
            archive_dir.join(format!("{id}{suffix}.json"))
        })
        .find(|candidate| !candidate.exists())
        .ok_or_else(|| format!("Too many archived copies of profile '{id}'"))?;

    fs::rename(&source, &destination).map_err(|error| {
        format!(
            "Failed to archive retired profile '{}' as '{}': {error}",
            source.display(),
            destination.display()
        )
    })
}

// ── Migration ───────────────────────────────────────────────────────

/// Migrate from old single-file formats to profiles directory. Idempotent.
fn ensure_migrated() -> Result<(), String> {
    let _lock = lock_profile_mutations()?;
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
                    save_profile_unlocked("migrated", &profile)?;
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
                        save_profile_unlocked("migrated", &profile)?;
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

        // Create the full paper review as the primary profile.
        let profile = full_review_profile(true);
        let json =
            serde_json::to_string_pretty(&profile).map_err(|e| format!("Serialize error: {e}"))?;
        restore_profile_bytes(&deep_review_path, json.as_bytes())
            .map_err(|e| format!("Failed to write deep-review profile: {e}"))?;
    } else if old_path.exists() {
        let _ = fs::remove_file(&old_path);
    }

    // Always ensure current built-in profiles exist and apply catalog changes
    // to installations created by earlier releases.
    create_builtin_profiles()?;
    migrate_builtin_catalog(&profiles)?;

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
            // Keep loading legacy profile files without mutating them during
            // a read. Explicit saves/imports publish the current format under
            // the cross-process profile lock.
            return Ok(profile);
        }
    }

    Err(format!("Failed to parse profile '{id}'"))
}

fn save_profile_unlocked(id: &str, profile: &ProfileData) -> Result<(), String> {
    let path = profile_path(id)?;
    validate_profile_data(profile)?;
    let json =
        serde_json::to_string_pretty(profile).map_err(|e| format!("Failed to serialize: {e}"))?;
    restore_profile_bytes(&path, json.as_bytes())
        .map_err(|e| format!("Failed to save profile '{}': {e}", path.display()))
}

pub fn save_profile(id: &str, profile: &ProfileData) -> Result<(), String> {
    let _lock = lock_profile_mutations()?;
    save_profile_unlocked(id, profile)
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
    validate_run_conditions(steps)?;
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
            crate::pipeline::structured::validate_schema(schema)
                .map_err(|e| format!("Step '{}' output schema is invalid: {e}", step.id))?;
        }
    }
    Ok(())
}

fn validate_artifact_context(profile: &ProfileData) -> Result<(), String> {
    const MAX_SELECTORS_PER_STEP: usize = 256;
    let named_inputs: std::collections::HashSet<&str> = profile
        .extraction
        .extra_inputs
        .iter()
        .map(|input| input.key.as_str())
        .collect();

    for step in &profile.steps {
        if step.context.include.len() > MAX_SELECTORS_PER_STEP {
            return Err(format!(
                "Step '{}' selects more than {MAX_SELECTORS_PER_STEP} artifact sources",
                step.id
            ));
        }
        let mut after = std::collections::HashSet::new();
        for dependency in &step.after {
            if !after.insert(dependency.as_str()) {
                return Err(format!(
                    "Step '{}' lists order dependency '{}' more than once",
                    step.id, dependency
                ));
            }
        }

        let mut sources = std::collections::HashSet::new();
        for selector in &step.context.include {
            let source_key = match selector {
                ArtifactSelector::Primary { parts } => {
                    if parts.is_empty() {
                        return Err(format!(
                            "Step '{}' selects the primary input without selecting any representations",
                            step.id
                        ));
                    }
                    if profile.extraction.input_mode == "none" {
                        return Err(format!(
                            "Step '{}' selects the primary input, but this workflow has no input",
                            step.id
                        ));
                    }
                    let unique: std::collections::HashSet<_> = parts.iter().collect();
                    if unique.len() != parts.len() {
                        return Err(format!(
                            "Step '{}' repeats a primary-input representation",
                            step.id
                        ));
                    }
                    "primary".to_string()
                }
                ArtifactSelector::Survey => {
                    if !profile.use_orientation {
                        return Err(format!(
                            "Step '{}' selects the survey, but this workflow disables the survey",
                            step.id
                        ));
                    }
                    "survey".to_string()
                }
                ArtifactSelector::NamedInput { key, parts } => {
                    if !named_inputs.contains(key.as_str()) {
                        return Err(format!(
                            "Step '{}' selects unknown named input '{}'",
                            step.id, key
                        ));
                    }
                    if parts.is_empty() {
                        return Err(format!(
                            "Step '{}' selects named input '{}' without a representation",
                            step.id, key
                        ));
                    }
                    let unique: std::collections::HashSet<_> = parts.iter().collect();
                    if unique.len() != parts.len() {
                        return Err(format!(
                            "Step '{}' repeats a representation for named input '{}'",
                            step.id, key
                        ));
                    }
                    format!("named:{key}")
                }
                ArtifactSelector::Step {
                    step: producer,
                    parts,
                    glob,
                } => {
                    if step.phase == Phase::Parallel {
                        return Err(format!(
                            "Parallel step '{}' cannot select output from step '{}'; parallel steps run independently",
                            step.id, producer
                        ));
                    }
                    if parts.is_empty() {
                        return Err(format!(
                            "Step '{}' selects artifacts from '{}' without selecting report or files",
                            step.id, producer
                        ));
                    }
                    let unique: std::collections::HashSet<_> = parts.iter().collect();
                    if unique.len() != parts.len() {
                        return Err(format!(
                            "Step '{}' repeats an artifact type from '{}'",
                            step.id, producer
                        ));
                    }
                    if !glob.is_empty() {
                        if !parts.contains(&StepArtifactPart::Files) {
                            return Err(format!(
                                "Step '{}' sets a file glob for '{}' without selecting files",
                                step.id, producer
                            ));
                        }
                        if glob.len() > 1024
                            || glob.starts_with('/')
                            || glob.contains('\\')
                            || glob.split('/').any(|part| part == "..")
                            || glob.contains(':')
                        {
                            return Err(format!(
                                "Step '{}' has an invalid supporting-file glob for '{}'",
                                step.id, producer
                            ));
                        }
                    }
                    format!("step:{producer}")
                }
            };
            if !sources.insert(source_key.clone()) {
                return Err(format!(
                    "Step '{}' selects artifact source '{}' more than once; combine its parts in one entry",
                    step.id, source_key
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
    validate_artifact_context(profile)?;

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
        "" | "auto" | "llm" | "marker" | "paddleocr-vl-full" | "pdftotext"
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
        if variable.label.len() > 1_024 {
            return Err(format!("Variable '{}' label is too long", variable.key));
        }
        if variable.default.len() > crate::safety::MAX_RUNTIME_VALUE_BYTES {
            return Err(format!(
                "Variable '{}' default exceeds the 1 MB safety limit",
                variable.key
            ));
        }
        if variable.choices.len() > 100
            || variable.choices.iter().any(|choice| choice.len() > 65_536)
        {
            return Err(format!(
                "Variable '{}' has too many or oversized choices",
                variable.key
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
        if input.label.len() > 1_024 {
            return Err(format!("Named input '{}' label is too long", input.key));
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

impl StepConfig {
    /// Step ids whose artifacts this step consumes.
    pub fn artifact_dependencies(&self) -> impl Iterator<Item = &str> {
        self.context
            .include
            .iter()
            .filter_map(|selector| match selector {
                ArtifactSelector::Step { step, .. } => Some(step.as_str()),
                _ => None,
            })
    }
}

/// Compute each enabled step's dependency set. `after` contributes order-only
/// edges; upstream artifact selections contribute data edges.
pub(crate) fn resolve_dependencies(
    enabled: &[&StepConfig],
) -> Vec<std::collections::HashSet<String>> {
    enabled
        .iter()
        .map(|step| {
            step.after
                .iter()
                .map(String::as_str)
                .chain(step.artifact_dependencies())
                .map(str::to_string)
                .collect()
        })
        .collect()
}

/// Reject enabled dependency graphs that cannot run. Disabled steps may retain
/// stale settings while being edited, but an enabled step may never wait on a
/// disabled or unknown upstream.
pub fn validate_dependencies(steps: &[StepConfig]) -> Result<(), String> {
    use std::collections::{HashMap, HashSet};
    let ids: HashSet<&str> = steps.iter().map(|s| s.id.as_str()).collect();
    let enabled_ids: HashSet<&str> = steps
        .iter()
        .filter(|step| step.enabled)
        .map(|step| step.id.as_str())
        .collect();

    // Unknown / self dependencies.
    for s in steps.iter().filter(|step| step.enabled) {
        for dep in s
            .after
            .iter()
            .map(String::as_str)
            .chain(s.artifact_dependencies())
        {
            if dep == s.id {
                return Err(format!("Step '{}' lists itself as a dependency.", s.id));
            }
            if !ids.contains(dep) {
                return Err(format!(
                    "Step '{}' depends on unknown step '{}'.",
                    s.id, dep
                ));
            }
            if !enabled_ids.contains(dep) {
                return Err(format!(
                    "Step '{}' depends on disabled step '{}'. Enable it or remove the dependency.",
                    s.id, dep
                ));
            }
        }
    }

    // Cycle detection over the complete effective graph (DFS with a colour
    // map), including both order-only and artifact-dataflow dependencies.
    let enabled: Vec<&StepConfig> = steps.iter().filter(|step| step.enabled).collect();
    let resolved = resolve_dependencies(&enabled);
    let graph: HashMap<&str, Vec<&str>> = enabled
        .iter()
        .zip(&resolved)
        .map(|(step, deps)| (step.id.as_str(), deps.iter().map(String::as_str).collect()))
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

fn validate_json_pointer(pointer: &str) -> bool {
    if pointer.is_empty() {
        return true;
    }
    if !pointer.starts_with('/') {
        return false;
    }
    let bytes = pointer.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            if index + 1 >= bytes.len() || !matches!(bytes[index + 1], b'0' | b'1') {
                return false;
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    true
}

/// Validate run guards against the effective dependency graph. An output guard
/// may only read a transitive upstream, which guarantees the referent has
/// completed before the scheduler evaluates the guard.
fn validate_run_conditions(steps: &[StepConfig]) -> Result<(), String> {
    let enabled: Vec<&StepConfig> = steps.iter().filter(|step| step.enabled).collect();
    let deps = resolve_dependencies(&enabled);
    let index: std::collections::HashMap<&str, usize> = enabled
        .iter()
        .enumerate()
        .map(|(i, step)| (step.id.as_str(), i))
        .collect();

    for (step_index, step) in enabled.iter().enumerate() {
        let Some(condition) = &step.run_if else {
            continue;
        };
        match condition {
            RunCondition::OutputMatches {
                step: target,
                pattern,
                ..
            } => {
                if target.is_empty() {
                    return Err(format!(
                        "Step '{}' output condition must select an upstream step.",
                        step.id
                    ));
                }
                if pattern.len() > MAX_RUN_IF_PATTERN_BYTES {
                    return Err(format!(
                        "Step '{}' output condition exceeds the {MAX_RUN_IF_PATTERN_BYTES}-byte pattern limit.",
                        step.id
                    ));
                }
                regex::Regex::new(pattern).map_err(|error| {
                    format!(
                        "Step '{}' output condition has an invalid regular expression: {error}",
                        step.id
                    )
                })?;
                let Some(&target_index) = index.get(target.as_str()) else {
                    return Err(format!(
                        "Step '{}' output condition refers to an unknown or disabled step '{}'.",
                        step.id, target
                    ));
                };
                let mut pending: Vec<usize> = deps[step_index]
                    .iter()
                    .filter_map(|id| index.get(id.as_str()).copied())
                    .collect();
                let mut upstream = std::collections::HashSet::new();
                while let Some(current) = pending.pop() {
                    if upstream.insert(current) {
                        pending.extend(
                            deps[current]
                                .iter()
                                .filter_map(|id| index.get(id.as_str()).copied()),
                        );
                    }
                }
                if !upstream.contains(&target_index) {
                    return Err(format!(
                        "Step '{}' output condition reads '{}', but that step is not an upstream dependency.",
                        step.id, target
                    ));
                }
            }
            RunCondition::SurveyPath { pointer, .. } => {
                if pointer.len() > MAX_JSON_POINTER_BYTES {
                    return Err(format!(
                        "Step '{}' survey pointer exceeds the {MAX_JSON_POINTER_BYTES}-byte limit.",
                        step.id
                    ));
                }
                if !validate_json_pointer(pointer) {
                    return Err(format!(
                        "Step '{}' survey condition uses an invalid JSON pointer '{}'.",
                        step.id, pointer
                    ));
                }
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
    let _lock = lock_profile_mutations()?;
    validate_profile_id(profile_id)?;
    validate_profile_steps(&config.steps)?;
    let name = load_profile(profile_id)?.name;
    let profile = ProfileData::from_config(name, config);
    save_profile_unlocked(profile_id, &profile)
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
    let mut walk = crate::safety::WalkBudget::new("Profile listing");
    for entry in fs::read_dir(&dir).map_err(|e| format!("Failed to read profiles dir: {e}"))? {
        walk.entry()?;
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
    let _lock = lock_profile_mutations()?;
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
    let profile = generic_profile(
        name,
        generic_starter_steps(),
        ExtractionConfig::default(),
        &[],
    );
    save_profile_unlocked(&id, &profile)?;
    Ok(profile_summary(id, &profile))
}

pub fn duplicate_profile(source_id: &str, new_name: &str) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let _lock = lock_profile_mutations()?;
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
    save_profile_unlocked(&new_id, &profile)?;
    Ok(profile_summary(new_id, &profile))
}

fn duplicate_profile_data(mut source: ProfileData, new_name: &str) -> ProfileData {
    source.name = new_name.to_string();
    source
}

pub fn rename_profile(id: &str, new_name: &str) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let _lock = lock_profile_mutations()?;
    let mut profile = load_profile(id)?;
    profile.name = new_name.to_string();
    save_profile_unlocked(id, &profile)?;
    Ok(profile_summary(id.to_string(), &profile))
}

pub fn delete_profile(id: &str) -> Result<(), String> {
    let _lock = lock_profile_mutations()?;
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
        context_cache: profile.context_cache,
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
                context_cache: ContextCacheConfig::default(),
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
            context_cache: ContextCacheConfig::default(),
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
    context_cache: ContextCacheConfig,
    use_orientation: bool,
    orientation_prompt: String,
    extraction: ExtractionConfig,
    parallel_context_template: String,
    variables: Vec<VarSpec>,
) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let _lock = lock_profile_mutations()?;
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
    profile.context_cache = context_cache;
    profile.use_orientation = use_orientation;
    profile.orientation_prompt = orientation_prompt;
    profile.extraction = extraction;
    profile.parallel_context_template = parallel_context_template;
    profile.variables = variables;
    save_profile_unlocked(&id, &profile)?;
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
            let _lock = lock_profile_mutations()?;
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
            current.marker_force_ocr = imported_settings.marker_force_ocr;
            current.marker_disable_images = imported_settings.marker_disable_images;
            current.marker_lowres_dpi = imported_settings.marker_lowres_dpi;
            current.marker_highres_dpi = imported_settings.marker_highres_dpi;
            current.marker_pdftext_workers = imported_settings.marker_pdftext_workers;
            current.marker_layout_batch_size = imported_settings.marker_layout_batch_size;
            current.marker_recognition_batch_size = imported_settings.marker_recognition_batch_size;
            current.paddle_page_concurrency = imported_settings.paddle_page_concurrency;
            current.paddle_mtmd_batch_tokens = imported_settings.paddle_mtmd_batch_tokens;
            current.paddle_flash_attention = imported_settings.paddle_flash_attention;
            current.paddle_max_output_tokens = imported_settings.paddle_max_output_tokens;
            current.paddle_page_retries = imported_settings.paddle_page_retries;
            current.paddle_render_dpi = imported_settings.paddle_render_dpi;
            current.paddle_full_layout_detection = imported_settings.paddle_full_layout_detection;
            current.paddle_full_layout_threshold = imported_settings.paddle_full_layout_threshold;
            current.paddle_full_layout_nms = imported_settings.paddle_full_layout_nms;
            current.paddle_full_layout_merge_bboxes_mode =
                imported_settings.paddle_full_layout_merge_bboxes_mode;
            current.paddle_full_merge_layout_blocks =
                imported_settings.paddle_full_merge_layout_blocks;
            current.paddle_full_ocr_image_blocks = imported_settings.paddle_full_ocr_image_blocks;
            current.paddle_full_format_block_content =
                imported_settings.paddle_full_format_block_content;
            current.paddle_full_merge_tables = imported_settings.paddle_full_merge_tables;
            current.paddle_full_relevel_titles = imported_settings.paddle_full_relevel_titles;
            current.paddle_full_show_formula_numbers =
                imported_settings.paddle_full_show_formula_numbers;
            current.pdf_extraction_timeout_secs = imported_settings.pdf_extraction_timeout_secs;
            current.reuse_pdf_extraction_cache = imported_settings.reuse_pdf_extraction_cache;
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
                    let prior = match fs::symlink_metadata(&path) {
                        Ok(_) => Some(read_profile_file(&path)?.into_bytes()),
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
                if let Err(error) = save_profile_unlocked(id, profile) {
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
    fn legacy_profile_marker_overrides_are_ignored_and_not_reserialized() {
        let extraction: ExtractionConfig = serde_json::from_value(serde_json::json!({
            "method": "marker",
            "marker_disable_ocr": true,
            "marker_disable_images": true
        }))
        .unwrap();
        assert_eq!(extraction.method, "marker");
        let serialized = serde_json::to_value(extraction).unwrap();
        assert!(serialized.get("marker_disable_ocr").is_none());
        assert!(serialized.get("marker_disable_images").is_none());
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
        });
        assert!(validate_run_conditions(&[step.clone()]).is_err());
        step.run_if = Some(RunCondition::SurveyPath {
            pointer: "/metadata/a~1b".into(),
            equals: None,
            exists: Some(true),
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
        assert_eq!(
            BUILTIN_PROFILES,
            ["deep-review", "quick-review", "grant-review"]
        );
        assert_eq!(
            V9_RETIRED_BUILTIN_PROFILES,
            [
                ("deep-code-review", "deep-review"),
                ("replication-audit", "deep-review"),
            ]
        );
    }

    #[test]
    fn artifact_migration_precedes_validation_of_legacy_builtin_tools() {
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

        let mut step = step_with_id("contribution");
        step.tools = vec!["Read".into()];
        let profile = ProfileData::new("Deep Review", vec![step], MergeConfig::default());
        assert!(validate_profile_data(&profile)
            .unwrap_err()
            .contains("unsupported tool 'Read'"));

        let path = dir.path().join("deep-review.json");
        fs::write(&path, serde_json::to_vec_pretty(&profile).unwrap()).unwrap();

        migrate_builtin_catalog(dir.path()).unwrap();

        let migrated: ProfileData = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        validate_profile_data(&migrated).unwrap();
        assert_eq!(migrated.name, "Paper Review (Full)");
        assert!(migrated.steps[0].tools.is_empty());
        assert!(migrated.steps[0].context.include.iter().any(|selector| {
            matches!(
                selector,
                ArtifactSelector::Primary {
                    parts
                } if parts.contains(&PrimaryArtifactPart::Source)
            )
        }));
        assert!(dir.path().join(".builtin-catalog-v6").exists());
    }

    #[test]
    fn context_cache_migration_upgrades_only_prior_stock_full_profiles() {
        for validate_enabled in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            mark_builtin_catalog_through_v6(dir.path());
            let path = dir.path().join("deep-review.json");
            let mut prior_stock = full_review_profile(validate_enabled);
            prior_stock.context_cache.enabled = false;
            prior_stock.parallel_context_template = prior_stock
                .parallel_context_template
                .replace(
                    "\nBefore reporting an issue, check the surrounding discussion, footnotes, and any appendix or supplementary material supplied with the paper to see whether it is addressed. Distinguish what the paper states from your inference. Treat any numerical limit in the specialist instructions as a ceiling, not a target: report only material, well-supported issues, even if that means reporting none.\n",
                    "",
                )
                .replace(
                    "Use the paper text already present in shared context when available; otherwise read it from the path above. Then produce your report following the instructions above.",
                    "Read the paper text, then produce your report following the instructions above.",
                );
            let validate = prior_stock
                .steps
                .iter_mut()
                .find(|step| step.id == "validate_feedback")
                .unwrap();
            validate.prompt = validate.prompt.replace(
                "STEP 1 — PREPARE THE PAPER EVIDENCE (do this before verification):\nIf the complete paper text and orientation map are already present in shared context, use them directly and do not read their staged files again. Otherwise read the orientation map and the complete paper text. Retrieve independent bounded ranges in batches or one tool turn when supported, and continue sequentially until the entire paper has been covered if batching is unavailable or incomplete.\n\nSTEP 2 — VERIFY ALL COMMENTS:\nCheck every comment in the consolidated report below against the complete paper evidence.",
                "STEP 1 — READ THE PAPER (do this first, before any verification):\nRead the full paper text at {paper_path} in a single Read call. Also read the orientation map: {orientation}\n\nSTEP 2 — VERIFY ALL COMMENTS:\nUsing the paper text now in your context, check every comment in the consolidated report below.",
            );
            validate.prompt = validate
                .prompt
                .replace(
                    "- **Misquoted or paraphrased claims**: If the actual text differs, determine whether the exact wording still supports the underlying concern. Correct an immaterial error; drop the comment if its substance depends on the misattribution.",
                    "- **Misquoted or paraphrased claims**: If the comment attributes a claim to the paper but the actual text says something different, the comment is invalid.",
                )
                .replace(
                    "- **Claims about tables, figures, or equations**: Inspect the relevant rendered page or document asset when available. Do not rely on possibly garbled extracted text when the visual evidence can resolve the claim.\n",
                    "",
                )
                .replace(
                    "Classify each comment privately as verified, repairable, or unsupported. Reproduce verified comments. When the underlying issue is valid but a quotation, page number, table entry, numerical detail, or scope is wrong, correct that detail and narrow any overstatement rather than dropping the comment. Keep the same underlying concern; do not introduce a different issue. Drop comments that are false positives or whose underlying concern you cannot confirm.\n\nOutput: Return the surviving comments using the same format and section groupings as the consolidated report. Omit empty sections and renumber sequentially.",
                    "Output: Reproduce only the valid comments using the same format and section groupings as the consolidated report. Renumber sequentially. Drop any comment that is a false positive or that you cannot confirm.",
                )
                .replace(
                    "The output should read like the consolidated report after factual corrections and filtering.",
                    "The output should read exactly like the consolidated report, just shorter.",
                );
            assert_eq!(
                prompt_digest(&prior_stock.parallel_context_template),
                "2d3f253aba5a39e76a560cda448296647b42cd3b4d487bbb761af9e7172fbe65"
            );
            assert_eq!(
                prompt_digest(&validate.prompt),
                "ad7af438fed68f08e4f2e85de9155314f9579d6d96a4c960fb4b53b5ff0e7555"
            );
            fs::write(&path, serde_json::to_vec_pretty(&prior_stock).unwrap()).unwrap();

            migrate_builtin_catalog(dir.path()).unwrap();

            let migrated: ProfileData = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert!(migrated.context_cache.enabled);
            assert_eq!(
                migrated.parallel_context_template,
                prompts::compiled_default("parallel_context").unwrap()
            );
            assert_eq!(
                migrated
                    .steps
                    .iter()
                    .find(|step| step.id == "validate_feedback")
                    .unwrap()
                    .enabled,
                validate_enabled
            );
            assert!(!migrated
                .steps
                .iter()
                .find(|step| step.id == "validate_feedback")
                .unwrap()
                .prompt
                .contains("single Read call"));
            assert!(dir.path().join(".builtin-catalog-v8").exists());
            assert!(dir.path().join(".builtin-catalog-v7").exists());
        }
    }

    #[test]
    fn context_cache_migration_preserves_distinguishable_customizations() {
        let dir = tempfile::tempdir().unwrap();
        mark_builtin_catalog_through_v6(dir.path());
        let path = dir.path().join("deep-review.json");
        let mut customized = full_review_profile(false);
        customized.context_cache.enabled = false;
        customized.steps[0].prompt.push_str("\nCustom instruction.");
        let original = serde_json::to_vec_pretty(&customized).unwrap();
        fs::write(&path, &original).unwrap();

        migrate_builtin_catalog(dir.path()).unwrap();

        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(dir.path().join(".builtin-catalog-v7").exists());
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

        step.tools = vec!["WebSearch".to_string()];
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

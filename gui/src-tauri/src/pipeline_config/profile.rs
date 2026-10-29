//! Runtime, persisted, and portable profile representations and their defaults.

use super::{default_parallel_template, ExtractionConfig, OutputConfig, StepConfig, VarSpec};
use serde::{Deserialize, Serialize};

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

    pub(crate) fn from_config(name: impl Into<String>, config: &PipelineConfig) -> Self {
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
/// survey conditions; v8 adds explicit published run products; v9 adds live
/// output-schema references, findings lineage, and artifact fan-out; v10 adds
/// required and quorum dependency-success policies; v11 adds validated,
/// secret-aware variables, constrained named inputs, and named products. v1
/// (unversioned) profiles read fine because every added field is
/// `#[serde(default)]`; exports are tagged so future format changes can
/// migrate or reject gracefully.
pub const CURRENT_SCHEMA_VERSION: u32 = 11;

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
    pub(super) fn from_profile(id: String, profile: ProfileData) -> Self {
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

    pub(super) fn to_profile_data(&self) -> ProfileData {
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

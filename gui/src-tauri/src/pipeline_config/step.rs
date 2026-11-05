//! Workflow step records, artifact selections, and inherited agent policy.

use super::PipelineConfig;
use serde::{Deserialize, Serialize};

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

/// Success requirements evaluated after dependency steps reach a terminal
/// state. `after` remains order-only; selected artifacts remain optional
/// unless named here. Every `required` step must succeed, and at least
/// `minimum_successes` members of `quorum` must succeed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependencyPolicy {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quorum: Vec<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub minimum_successes: u32,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

impl DependencyPolicy {
    fn is_empty(&self) -> bool {
        self.required.is_empty() && self.quorum.is_empty() && self.minimum_successes == 0
    }
}

/// What a step runs. Model steps call a provider; analyzer steps run one
/// deterministic Pipeline check and make no model call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepKind {
    #[default]
    Model,
    Analyzer,
}

impl StepKind {
    pub fn is_model(&self) -> bool {
        *self == StepKind::Model
    }
}

/// A versioned host analyzer and its options. The id and version are pinned
/// so a saved workflow keeps the exact check it was written for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzerSpec {
    pub id: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepConfig {
    pub id: String,
    pub label: String,
    /// `model` (default) or `analyzer`. Omitted when `model`, so existing
    /// workflows serialize and fingerprint exactly as before.
    #[serde(default, skip_serializing_if = "StepKind::is_model")]
    pub kind: StepKind,
    /// Required for analyzer steps; absent for model steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analyzer: Option<AnalyzerSpec>,
    pub prompt: String,
    /// Trusted, literal reviewer instructions. Codex App Server uses developerInstructions.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub system_prompt: String,
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
    /// Required and quorum success semantics for terminal dependencies.
    #[serde(default, skip_serializing_if = "DependencyPolicy::is_empty")]
    pub dependency_policy: DependencyPolicy,
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

/// Fan-out configuration for a step: run its prompt once per matching file,
/// or once per element of an upstream structured artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForEach {
    /// Glob relative to the input root (e.g. "chapters/*.tex", "**/*.py").
    /// Empty when `artifact` supplies the items instead.
    #[serde(default)]
    pub glob: String,
    /// Hard cap on the number of items, to bound cost. Defaults to 20.
    #[serde(default = "default_for_each_max")]
    pub max: u32,
    /// Fan out over the elements of an upstream step's canonical JSON
    /// artifact instead of files. Adds a dataflow edge on that step; each
    /// element binds `{item}` in the prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ForEachArtifact>,
}

/// An upstream array to fan out over: a producing step plus an RFC 6901 JSON
/// pointer into its canonical artifact (empty pointer = the artifact root).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForEachArtifact {
    pub step: String,
    #[serde(default)]
    pub pointer: String,
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
            kind: StepKind::Model,
            analyzer: None,
            prompt: String::new(),
            system_prompt: String::new(),
            enabled: true,
            phase: Phase::Parallel,
            tools: Vec::new(),
            agents: Vec::new(),
            model: String::new(),
            model_overrides: std::collections::HashMap::new(),
            effort: String::new(),
            effort_overrides: std::collections::HashMap::new(),
            after: Vec::new(),
            dependency_policy: DependencyPolicy::default(),
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
        // Analyzer steps make no provider call.
        if !step.agents.is_empty() || step.kind == StepKind::Analyzer {
            continue;
        }
        let (models, efforts) = match step.phase {
            Phase::Parallel => {
                step.agents = settings.parallel_agents();
                (
                    &settings.default_parallel_model_overrides,
                    &settings.default_parallel_effort_overrides,
                )
            }
            Phase::Sequential => {
                step.agents = vec![settings.sequential_agent().to_string()];
                (
                    &settings.default_sequential_model_overrides,
                    &settings.default_sequential_effort_overrides,
                )
            }
        };
        // Role defaults fill gaps; a step's own override for the same key wins.
        // Overrides take precedence over the step's single `model`/`effort`
        // fields, so a step that sets one of them takes no role default.
        if step.model.trim().is_empty() {
            for (key, selection) in models {
                step.model_overrides
                    .entry(key.clone())
                    .or_insert_with(|| selection.clone());
            }
        }
        if step.effort.trim().is_empty() {
            for (key, effort) in efforts {
                step.effort_overrides
                    .entry(key.clone())
                    .or_insert_with(|| effort.clone());
            }
        }
    }
}

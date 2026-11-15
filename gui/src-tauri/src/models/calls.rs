use serde::{Deserialize, Serialize};

/// Stable, provider-neutral categories for model-issued tool calls. Providers
/// expose different tool names and levels of detail, so unclassifiable calls
/// remain visible instead of being dropped or guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallKind {
    TextFile,
    Image,
    Web,
    ShellOrOther,
    Unknown,
}

/// Reported tool calls grouped into categories that can be compared across
/// providers. All fields default to zero so older reports and manifests load.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallCounts {
    #[serde(default)]
    pub text_file: u64,
    #[serde(default)]
    pub image: u64,
    #[serde(default)]
    pub web: u64,
    #[serde(default)]
    pub shell_or_other: u64,
    #[serde(default)]
    pub unknown: u64,
}

impl ToolCallCounts {
    pub const ZERO: Self = Self {
        text_file: 0,
        image: 0,
        web: 0,
        shell_or_other: 0,
        unknown: 0,
    };

    pub fn add_kind(&mut self, kind: ToolCallKind, count: u64) {
        let target = match kind {
            ToolCallKind::TextFile => &mut self.text_file,
            ToolCallKind::Image => &mut self.image,
            ToolCallKind::Web => &mut self.web,
            ToolCallKind::ShellOrOther => &mut self.shell_or_other,
            ToolCallKind::Unknown => &mut self.unknown,
        };
        *target = target.saturating_add(count);
    }

    pub fn add_counts(&mut self, other: Self) {
        self.text_file = self.text_file.saturating_add(other.text_file);
        self.image = self.image.saturating_add(other.image);
        self.web = self.web.saturating_add(other.web);
        self.shell_or_other = self.shell_or_other.saturating_add(other.shell_or_other);
        self.unknown = self.unknown.saturating_add(other.unknown);
    }

    pub fn total(self) -> u64 {
        self.text_file
            .saturating_add(self.image)
            .saturating_add(self.web)
            .saturating_add(self.shell_or_other)
            .saturating_add(self.unknown)
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StepCallRecord {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub model_transport: String,
    #[serde(default)]
    pub model_policy: String,
    #[serde(default)]
    pub model_source: String,
    #[serde(default)]
    pub model_catalog_updated_at: String,
    /// How Pipeline knows `model`: `reported` (the provider said so after the
    /// call), `requested` (Pipeline sent this model), `expected` (the CLI
    /// chose and did not say; this is the model it last chose or listed as
    /// its default), or `unknown`. Empty for reports saved before this field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model_basis: String,
    /// Resolved reasoning-effort setting requested for this call. Empty when
    /// the provider's default applied, and for reports saved before effort
    /// provenance was recorded.
    #[serde(default)]
    pub effort: String,
    #[serde(default)]
    pub duration_secs: u64,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cached_input_tokens: u64,
    #[serde(default)]
    pub cache_write_input_tokens: u64,
    /// Model generations reported inside this provider call. A single logical
    /// step can contain several generations when tools are used.
    #[serde(default)]
    pub model_round_trips: u64,
    /// Model-issued tool calls reported inside this provider call.
    #[serde(default, skip_serializing_if = "ToolCallCounts::is_empty")]
    pub tool_calls: ToolCallCounts,
    #[serde(default)]
    pub attempt_count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StepOutput {
    pub step_id: String,
    pub step_label: String,
    /// Non-empty only when this output must be merged with sibling agent
    /// outputs.  Unlike parsing `step_id`, this keeps fan-out units distinct.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub merge_group: String,
    /// Source item bound to `{item}` for fan-out outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fan_out_item: Option<String>,
    /// What this step's fan-out limit left unread, when it stopped short.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_note: Option<String>,
    #[serde(default = "default_phase")]
    pub phase: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub raw_text: String,
    /// True when `raw_text` is a schema-validated, standalone JSON artifact.
    /// This controls persistence/export and schema-aware rerun reuse.
    #[serde(default)]
    pub structured_json: bool,
    /// Wall-clock time the step's LLM call took, in seconds. 0 for old reports.
    #[serde(default)]
    pub duration_secs: u64,
    /// Input tokens the call reported (0 when the provider reports no usage).
    #[serde(default)]
    pub input_tokens: u64,
    /// Output tokens the call reported.
    #[serde(default)]
    pub output_tokens: u64,
    /// Input tokens served from a provider cache.
    #[serde(default)]
    pub cached_input_tokens: u64,
    /// Input tokens written to a provider cache.
    #[serde(default)]
    pub cache_write_input_tokens: u64,
    /// Model generations represented by this step, summed across its calls.
    #[serde(default)]
    pub model_round_trips: u64,
    /// Model-issued tool calls represented by this step.
    #[serde(default, skip_serializing_if = "ToolCallCounts::is_empty")]
    pub tool_calls: ToolCallCounts,
    /// Number of provider attempts represented by the totals above.
    #[serde(default)]
    pub attempt_count: u32,
    /// Effective model id/alias used for this step (resolved override or global).
    #[serde(default)]
    pub model: String,
    /// Provider that ran this step ("claude", "codex", "antigravity", "local").
    #[serde(default)]
    pub provider: String,
    /// Transport used by the provider ("cli" or "api").
    #[serde(default)]
    pub model_transport: String,
    /// Selection policy recorded for reproducibility (Automatic/role/pinned).
    #[serde(default)]
    pub model_policy: String,
    /// Where the model catalog came from (installed_cli, provider_api, cache,
    /// bundled_policy, or provider_default).
    #[serde(default)]
    pub model_source: String,
    /// Catalog timestamp used to resolve this model.
    #[serde(default)]
    pub model_catalog_updated_at: String,
    /// Every provider call represented by this output. Multi-agent merges keep
    /// the original analyses plus the merge call rather than losing their
    /// usage and model provenance. Empty for reports saved before this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<StepCallRecord>,
    /// True when the step was skipped by its `run_if` guard. The entry is kept
    /// (so dependents' `{step:id}` placeholders resolve) but is not real report
    /// content.
    #[serde(default)]
    pub skipped: bool,
}

fn default_phase() -> String {
    "parallel".to_string()
}

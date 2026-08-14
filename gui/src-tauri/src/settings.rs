use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const MAX_SETTINGS_BYTES: usize = 4 * 1024 * 1024;

/// A durable model-selection policy. `Automatic` deliberately means
/// "delegate to the provider" rather than a particular model ID. Roles are
/// resolved through the provider/transport catalog, while pinned IDs never
/// float silently.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ModelSelection {
    #[default]
    Automatic,
    Role {
        role: String,
    },
    Pinned {
        model: String,
    },
}

impl ModelSelection {
    pub fn from_legacy(value: &str) -> Self {
        let value = value.trim();
        if value.is_empty() {
            return Self::Automatic;
        }
        match value {
            "auto" | "automatic" => Self::Automatic,
            "sonnet" | "opus" | "haiku" | "fable" => Self::Role {
                role: value.to_string(),
            },
            _ => Self::Pinned {
                model: value.to_string(),
            },
        }
    }

    pub fn legacy_value(&self) -> String {
        match self {
            Self::Automatic => String::new(),
            Self::Role { role } => role.clone(),
            Self::Pinned { model } => model.clone(),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Automatic => "Automatic".to_string(),
            Self::Role { role } => format!("{role} role"),
            Self::Pinned { model } => model.clone(),
        }
    }
}

/// Validate a string that will be passed as a CLI argument value.
/// Rejects values that look like flags or contain control characters.
/// Returns the trimmed value, or empty string if invalid.
pub fn sanitize_cli_arg(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Reject values starting with - (could be misinterpreted as flags)
    if trimmed.starts_with('-') {
        return String::new();
    }
    // Reject control characters (newlines, tabs, null bytes)
    if trimmed.chars().any(|c| c.is_control()) {
        return String::new();
    }
    // Reject unreasonably long values
    if trimmed.len() > 200 {
        return String::new();
    }
    trimmed.to_string()
}

/// Persisted user settings at ~/.pipeline/settings.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Preferred LLM provider: "claude", "codex", or "antigravity".
    /// Used for steps that don't specify an explicit agent. Default = "claude".
    #[serde(default = "default_provider", alias = "llm_provider")]
    pub preferred_provider: String,

    /// Providers used when a Parallel workflow step leaves `agents` empty.
    /// An empty list is retained as a legacy representation and resolves to
    /// `preferred_provider`; the current Settings UI always saves at least one.
    #[serde(default)]
    pub default_parallel_agents: Vec<String>,

    /// Per-provider model and effort policies for inherited Parallel steps.
    /// Keys use the same `provider:transport` form as StepConfig overrides.
    #[serde(default)]
    pub default_parallel_model_overrides: std::collections::HashMap<String, ModelSelection>,
    #[serde(default)]
    pub default_parallel_effort_overrides: std::collections::HashMap<String, String>,

    /// Provider and policy used to merge outputs from multi-agent Parallel
    /// steps. Empty preserves the pre-field behavior by inheriting the
    /// Sequential default and its policies.
    #[serde(default)]
    pub default_merge_agent: String,
    #[serde(default)]
    pub default_merge_model_overrides: std::collections::HashMap<String, ModelSelection>,
    #[serde(default)]
    pub default_merge_effort_overrides: std::collections::HashMap<String, String>,

    /// Provider used when a Sequential step leaves `agents` empty. Empty is a
    /// backward-compatible alias for `preferred_provider`.
    #[serde(default)]
    pub default_sequential_agent: String,
    #[serde(default)]
    pub default_sequential_model_overrides: std::collections::HashMap<String, ModelSelection>,
    #[serde(default)]
    pub default_sequential_effort_overrides: std::collections::HashMap<String, String>,

    /// Provider and policy used for the orientation-map call. Empty inherits
    /// `preferred_provider`, matching settings written before this distinction
    /// existed.
    #[serde(default)]
    pub default_orientation_agent: String,
    #[serde(default)]
    pub default_orientation_model_overrides: std::collections::HashMap<String, ModelSelection>,
    #[serde(default)]
    pub default_orientation_effort_overrides: std::collections::HashMap<String, String>,

    /// Optional provider/model used once when the active provider reports a
    /// durable account or subscription usage limit. Empty disables automatic
    /// fallback. Unlike ordinary role defaults, this applies to every LLM
    /// call, including orientation and merge calls.
    #[serde(default)]
    pub usage_limit_fallback_agent: String,
    #[serde(default)]
    pub usage_limit_fallback_model_overrides: std::collections::HashMap<String, ModelSelection>,
    #[serde(default)]
    pub usage_limit_fallback_effort_overrides: std::collections::HashMap<String, String>,

    /// Max concurrent referee passes (1-20).
    #[serde(default = "default_workers")]
    pub max_workers: u32,

    /// Active pipeline profile ID.
    #[serde(default = "default_profile")]
    pub active_profile: String,

    /// Legacy provider-level model field retained for settings-file and bundle
    /// compatibility. New runs use role/pass overrides or provider automatic.
    #[serde(default)]
    pub claude_model: String,

    /// Legacy transport-specific selection retained for wire compatibility.
    #[serde(default)]
    pub claude_cli_model_selection: ModelSelection,

    /// Legacy transport-specific selection retained for wire compatibility.
    #[serde(default)]
    pub claude_api_model_selection: ModelSelection,

    /// Legacy provider-level effort retained for wire compatibility.
    #[serde(default)]
    pub claude_effort: String,

    /// Legacy provider-level model field retained for settings-file and bundle
    /// compatibility. New runs use role/pass overrides or provider automatic.
    #[serde(default)]
    pub codex_model: String,

    /// Legacy transport-specific selection retained for wire compatibility.
    #[serde(default)]
    pub codex_cli_model_selection: ModelSelection,

    /// Legacy transport-specific selection retained for wire compatibility.
    #[serde(default)]
    pub codex_api_model_selection: ModelSelection,

    /// Legacy provider-level effort retained for wire compatibility.
    #[serde(default)]
    pub codex_effort: String,

    /// Legacy transport-specific selection retained for wire compatibility.
    #[serde(default)]
    pub antigravity_cli_model_selection: ModelSelection,

    /// Legacy transport-specific selection retained for wire compatibility.
    #[serde(default)]
    pub antigravity_api_model_selection: ModelSelection,

    /// Legacy provider-level effort retained for wire compatibility.
    #[serde(default)]
    pub antigravity_effort: String,

    /// PDF extraction method: "llm", "auto", "paddleocr-vl-full", or
    /// "pdftotext". "auto" uses an installed Full Parser and otherwise LLM.
    /// The retired "paddleocr-vl" value is migrated to the Full Parser when
    /// older settings are deserialized.
    #[serde(
        default = "default_pdf_extractor",
        deserialize_with = "deserialize_pdf_extractor"
    )]
    pub pdf_extractor: String,

    /// Number of PDF pages PaddleOCR-VL may process concurrently. 0 selects
    /// a platform-aware default; 1 through 4 are explicit expert overrides.
    #[serde(default = "default_paddle_page_concurrency")]
    pub paddle_page_concurrency: u32,

    /// Maximum number of multimodal tokens encoded in one llama.cpp batch.
    /// 0 selects a platform-aware default. Larger batches can improve
    /// vision-prefill throughput at the cost of additional peak memory.
    #[serde(default = "default_paddle_mtmd_batch_tokens")]
    pub paddle_mtmd_batch_tokens: u32,

    /// llama.cpp Flash Attention policy: "auto", "on", or "off".
    #[serde(default = "default_paddle_flash_attention")]
    pub paddle_flash_attention: String,

    /// Maximum generated tokens for one PaddleOCR-VL page.
    #[serde(default = "default_paddle_max_output_tokens")]
    pub paddle_max_output_tokens: u32,

    /// Number of retries for a failed or suspicious PaddleOCR-VL page.
    #[serde(default = "default_paddle_page_retries")]
    pub paddle_page_retries: u32,

    /// Full-parser client controls. These are intentionally separate from
    /// llama.cpp throughput tuning because they change semantic structure and
    /// therefore participate in a different extraction-cache fingerprint.
    #[serde(default = "default_true")]
    pub paddle_full_layout_detection: bool,

    #[serde(default = "default_paddle_full_layout_threshold")]
    pub paddle_full_layout_threshold: f32,

    #[serde(default = "default_true")]
    pub paddle_full_layout_nms: bool,

    #[serde(default = "default_paddle_full_layout_merge_bboxes_mode")]
    pub paddle_full_layout_merge_bboxes_mode: String,

    #[serde(default = "default_true")]
    pub paddle_full_merge_layout_blocks: bool,

    #[serde(default = "default_true")]
    pub paddle_full_ocr_image_blocks: bool,

    #[serde(default = "default_true")]
    pub paddle_full_format_block_content: bool,

    #[serde(default = "default_true")]
    pub paddle_full_merge_tables: bool,

    #[serde(default = "default_true")]
    pub paddle_full_relevel_titles: bool,

    #[serde(default = "default_true")]
    pub paddle_full_show_formula_numbers: bool,

    /// Wall-clock budget for the complete PDF extraction stage.
    #[serde(default = "default_pdf_extraction_timeout_secs")]
    pub pdf_extraction_timeout_secs: u64,

    /// Reuse versioned local extraction checkpoints when the PDF and
    /// extractor settings are unchanged.
    #[serde(default = "default_reuse_pdf_extraction_cache")]
    pub reuse_pdf_extraction_cache: bool,

    /// Show verbose LLM output in the console (command lines, stdout, stderr).
    #[serde(default)]
    pub verbose_logging: bool,

    /// Timeout in seconds for each LLM subprocess call. Default = 1200 (20 min).
    #[serde(default = "default_timeout")]
    pub step_timeout_secs: u64,

    /// Number of times to retry a failed step before giving up. Default = 1.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Automatically compare a completed run with the newest prior run for
    /// the same input path or content hash. Disabled by default because it
    /// adds a niche, potentially costly LLM reconciliation call.
    #[serde(default)]
    pub auto_revision_reconciliation: bool,

    /// Maximum number of past runs to keep on disk. The oldest are purged after
    /// each run once the count exceeds this. 0 = keep everything (default).
    #[serde(default)]
    pub max_saved_runs: u32,

    /// Maximum total bytes retained under the run-history directory. Oldest
    /// completed runs are purged until both this and `max_saved_runs` hold.
    /// 0 disables the byte ceiling.
    #[serde(default = "default_max_saved_run_bytes")]
    pub max_saved_run_bytes: u64,

    /// Connection mode for each cloud provider. Older settings files omitted
    /// these fields and selected the API transport implicitly whenever a key
    /// was present; `normalize_access_modes` preserves that behavior once and
    /// then saves an explicit choice.
    #[serde(default)]
    pub claude_access_mode: String,

    #[serde(default)]
    pub codex_access_mode: String,

    #[serde(default)]
    pub antigravity_access_mode: String,

    /// Anthropic API key, used only when Claude is in API mode.
    #[serde(default)]
    pub anthropic_api_key: String,

    /// OpenAI API key, used only when ChatGPT is in API mode.
    #[serde(default)]
    pub openai_api_key: String,

    /// Google AI API key, used only when Antigravity is in API mode.
    #[serde(default)]
    pub google_api_key: String,

    /// Base URL of a local OpenAI-compatible server for the "local" provider.
    /// Default is Ollama's endpoint; LM Studio, llama.cpp server, and vLLM
    /// work by changing the URL.
    #[serde(default = "default_local_base_url")]
    pub local_base_url: String,

    /// Model name on the local server (e.g. "llama3.3", "qwen2.5:14b").
    /// Required for the local provider — there is no meaningful default.
    #[serde(default)]
    pub local_model: String,

    /// Optional bearer token for the local server. Most local runtimes need
    /// none; encrypted at rest like the cloud keys since users may point the
    /// base URL at remote OpenAI-compatible services.
    #[serde(default)]
    pub local_api_key: String,
}

fn default_local_base_url() -> String {
    "http://localhost:11434/v1".to_string()
}

fn default_pdf_extractor() -> String {
    "auto".to_string()
}

/// Resolve the global automatic PDF policy without introducing a fallback
/// between extractors. The chosen method remains authoritative for the run.
pub(crate) fn resolve_pdf_extractor(configured: &str) -> &str {
    if configured == "auto" {
        resolve_pdf_extractor_for_paddle_availability(
            configured,
            crate::engines::paddle_full_parser_status().is_ok(),
        )
    } else {
        configured
    }
}

fn resolve_pdf_extractor_for_paddle_availability(configured: &str, paddle_installed: bool) -> &str {
    if configured == "auto" {
        if paddle_installed {
            "paddleocr-vl-full"
        } else {
            "llm"
        }
    } else {
        configured
    }
}

fn deserialize_pdf_extractor<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(match value.as_str() {
        "paddleocr-vl" => "paddleocr-vl-full".to_string(),
        // Retired extractor from pre-0.9 builds whose "auto" policy resolved
        // marker → pdftotext; map it to the current automatic policy instead
        // of letting validate() reject the whole settings file.
        "marker" => "auto".to_string(),
        _ => value,
    })
}

fn default_paddle_page_concurrency() -> u32 {
    0
}

fn default_paddle_mtmd_batch_tokens() -> u32 {
    0
}

fn default_paddle_flash_attention() -> String {
    "auto".to_string()
}

fn default_paddle_max_output_tokens() -> u32 {
    4096
}

fn default_paddle_page_retries() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

fn default_paddle_full_layout_threshold() -> f32 {
    0.5
}

fn default_paddle_full_layout_merge_bboxes_mode() -> String {
    "large".to_string()
}

fn default_pdf_extraction_timeout_secs() -> u64 {
    1800
}

fn default_reuse_pdf_extraction_cache() -> bool {
    true
}

/// Resolve Paddle's automatic settings without persisting machine-specific
/// values. Apple Silicon generally has enough unified-memory bandwidth to
/// benefit from two slots and a larger vision-prefill batch. Conservative
/// defaults remain preferable on other platforms, where GPU availability is
/// not reliably discoverable before llama.cpp starts.
pub fn resolved_paddle_page_concurrency(settings: &Settings) -> u32 {
    match settings.paddle_page_concurrency {
        0 if cfg!(all(target_os = "macos", target_arch = "aarch64")) => 2,
        0 => 1,
        value => value,
    }
}

pub fn resolved_paddle_mtmd_batch_tokens(settings: &Settings) -> u32 {
    match settings.paddle_mtmd_batch_tokens {
        0 if cfg!(all(target_os = "macos", target_arch = "aarch64")) => 2048,
        0 => 1024,
        value => value,
    }
}

fn default_provider() -> String {
    "claude".to_string()
}

fn default_workers() -> u32 {
    16
}

fn default_timeout() -> u64 {
    1200
}

fn default_max_retries() -> u32 {
    1
}

fn default_max_saved_run_bytes() -> u64 {
    5_000_000_000
}

fn default_profile() -> String {
    "auto-review".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            preferred_provider: "claude".to_string(),
            default_parallel_agents: vec!["claude".to_string()],
            default_parallel_model_overrides: std::collections::HashMap::new(),
            default_parallel_effort_overrides: std::collections::HashMap::new(),
            default_merge_agent: "claude".to_string(),
            default_merge_model_overrides: std::collections::HashMap::new(),
            default_merge_effort_overrides: std::collections::HashMap::new(),
            default_sequential_agent: "claude".to_string(),
            default_sequential_model_overrides: std::collections::HashMap::new(),
            default_sequential_effort_overrides: std::collections::HashMap::new(),
            default_orientation_agent: "claude".to_string(),
            default_orientation_model_overrides: std::collections::HashMap::new(),
            default_orientation_effort_overrides: std::collections::HashMap::new(),
            usage_limit_fallback_agent: String::new(),
            usage_limit_fallback_model_overrides: std::collections::HashMap::new(),
            usage_limit_fallback_effort_overrides: std::collections::HashMap::new(),
            max_workers: default_workers(),
            active_profile: "auto-review".to_string(),
            claude_model: String::new(),
            claude_cli_model_selection: ModelSelection::Automatic,
            claude_api_model_selection: ModelSelection::Automatic,
            claude_effort: String::new(),
            codex_model: String::new(),
            codex_cli_model_selection: ModelSelection::Automatic,
            codex_api_model_selection: ModelSelection::Automatic,
            codex_effort: String::new(),
            antigravity_cli_model_selection: ModelSelection::Automatic,
            antigravity_api_model_selection: ModelSelection::Automatic,
            antigravity_effort: String::new(),
            pdf_extractor: default_pdf_extractor(),
            paddle_page_concurrency: default_paddle_page_concurrency(),
            paddle_mtmd_batch_tokens: default_paddle_mtmd_batch_tokens(),
            paddle_flash_attention: default_paddle_flash_attention(),
            paddle_max_output_tokens: default_paddle_max_output_tokens(),
            paddle_page_retries: default_paddle_page_retries(),
            paddle_full_layout_detection: true,
            paddle_full_layout_threshold: default_paddle_full_layout_threshold(),
            paddle_full_layout_nms: true,
            paddle_full_layout_merge_bboxes_mode: default_paddle_full_layout_merge_bboxes_mode(),
            paddle_full_merge_layout_blocks: true,
            paddle_full_ocr_image_blocks: true,
            paddle_full_format_block_content: true,
            paddle_full_merge_tables: true,
            paddle_full_relevel_titles: true,
            paddle_full_show_formula_numbers: true,
            pdf_extraction_timeout_secs: default_pdf_extraction_timeout_secs(),
            reuse_pdf_extraction_cache: default_reuse_pdf_extraction_cache(),
            verbose_logging: false,
            step_timeout_secs: 1200,
            max_retries: 1,
            auto_revision_reconciliation: false,
            max_saved_runs: 0,
            max_saved_run_bytes: default_max_saved_run_bytes(),
            claude_access_mode: "subscription".to_string(),
            codex_access_mode: "subscription".to_string(),
            antigravity_access_mode: "subscription".to_string(),
            anthropic_api_key: String::new(),
            openai_api_key: String::new(),
            google_api_key: String::new(),
            local_base_url: default_local_base_url(),
            local_model: String::new(),
            local_api_key: String::new(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.preferred_provider.as_str(),
            "claude" | "codex" | "antigravity" | "local"
        ) {
            return Err(format!(
                "Invalid preferred provider '{}'",
                self.preferred_provider
            ));
        }
        if self.default_parallel_agents.len() > 4 {
            return Err("Default Parallel agents cannot contain more than four providers".into());
        }
        for provider in self
            .default_parallel_agents
            .iter()
            .map(String::as_str)
            .chain([
                self.default_merge_agent.as_str(),
                self.default_sequential_agent.as_str(),
                self.default_orientation_agent.as_str(),
                self.usage_limit_fallback_agent.as_str(),
            ])
            .filter(|provider| !provider.is_empty())
        {
            if !matches!(provider, "claude" | "codex" | "antigravity" | "local") {
                return Err(format!("Invalid default agent provider '{provider}'"));
            }
        }
        let unique_parallel = self
            .default_parallel_agents
            .iter()
            .collect::<std::collections::HashSet<_>>();
        if unique_parallel.len() != self.default_parallel_agents.len() {
            return Err("Default Parallel agents cannot contain duplicates".into());
        }
        if !(1..=20).contains(&self.max_workers) {
            return Err("Maximum workers must be between 1 and 20".to_string());
        }
        if !(60..=7200).contains(&self.step_timeout_secs) {
            return Err("Step timeout must be between 60 and 7200 seconds".to_string());
        }
        if self.max_retries > 10 {
            return Err("Step retries must be between 0 and 10".to_string());
        }
        if self.max_saved_run_bytes > 1_000_000_000_000 {
            return Err("Run-history byte limit cannot exceed 1 TB".to_string());
        }
        for (provider, mode) in [
            ("Claude", self.claude_access_mode.as_str()),
            ("ChatGPT", self.codex_access_mode.as_str()),
            ("Antigravity", self.antigravity_access_mode.as_str()),
        ] {
            if !matches!(mode, "" | "subscription" | "api") {
                return Err(format!(
                    "Invalid {provider} access mode '{mode}'; choose subscription or api"
                ));
            }
        }
        if !matches!(
            self.pdf_extractor.as_str(),
            "llm" | "auto" | "paddleocr-vl-full" | "pdftotext"
        ) {
            return Err(format!("Invalid PDF extractor '{}'", self.pdf_extractor));
        }
        if self.paddle_page_concurrency > 4 {
            return Err(
                "PaddleOCR-VL concurrent pages must be automatic or between 1 and 4".to_string(),
            );
        }
        if !matches!(self.paddle_mtmd_batch_tokens, 0 | 512 | 1024 | 2048 | 4096) {
            return Err(
                "PaddleOCR-VL vision batch must be automatic, 512, 1024, 2048, or 4096 tokens"
                    .to_string(),
            );
        }
        if !matches!(self.paddle_flash_attention.as_str(), "auto" | "on" | "off") {
            return Err("PaddleOCR-VL Flash Attention must be auto, on, or off".to_string());
        }
        if !matches!(self.paddle_max_output_tokens, 2048 | 4096 | 8192) {
            return Err("PaddleOCR-VL page output must be 2048, 4096, or 8192 tokens".to_string());
        }
        if self.paddle_page_retries > 3 {
            return Err("PaddleOCR-VL page retries must be between 0 and 3".to_string());
        }
        if !self.paddle_full_layout_threshold.is_finite()
            || !(0.05..=0.95).contains(&self.paddle_full_layout_threshold)
        {
            return Err(
                "PaddleOCR-VL full-parser layout threshold must be between 0.05 and 0.95"
                    .to_string(),
            );
        }
        if !matches!(
            self.paddle_full_layout_merge_bboxes_mode.as_str(),
            "large" | "small" | "union"
        ) {
            return Err(
                "PaddleOCR-VL layout box merge mode must be large, small, or union".to_string(),
            );
        }
        if !(120..=7200).contains(&self.pdf_extraction_timeout_secs) {
            return Err("PDF extraction timeout must be between 120 and 7200 seconds".to_string());
        }
        if self.active_profile.is_empty()
            || self.active_profile.len() > 64
            || !self
                .active_profile
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err("Invalid active profile ID".to_string());
        }
        // Bundles produced by older Pipeline releases may contain only these
        // legacy fields. Validate them before save_unlocked migrates them into
        // the transport-specific selections, otherwise an unsafe flag-like
        // value can bypass the checks below.
        for (label, value) in [
            ("Claude", self.claude_model.as_str()),
            ("Codex", self.codex_model.as_str()),
        ] {
            if !value.is_empty() && (value.trim() != value || sanitize_cli_arg(value) != value) {
                return Err(format!("Invalid legacy {label} model selection"));
            }
        }
        for (label, selection) in [
            ("Claude CLI", &self.claude_cli_model_selection),
            ("Anthropic API", &self.claude_api_model_selection),
            ("Codex CLI", &self.codex_cli_model_selection),
            ("OpenAI API", &self.codex_api_model_selection),
            ("Antigravity CLI", &self.antigravity_cli_model_selection),
            ("Google API", &self.antigravity_api_model_selection),
        ] {
            let value = match selection {
                ModelSelection::Automatic => continue,
                ModelSelection::Role { role } => role,
                ModelSelection::Pinned { model } => model,
            };
            if value.trim() != value || sanitize_cli_arg(value) != *value {
                return Err(format!("Invalid {label} model selection"));
            }
        }
        for (key, selection) in self
            .default_parallel_model_overrides
            .iter()
            .chain(self.default_merge_model_overrides.iter())
            .chain(self.default_sequential_model_overrides.iter())
            .chain(self.default_orientation_model_overrides.iter())
            .chain(self.usage_limit_fallback_model_overrides.iter())
        {
            let valid_key = ["claude", "codex", "antigravity", "local"]
                .iter()
                .any(|provider| {
                    key == provider
                        || key == &format!("{provider}:cli")
                        || key == &format!("{provider}:api")
                });
            if !valid_key {
                return Err(format!("Invalid default model override key '{key}'"));
            }
            let value = match selection {
                ModelSelection::Automatic => continue,
                ModelSelection::Role { role } => role,
                ModelSelection::Pinned { model } => model,
            };
            if value.trim() != value || sanitize_cli_arg(value) != *value {
                return Err(format!("Invalid model selection for default agent '{key}'"));
            }
        }
        for (label, effort) in [
            ("Claude", self.claude_effort.as_str()),
            ("Codex", self.codex_effort.as_str()),
            ("Antigravity", self.antigravity_effort.as_str()),
        ]
        .into_iter()
        .chain(
            self.default_parallel_effort_overrides
                .iter()
                .map(|(key, effort)| (key.as_str(), effort.as_str())),
        )
        .chain(
            self.default_merge_effort_overrides
                .iter()
                .map(|(key, effort)| (key.as_str(), effort.as_str())),
        )
        .chain(
            self.default_sequential_effort_overrides
                .iter()
                .map(|(key, effort)| (key.as_str(), effort.as_str())),
        )
        .chain(
            self.default_orientation_effort_overrides
                .iter()
                .map(|(key, effort)| (key.as_str(), effort.as_str())),
        )
        .chain(
            self.usage_limit_fallback_effort_overrides
                .iter()
                .map(|(key, effort)| (key.as_str(), effort.as_str())),
        ) {
            if !effort.is_empty() && sanitize_cli_arg(effort) != effort {
                return Err(format!("Invalid {label} effort value"));
            }
        }
        if !self.local_model.is_empty() && sanitize_cli_arg(&self.local_model) != self.local_model {
            return Err("Invalid local model name".to_string());
        }
        validate_local_base_url(&self.local_base_url)?;
        Ok(())
    }

    /// "cli" for subscription-backed command-line providers and "api" for
    /// direct HTTP/local providers.
    pub fn model_transport(&self, provider: &str) -> &'static str {
        match provider {
            "local" => "api",
            "codex" if self.codex_access_mode == "api" => "api",
            "codex"
                if self.codex_access_mode.is_empty() && !self.openai_api_key.trim().is_empty() =>
            {
                "api"
            }
            "antigravity" if self.antigravity_access_mode == "api" => "api",
            "antigravity"
                if self.antigravity_access_mode.is_empty()
                    && !self.google_api_key.trim().is_empty() =>
            {
                "api"
            }
            "claude" | "" if self.claude_access_mode == "api" => "api",
            "claude" | ""
                if self.claude_access_mode.is_empty()
                    && !self.anthropic_api_key.trim().is_empty() =>
            {
                "api"
            }
            _ => "cli",
        }
    }

    pub fn model_context_key(&self, provider: &str) -> String {
        format!(
            "{}:{}",
            if provider.is_empty() {
                "claude"
            } else {
                provider
            },
            self.model_transport(provider)
        )
    }

    pub fn model_selection(&self, provider: &str) -> ModelSelection {
        match provider {
            "local" => {
                if self.local_model.trim().is_empty() {
                    ModelSelection::Automatic
                } else {
                    ModelSelection::Pinned {
                        model: self.local_model.trim().to_string(),
                    }
                }
            }
            // Provider-level model boxes were retired. A missing role or
            // workflow override now delegates directly to the active
            // provider/transport instead of inheriting an invisible setting.
            _ => ModelSelection::Automatic,
        }
    }

    pub fn model_effort(&self, _provider: &str) -> &str {
        // As with model selection, absent role/workflow effort now means the
        // provider's own default. The legacy provider effort fields remain in
        // the wire format only so older settings and bundles still deserialize.
        ""
    }

    pub fn parallel_agents(&self) -> Vec<String> {
        if self.default_parallel_agents.is_empty() {
            vec![self.preferred_provider.clone()]
        } else {
            self.default_parallel_agents.clone()
        }
    }

    pub fn sequential_agent(&self) -> &str {
        if self.default_sequential_agent.trim().is_empty() {
            &self.preferred_provider
        } else {
            &self.default_sequential_agent
        }
    }

    pub fn merge_agent(&self) -> &str {
        if self.default_merge_agent.trim().is_empty() {
            self.sequential_agent()
        } else {
            &self.default_merge_agent
        }
    }

    pub fn orientation_agent(&self) -> &str {
        if self.default_orientation_agent.trim().is_empty() {
            &self.preferred_provider
        } else {
            &self.default_orientation_agent
        }
    }

    fn role_model_selection(
        &self,
        overrides: &std::collections::HashMap<String, ModelSelection>,
        provider: &str,
    ) -> Option<ModelSelection> {
        overrides
            .get(&self.model_context_key(provider))
            .or_else(|| overrides.get(provider))
            .cloned()
    }

    fn role_effort(
        &self,
        overrides: &std::collections::HashMap<String, String>,
        provider: &str,
    ) -> String {
        overrides
            .get(&self.model_context_key(provider))
            .or_else(|| overrides.get(provider))
            .filter(|effort| !effort.trim().is_empty())
            .cloned()
            .unwrap_or_default()
    }

    pub fn parallel_model_selection(&self, provider: &str) -> Option<ModelSelection> {
        self.role_model_selection(&self.default_parallel_model_overrides, provider)
    }

    pub fn parallel_effort(&self, provider: &str) -> String {
        self.role_effort(&self.default_parallel_effort_overrides, provider)
    }

    pub fn sequential_model_selection(&self, provider: &str) -> Option<ModelSelection> {
        self.role_model_selection(&self.default_sequential_model_overrides, provider)
    }

    pub fn merge_model_selection(&self, provider: &str) -> Option<ModelSelection> {
        self.role_model_selection(&self.default_merge_model_overrides, provider)
            .or_else(|| {
                self.default_merge_agent
                    .trim()
                    .is_empty()
                    .then(|| self.sequential_model_selection(provider))
                    .flatten()
            })
    }

    pub fn merge_effort(&self, provider: &str) -> String {
        let configured = self.role_effort(&self.default_merge_effort_overrides, provider);
        if configured.is_empty() && self.default_merge_agent.trim().is_empty() {
            self.sequential_effort(provider)
        } else {
            configured
        }
    }

    pub fn sequential_effort(&self, provider: &str) -> String {
        self.role_effort(&self.default_sequential_effort_overrides, provider)
    }

    pub fn orientation_model_selection(&self, provider: &str) -> Option<ModelSelection> {
        self.role_model_selection(&self.default_orientation_model_overrides, provider)
    }

    pub fn orientation_effort(&self, provider: &str) -> String {
        self.role_effort(&self.default_orientation_effort_overrides, provider)
    }

    pub fn usage_limit_fallback_agent(&self) -> Option<&str> {
        let provider = self.usage_limit_fallback_agent.trim();
        (!provider.is_empty()).then_some(provider)
    }

    pub fn usage_limit_fallback_model_selection(&self, provider: &str) -> Option<ModelSelection> {
        self.role_model_selection(&self.usage_limit_fallback_model_overrides, provider)
    }

    pub fn usage_limit_fallback_effort(&self, provider: &str) -> String {
        self.role_effort(&self.usage_limit_fallback_effort_overrides, provider)
    }

    /// Preserve old model fields when settings are opened and re-saved. These
    /// selections are no longer provider-level runtime defaults.
    fn migrate_legacy_model_fields(&mut self) {
        if self.claude_cli_model_selection == ModelSelection::Automatic
            && self.claude_api_model_selection == ModelSelection::Automatic
            && !self.claude_model.trim().is_empty()
        {
            let migrated = ModelSelection::from_legacy(&self.claude_model);
            self.claude_cli_model_selection = migrated.clone();
            self.claude_api_model_selection = migrated;
        }
        if self.codex_cli_model_selection == ModelSelection::Automatic
            && self.codex_api_model_selection == ModelSelection::Automatic
            && !self.codex_model.trim().is_empty()
        {
            let migrated = ModelSelection::from_legacy(&self.codex_model);
            self.codex_cli_model_selection = migrated.clone();
            self.codex_api_model_selection = migrated;
        }
    }

    /// Keep old Pipeline builds and exported settings usable while clearing the
    /// retired provider-level default from newly saved settings.
    fn sync_legacy_model_fields(&mut self) {
        self.claude_model = self.model_selection("claude").legacy_value();
        self.codex_model = self.model_selection("codex").legacy_value();
    }

    pub fn normalized(mut self) -> Self {
        self.migrate_legacy_model_fields();
        self.normalize_access_modes();
        self.drop_unknown_providers();
        self
    }

    /// Migrate the pre-mode behavior exactly once. An absent mode follows the
    /// old rule (stored key => API, otherwise subscription); an explicit mode
    /// is never changed merely because a credential is entered or cleared.
    fn normalize_access_modes(&mut self) {
        if self.claude_access_mode.trim().is_empty() {
            self.claude_access_mode = if self.anthropic_api_key.trim().is_empty() {
                "subscription"
            } else {
                "api"
            }
            .to_string();
        }
        if self.codex_access_mode.trim().is_empty() {
            self.codex_access_mode = if self.openai_api_key.trim().is_empty() {
                "subscription"
            } else {
                "api"
            }
            .to_string();
        }
        if self.antigravity_access_mode.trim().is_empty() {
            self.antigravity_access_mode = if self.google_api_key.trim().is_empty() {
                "subscription"
            } else {
                "api"
            }
            .to_string();
        }
    }

    /// Drop provider ids this build does not support from the surviving
    /// agent-default fields and override maps. Settings written by earlier
    /// builds can still name removed providers (for example "gemini");
    /// validation on the run-launch path rejects them, and the Settings UI
    /// can neither display nor delete them, so they are normalized away at
    /// load instead.
    fn drop_unknown_providers(&mut self) {
        const KNOWN_PROVIDERS: [&str; 4] = ["claude", "codex", "antigravity", "local"];
        let known = |provider: &str| KNOWN_PROVIDERS.contains(&provider);
        if !known(&self.preferred_provider) {
            self.preferred_provider = default_provider();
        }
        self.default_parallel_agents
            .retain(|provider| known(provider));
        if !self.default_sequential_agent.is_empty() && !known(&self.default_sequential_agent) {
            self.default_sequential_agent = String::new();
        }
        if !self.default_merge_agent.is_empty() && !known(&self.default_merge_agent) {
            self.default_merge_agent = String::new();
        }
        if !self.default_orientation_agent.is_empty() && !known(&self.default_orientation_agent) {
            self.default_orientation_agent = String::new();
        }
        if !self.usage_limit_fallback_agent.is_empty() && !known(&self.usage_limit_fallback_agent) {
            self.usage_limit_fallback_agent = String::new();
        }
        let known_key = |key: &str| {
            KNOWN_PROVIDERS.iter().any(|provider| {
                key == *provider
                    || key == format!("{provider}:cli")
                    || key == format!("{provider}:api")
            })
        };
        for overrides in [
            &mut self.default_parallel_model_overrides,
            &mut self.default_merge_model_overrides,
            &mut self.default_sequential_model_overrides,
            &mut self.default_orientation_model_overrides,
            &mut self.usage_limit_fallback_model_overrides,
        ] {
            overrides.retain(|key, _| known_key(key));
        }
        for overrides in [
            &mut self.default_parallel_effort_overrides,
            &mut self.default_merge_effort_overrides,
            &mut self.default_sequential_effort_overrides,
            &mut self.default_orientation_effort_overrides,
            &mut self.usage_limit_fallback_effort_overrides,
        ] {
            overrides.retain(|key, _| known_key(key));
        }
    }
}

fn settings_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".pipeline");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create .pipeline dir: {e}"))?;
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
    Ok(dir.join("settings.json"))
}

/// Load settings with any warnings about fallback behavior.
/// Warnings indicate the user's saved settings were partially or fully ignored.
pub fn load_with_warnings() -> (Settings, Vec<String>) {
    let mut warnings = Vec::new();

    let path = match settings_path() {
        Ok(p) => p,
        Err(e) => {
            warnings.push(format!(
                "Could not locate settings file: {e}. Using defaults."
            ));
            return (Settings::default(), warnings);
        }
    };
    if !path.exists() {
        return (Settings::default(), warnings);
    }
    let content = match read_settings_file(&path) {
        Ok(c) => c,
        Err(e) => {
            warnings.push(format!(
                "Could not read settings file: {e}. Using defaults."
            ));
            return (Settings::default(), warnings);
        }
    };
    let mut settings: Settings = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(e) => {
            // Move the unparseable file aside before falling back to defaults.
            // Callers like switch_profile() do load() -> mutate -> save(); without
            // the quarantine that save would overwrite the user's settings
            // (including encrypted API keys) with defaults.
            match quarantine_corrupt_file(&path) {
                Some(backup) => warnings.push(format!(
                    "Settings file has invalid JSON: {e}. It was moved to {} — fix and rename it back, or re-enter your settings.",
                    backup.display()
                )),
                None => warnings.push(format!(
                    "Settings file has invalid JSON: {e}. Your saved settings were not loaded."
                )),
            }
            Settings::default()
        }
    };

    settings.migrate_legacy_model_fields();
    settings.normalize_access_modes();
    settings.drop_unknown_providers();

    // Decrypt API keys (plaintext values pass through for backward compat)
    match load_or_create_key() {
        Ok(key) => {
            settings.anthropic_api_key = decrypt_string(&settings.anthropic_api_key, &key)
                .unwrap_or_else(|e| {
                    warnings.push(format!(
                        "Could not decrypt Anthropic API key: {e}. The key may need to be re-entered."
                    ));
                    String::new()
                });
            settings.openai_api_key = decrypt_string(&settings.openai_api_key, &key)
                .unwrap_or_else(|e| {
                    warnings.push(format!(
                        "Could not decrypt OpenAI API key: {e}. The key may need to be re-entered."
                    ));
                    String::new()
                });
            settings.google_api_key = decrypt_string(&settings.google_api_key, &key)
                .unwrap_or_else(|e| {
                    warnings.push(format!(
                        "Could not decrypt Google API key: {e}. The key may need to be re-entered."
                    ));
                    String::new()
                });
            settings.local_api_key = decrypt_string(&settings.local_api_key, &key)
                .unwrap_or_else(|e| {
                    warnings.push(format!(
                        "Could not decrypt local-server API key: {e}. The key may need to be re-entered."
                    ));
                    String::new()
                });
        }
        Err(e) => {
            warnings.push(format!(
                "Could not load encryption key: {e}. API keys have been cleared — please re-enter them in Settings."
            ));
            settings.anthropic_api_key = String::new();
            settings.openai_api_key = String::new();
            settings.google_api_key = String::new();
            settings.local_api_key = String::new();
        }
    }

    (settings, warnings)
}

fn read_settings_file(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(path)?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(MAX_SETTINGS_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SETTINGS_BYTES {
        return Err(format!(
            "settings file exceeds the {} MB safety limit",
            MAX_SETTINGS_BYTES / 1024 / 1024
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("settings file is not UTF-8: {e}"))
}

/// Validate the OpenAI-compatible endpoint before any API key or document
/// content can be sent to it. Plain HTTP is deliberately limited to literal
/// loopback hosts; remote servers must authenticate with TLS.
pub(crate) fn validate_local_base_url(value: &str) -> Result<reqwest::Url, String> {
    let url =
        reqwest::Url::parse(value).map_err(|error| format!("Invalid local server URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err(
            "Local server URL must be an http(s) URL without credentials, query, or fragment"
                .to_string(),
        );
    }
    if url.scheme() == "http" {
        let host = url.host_str().unwrap_or_default().trim_end_matches('.');
        // `url::Url::host_str` retains brackets around IPv6 literals.
        let ip_host = host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host);
        let is_loopback = host.eq_ignore_ascii_case("localhost")
            || ip_host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| match address {
                    std::net::IpAddr::V4(address) => address.is_loopback(),
                    std::net::IpAddr::V6(address) => {
                        address.is_loopback()
                            || address
                                .to_ipv4_mapped()
                                .is_some_and(|mapped| mapped.is_loopback())
                    }
                });
        if !is_loopback {
            return Err(
                "Plain HTTP local server URLs are allowed only on loopback; use HTTPS for remote servers"
                    .to_string(),
            );
        }
    }
    Ok(url)
}

/// Load settings, discarding any warnings. Used by non-UI callers
/// (pipeline execution, etc.) where fallback to defaults is fine.
static RUN_SETTINGS: std::sync::Mutex<Option<(u64, Settings)>> = std::sync::Mutex::new(None);
static RUN_SETTINGS_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct RunSettingsGuard {
    token: u64,
}

impl Drop for RunSettingsGuard {
    fn drop(&mut self) {
        let mut snapshot = RUN_SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
        if snapshot.as_ref().map(|(token, _)| *token) == Some(self.token) {
            *snapshot = None;
        }
    }
}

pub fn freeze_for_run(settings: Settings) -> RunSettingsGuard {
    use std::sync::atomic::Ordering;
    let token = RUN_SETTINGS_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    *RUN_SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = Some((token, settings));
    RunSettingsGuard { token }
}

pub fn load() -> Settings {
    RUN_SETTINGS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|(_, settings)| settings.clone())
        .unwrap_or_else(|| load_with_warnings().0)
}

/// Load the current on-disk settings without consulting the immutable
/// execution snapshot. UI/profile mutations must use this path: otherwise an
/// edit made while a run is active can start from the run's older snapshot and
/// overwrite settings saved by another window.
pub fn load_persisted() -> Settings {
    load_with_warnings().0
}

/// Load the persisted settings for an operation that must not silently run or
/// mutate state with defaults. Unlike the UI recovery loader, this never
/// quarantines or substitutes around a malformed/decryption-failed file.
pub fn load_persisted_required() -> Result<Settings, String> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(Settings::default());
    }
    let mut settings = load_raw_settings_required(&path)?;
    let key = load_or_create_key().map_err(|e| format!("Could not load encryption key: {e}"))?;
    settings.anthropic_api_key = decrypt_string(&settings.anthropic_api_key, &key)
        .map_err(|e| format!("Could not decrypt Anthropic API key: {e}"))?;
    settings.openai_api_key = decrypt_string(&settings.openai_api_key, &key)
        .map_err(|e| format!("Could not decrypt OpenAI API key: {e}"))?;
    settings.google_api_key = decrypt_string(&settings.google_api_key, &key)
        .map_err(|e| format!("Could not decrypt Google API key: {e}"))?;
    settings.local_api_key = decrypt_string(&settings.local_api_key, &key)
        .map_err(|e| format!("Could not decrypt local-server API key: {e}"))?;
    settings.validate()?;
    Ok(settings)
}

/// Parse settings without decrypting secrets. This is the only safe starting
/// point for an active-profile-only mutation: encrypted fields can be written
/// back byte-for-byte even when the key is temporarily unavailable.
fn load_raw_settings_required(path: &std::path::Path) -> Result<Settings, String> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let content = read_settings_file(path).map_err(|e| format!("Could not read settings: {e}"))?;
    let mut settings: Settings = serde_json::from_str(&content)
        .map_err(|e| format!("Settings file is invalid JSON: {e}"))?;
    settings.migrate_legacy_model_fields();
    settings.normalize_access_modes();
    // Settings written by earlier builds may name retired providers (e.g.
    // "gemini"); validate() on the run-launch path rejects them and the UI
    // cannot repair them, so they must be normalized away on every load.
    settings.drop_unknown_providers();
    Ok(settings)
}

/// Move an unparseable settings file to `<name>.corrupt` so a subsequent
/// save() cannot destroy the user's data. Returns the backup path on success.
fn quarantine_corrupt_file(path: &std::path::Path) -> Option<PathBuf> {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".corrupt");
    let backup = PathBuf::from(backup);
    match fs::rename(path, &backup) {
        Ok(()) => Some(backup),
        Err(e) => {
            eprintln!(
                "WARNING: could not quarantine corrupt settings file {}: {e}",
                path.display()
            );
            None
        }
    }
}

static SETTINGS_WRITE_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn acquire_settings_write_lock(
    path: &std::path::Path,
) -> Result<(std::sync::MutexGuard<'static, ()>, fs::File), String> {
    let process_guard = SETTINGS_WRITE_MUTEX
        .lock()
        .map_err(|_| "Settings write mutex poisoned".to_string())?;
    let lock_path = path.with_file_name("settings.lock");
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| format!("Failed to open settings lock: {e}"))?;
    fs2::FileExt::lock_exclusive(&lock_file)
        .map_err(|e| format!("Failed to lock settings: {e}"))?;
    Ok((process_guard, lock_file))
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let path = settings_path()?;
    let (_process_guard, lock_file) = acquire_settings_write_lock(&path)?;
    let result = save_unlocked(&path, settings);
    let _ = fs2::FileExt::unlock(&lock_file);
    result
}

/// Save settings originating from the Settings page while preserving the
/// active profile selected by another window/process after that page loaded.
pub fn save_preserving_active(settings: &Settings) -> Result<(), String> {
    let path = settings_path()?;
    let (_process_guard, lock_file) = acquire_settings_write_lock(&path)?;
    let current = load_raw_settings_required(&path);
    let mut merged = settings.clone();
    let result = match current {
        Ok(current) => {
            merged.active_profile = current.active_profile.clone();
            save_unlocked_preserving_raw_secrets(&path, &merged, &current)
        }
        Err(e) => Err(e),
    };
    let _ = fs2::FileExt::unlock(&lock_file);
    result
}

/// Change only the active profile under the same process/cross-process lock as
/// normal settings writes. This prevents a profile switch from replaying a
/// stale full Settings value over a concurrent Settings-page save.
pub fn set_active_profile(active_profile: &str) -> Result<(), String> {
    let path = settings_path()?;
    let (_process_guard, lock_file) = acquire_settings_write_lock(&path)?;
    let result = load_raw_settings_required(&path).and_then(|mut current| {
        current.active_profile = active_profile.to_string();
        save_raw_unlocked(&path, &current)
    });
    let _ = fs2::FileExt::unlock(&lock_file);
    result
}

/// Replace an active profile only if it still matches `expected`. Used by
/// profile deletion so a concurrent switch to another valid profile wins.
pub fn replace_active_profile_if(expected: &str, replacement: &str) -> Result<bool, String> {
    let path = settings_path()?;
    let (_process_guard, lock_file) = acquire_settings_write_lock(&path)?;
    let result = load_raw_settings_required(&path).and_then(|mut current| {
        let changed = current.active_profile == expected;
        if changed {
            current.active_profile = replacement.to_string();
            save_raw_unlocked(&path, &current)?;
        }
        Ok(changed)
    });
    let _ = fs2::FileExt::unlock(&lock_file);
    result
}

fn save_unlocked(path: &std::path::Path, settings: &Settings) -> Result<(), String> {
    settings.validate()?;
    let key = load_or_create_key()?;

    // Encrypt API keys before writing to disk
    let mut to_save = settings.clone();
    to_save.migrate_legacy_model_fields();
    to_save.normalize_access_modes();
    to_save.sync_legacy_model_fields();
    to_save.anthropic_api_key = encrypt_string(&settings.anthropic_api_key, &key)?;
    to_save.openai_api_key = encrypt_string(&settings.openai_api_key, &key)?;
    to_save.google_api_key = encrypt_string(&settings.google_api_key, &key)?;
    to_save.local_api_key = encrypt_string(&settings.local_api_key, &key)?;

    let json =
        serde_json::to_string_pretty(&to_save).map_err(|e| format!("Failed to serialize: {e}"))?;
    atomic_write(path, &json)
}

fn save_raw_unlocked(path: &std::path::Path, settings: &Settings) -> Result<(), String> {
    settings.validate()?;
    let mut to_save = settings.clone();
    to_save.migrate_legacy_model_fields();
    to_save.normalize_access_modes();
    to_save.sync_legacy_model_fields();
    let json =
        serde_json::to_string_pretty(&to_save).map_err(|e| format!("Failed to serialize: {e}"))?;
    atomic_write(path, &json)
}

fn prepare_secret_for_save(
    plaintext: &str,
    raw: &str,
    key: &Result<[u8; KEY_SIZE], String>,
    label: &str,
) -> Result<String, String> {
    match key {
        Ok(key) => {
            // An empty UI value after a decryption failure must not erase the
            // ciphertext. If the current ciphertext decrypts, an empty value
            // is an intentional clear and remains empty.
            if plaintext.is_empty()
                && !raw.is_empty()
                && raw.starts_with(ENC_PREFIX)
                && decrypt_string(raw, key).is_err()
            {
                Ok(raw.to_string())
            } else {
                encrypt_string(plaintext, key)
            }
        }
        Err(_) if plaintext.is_empty() && !raw.is_empty() => Ok(raw.to_string()),
        Err(_) if plaintext.is_empty() => Ok(String::new()),
        Err(error) => Err(format!(
            "Could not encrypt {label} API key because the encryption key is unavailable: {error}"
        )),
    }
}

fn save_unlocked_preserving_raw_secrets(
    path: &std::path::Path,
    settings: &Settings,
    raw: &Settings,
) -> Result<(), String> {
    settings.validate()?;
    let key = load_or_create_key();
    let mut to_save = settings.clone();
    to_save.migrate_legacy_model_fields();
    to_save.normalize_access_modes();
    to_save.sync_legacy_model_fields();

    to_save.anthropic_api_key = prepare_secret_for_save(
        &settings.anthropic_api_key,
        &raw.anthropic_api_key,
        &key,
        "Anthropic",
    )?;
    to_save.openai_api_key = prepare_secret_for_save(
        &settings.openai_api_key,
        &raw.openai_api_key,
        &key,
        "OpenAI",
    )?;
    to_save.google_api_key = prepare_secret_for_save(
        &settings.google_api_key,
        &raw.google_api_key,
        &key,
        "Google",
    )?;
    to_save.local_api_key = prepare_secret_for_save(
        &settings.local_api_key,
        &raw.local_api_key,
        &key,
        "local-server",
    )?;

    let json =
        serde_json::to_string_pretty(&to_save).map_err(|e| format!("Failed to serialize: {e}"))?;
    atomic_write(path, &json)
}

mod crypto;

use crypto::*;

#[cfg(test)]
mod tests;

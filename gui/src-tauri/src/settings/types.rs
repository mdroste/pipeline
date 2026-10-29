//! Flat persisted settings, model-selection records, and representation defaults.

use serde::{Deserialize, Serialize};

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

    /// Queued reviews started at the same time; the queue clamps it to 1–4.
    #[serde(default = "default_max_concurrent_reviews")]
    pub max_concurrent_reviews: u32,

    /// Per-Review limits checked before each wave; 0 means no limit. Cost is
    /// the API list-price estimate of direct-API calls only.
    #[serde(default)]
    pub review_max_cost_usd: f64,
    #[serde(default)]
    pub review_max_calls: u32,
    #[serde(default)]
    pub review_max_minutes: u32,

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
    /// Native Workflow backend. App Server is the standard subscription path;
    /// the legacy CLI remains an explicit advanced compatibility option.
    #[serde(default = "default_codex_backend")]
    pub codex_backend: String,
    /// Version 1 retires the former implicit legacy-CLI default once, while
    /// preserving an advanced backend choice made after that migration.
    #[serde(default)]
    pub codex_backend_preference_version: u32,

    /// Google is API-only: Google's Antigravity terms do not permit
    /// third-party software to use an Antigravity sign-in, so subscription
    /// mode is disabled and any stored value normalizes to "api".
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

pub(super) fn default_provider() -> String {
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

fn default_max_concurrent_reviews() -> u32 {
    2
}

fn default_max_saved_run_bytes() -> u64 {
    0
}

fn default_profile() -> String {
    "auto-paper-review".to_string()
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
            active_profile: default_profile(),
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
            max_concurrent_reviews: default_max_concurrent_reviews(),
            review_max_cost_usd: 0.0,
            review_max_calls: 0,
            review_max_minutes: 0,
            auto_revision_reconciliation: false,
            max_saved_runs: 0,
            max_saved_run_bytes: default_max_saved_run_bytes(),
            claude_access_mode: "subscription".to_string(),
            codex_access_mode: "subscription".to_string(),
            codex_backend: default_codex_backend(),
            codex_backend_preference_version: 1,
            antigravity_access_mode: "api".to_string(),
            anthropic_api_key: String::new(),
            openai_api_key: String::new(),
            google_api_key: String::new(),
            local_base_url: default_local_base_url(),
            local_model: String::new(),
            local_api_key: String::new(),
        }
    }
}

pub(super) fn default_codex_backend() -> String {
    "app_server".into()
}

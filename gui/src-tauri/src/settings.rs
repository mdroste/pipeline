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
            "sonnet" | "opus" | "haiku" | "fable" | "pro" | "flash" | "flash-lite" => Self::Role {
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
    /// Preferred LLM provider: "claude", "codex", or "gemini".
    /// Used for steps that don't specify an explicit agent. Default = "claude".
    #[serde(default = "default_provider", alias = "llm_provider")]
    pub preferred_provider: String,

    /// Max concurrent referee passes (1-10).
    #[serde(default = "default_workers")]
    pub max_workers: u32,

    /// Active pipeline profile ID.
    #[serde(default = "default_profile")]
    pub active_profile: String,

    /// Claude model to use. Empty = Claude Code default.
    /// Examples: "sonnet", "opus", "haiku", or a full model ID.
    #[serde(default)]
    pub claude_model: String,

    /// Claude Code CLI selection. Separate from the API selection because
    /// subscription entitlements and accepted IDs can differ.
    #[serde(default)]
    pub claude_cli_model_selection: ModelSelection,

    /// Anthropic API selection.
    #[serde(default)]
    pub claude_api_model_selection: ModelSelection,

    /// Thinking effort level. Empty = Claude Code default.
    /// Options: "low", "medium", "high", "max".
    #[serde(default)]
    pub claude_effort: String,

    /// Codex model to use. Empty = Codex default.
    /// Examples: "gpt-5.6-terra", "o3", "gpt-4.1", or a full model ID.
    #[serde(default)]
    pub codex_model: String,

    /// Codex CLI selection (ChatGPT subscription transport).
    #[serde(default)]
    pub codex_cli_model_selection: ModelSelection,

    /// OpenAI API selection.
    #[serde(default)]
    pub codex_api_model_selection: ModelSelection,

    /// Codex reasoning effort level. Empty = Codex default.
    /// Options: "low", "medium", "high".
    #[serde(default)]
    pub codex_effort: String,

    /// Gemini model to use. Empty = Gemini CLI default.
    /// Examples: "gemini-3.1-pro-preview", "gemini-3.6-flash", or a full model ID.
    #[serde(default)]
    pub gemini_model: String,

    /// Gemini CLI selection.
    #[serde(default)]
    pub gemini_cli_model_selection: ModelSelection,

    /// Google Gemini API selection.
    #[serde(default)]
    pub gemini_api_model_selection: ModelSelection,

    /// PDF extraction method: "llm", "auto", "paddleocr-vl", or "pdftotext".
    /// The legacy value "marker" remains deserializable so the UI can explain
    /// why the user must choose a supported replacement.
    #[serde(default = "default_pdf_extractor")]
    pub pdf_extractor: String,

    /// Retired Marker setting retained only for settings-file compatibility.
    #[serde(default)]
    pub marker_disable_ocr: bool,

    /// Retired Marker setting retained only for settings-file compatibility.
    #[serde(default)]
    pub marker_force_ocr: bool,

    /// Retired Marker setting retained only for settings-file compatibility.
    #[serde(default)]
    pub marker_disable_images: bool,

    /// Retired Marker setting retained only for settings-file compatibility.
    #[serde(default = "default_marker_lowres_dpi")]
    pub marker_lowres_dpi: u32,

    /// Retired Marker setting retained only for settings-file compatibility.
    #[serde(default = "default_marker_highres_dpi")]
    pub marker_highres_dpi: u32,

    /// Retired Marker setting retained only for settings-file compatibility.
    #[serde(default = "default_marker_pdftext_workers")]
    pub marker_pdftext_workers: u32,

    /// Retired Marker setting retained only for settings-file compatibility.
    #[serde(default = "default_marker_layout_batch_size")]
    pub marker_layout_batch_size: u32,

    /// Retired Marker setting retained only for settings-file compatibility.
    #[serde(default = "default_marker_recognition_batch_size")]
    pub marker_recognition_batch_size: u32,

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

    /// DPI used to render each page before PaddleOCR-VL inference.
    #[serde(default = "default_paddle_render_dpi")]
    pub paddle_render_dpi: u32,

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

    /// Anthropic API key. When set, bypasses Claude CLI for direct API calls.
    #[serde(default)]
    pub anthropic_api_key: String,

    /// OpenAI API key. When set, bypasses Codex CLI for direct API calls.
    #[serde(default)]
    pub openai_api_key: String,

    /// Google AI API key. When set, bypasses Gemini CLI for direct API calls.
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
    "llm".to_string()
}

fn default_marker_lowres_dpi() -> u32 {
    96
}

fn default_marker_highres_dpi() -> u32 {
    192
}

fn default_marker_pdftext_workers() -> u32 {
    0
}

fn default_marker_layout_batch_size() -> u32 {
    0
}

fn default_marker_recognition_batch_size() -> u32 {
    0
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

fn default_paddle_render_dpi() -> u32 {
    150
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
    5
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
    "deep-review".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            preferred_provider: "claude".to_string(),
            max_workers: 5,
            active_profile: "deep-review".to_string(),
            claude_model: String::new(),
            claude_cli_model_selection: ModelSelection::Automatic,
            claude_api_model_selection: ModelSelection::Automatic,
            claude_effort: String::new(),
            codex_model: String::new(),
            codex_cli_model_selection: ModelSelection::Automatic,
            codex_api_model_selection: ModelSelection::Automatic,
            codex_effort: String::new(),
            gemini_model: String::new(),
            gemini_cli_model_selection: ModelSelection::Automatic,
            gemini_api_model_selection: ModelSelection::Automatic,
            pdf_extractor: "llm".to_string(),
            marker_disable_ocr: false,
            marker_force_ocr: false,
            marker_disable_images: false,
            marker_lowres_dpi: default_marker_lowres_dpi(),
            marker_highres_dpi: default_marker_highres_dpi(),
            marker_pdftext_workers: default_marker_pdftext_workers(),
            marker_layout_batch_size: default_marker_layout_batch_size(),
            marker_recognition_batch_size: default_marker_recognition_batch_size(),
            paddle_page_concurrency: default_paddle_page_concurrency(),
            paddle_mtmd_batch_tokens: default_paddle_mtmd_batch_tokens(),
            paddle_flash_attention: default_paddle_flash_attention(),
            paddle_max_output_tokens: default_paddle_max_output_tokens(),
            paddle_page_retries: default_paddle_page_retries(),
            paddle_render_dpi: default_paddle_render_dpi(),
            pdf_extraction_timeout_secs: default_pdf_extraction_timeout_secs(),
            reuse_pdf_extraction_cache: default_reuse_pdf_extraction_cache(),
            verbose_logging: false,
            step_timeout_secs: 1200,
            max_retries: 1,
            auto_revision_reconciliation: false,
            max_saved_runs: 0,
            max_saved_run_bytes: default_max_saved_run_bytes(),
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
            "claude" | "codex" | "gemini" | "local"
        ) {
            return Err(format!(
                "Invalid preferred provider '{}'",
                self.preferred_provider
            ));
        }
        if !(1..=10).contains(&self.max_workers) {
            return Err("Maximum workers must be between 1 and 10".to_string());
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
        if !matches!(
            self.pdf_extractor.as_str(),
            "llm" | "auto" | "marker" | "paddleocr-vl" | "pdftotext"
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
        if !matches!(self.paddle_render_dpi, 120 | 150 | 180 | 200) {
            return Err(
                "PaddleOCR-VL render resolution must be 120, 150, 180, or 200 DPI".to_string(),
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
            ("Gemini", self.gemini_model.as_str()),
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
            ("Gemini CLI", &self.gemini_cli_model_selection),
            ("Google API", &self.gemini_api_model_selection),
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
        for (label, effort) in [
            ("Claude", self.claude_effort.as_str()),
            ("Codex", self.codex_effort.as_str()),
        ] {
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
            "claude" | "" if self.anthropic_api_key.trim().is_empty() => "cli",
            "codex" if self.openai_api_key.trim().is_empty() => "cli",
            "gemini" if self.google_api_key.trim().is_empty() => "cli",
            _ => "api",
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
        match (provider, self.model_transport(provider)) {
            ("codex", "cli") => self.codex_cli_model_selection.clone(),
            ("codex", _) => self.codex_api_model_selection.clone(),
            ("gemini", "cli") => self.gemini_cli_model_selection.clone(),
            ("gemini", _) => self.gemini_api_model_selection.clone(),
            ("local", _) => {
                if self.local_model.trim().is_empty() {
                    ModelSelection::Automatic
                } else {
                    ModelSelection::Pinned {
                        model: self.local_model.trim().to_string(),
                    }
                }
            }
            (_, "cli") => self.claude_cli_model_selection.clone(),
            _ => self.claude_api_model_selection.clone(),
        }
    }

    pub fn model_effort(&self, provider: &str) -> &str {
        match provider {
            "codex" => &self.codex_effort,
            "claude" | "" => &self.claude_effort,
            _ => "",
        }
    }

    /// Import the three legacy free-text model fields once. New fields win if
    /// either transport already contains an explicit selection.
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
        if self.gemini_cli_model_selection == ModelSelection::Automatic
            && self.gemini_api_model_selection == ModelSelection::Automatic
            && !self.gemini_model.trim().is_empty()
        {
            let migrated = ModelSelection::from_legacy(&self.gemini_model);
            self.gemini_cli_model_selection = migrated.clone();
            self.gemini_api_model_selection = migrated;
        }
    }

    /// Keep old Pipeline builds and exported settings usable. The legacy field
    /// mirrors whichever transport is active; the transport-specific fields
    /// remain the source of truth for this build.
    fn sync_legacy_model_fields(&mut self) {
        self.claude_model = self.model_selection("claude").legacy_value();
        self.codex_model = self.model_selection("codex").legacy_value();
        self.gemini_model = self.model_selection("gemini").legacy_value();
    }

    pub fn normalized(mut self) -> Self {
        self.migrate_legacy_model_fields();
        self
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

// ── Encryption helpers ─────────────────────────────────────────────

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};

const NONCE_SIZE: usize = 12;
const KEY_SIZE: usize = 32;
const ENC_PREFIX: &str = "enc:";

/// Cached encryption key to prevent TOCTOU race conditions.
/// The Mutex ensures only one thread loads/creates the key at a time.
static CACHED_KEY: std::sync::Mutex<Option<[u8; KEY_SIZE]>> = std::sync::Mutex::new(None);

/// Load the encryption key from ~/.pipeline/keyfile, creating it on first use.
/// Uses an in-memory cache to prevent race conditions when multiple async
/// tasks call load() or save() concurrently.
fn load_or_create_key() -> Result<[u8; KEY_SIZE], String> {
    let mut cached = CACHED_KEY
        .lock()
        .map_err(|_| "Encryption key mutex poisoned".to_string())?;
    if let Some(key) = *cached {
        return Ok(key);
    }
    let key = load_or_create_key_inner()?;
    *cached = Some(key);
    Ok(key)
}

fn read_keyfile(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(path)?;
    let mut bytes = Vec::with_capacity(KEY_SIZE + 1);
    file.take(KEY_SIZE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read keyfile: {error}"))?;
    Ok(bytes)
}

fn load_or_create_key_inner() -> Result<[u8; KEY_SIZE], String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let path = home.join(".pipeline").join("keyfile");

    if path.exists() {
        let bytes = read_keyfile(&path)?;
        if bytes.len() == KEY_SIZE {
            let mut key = [0u8; KEY_SIZE];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
        // Wrong size — the existing keyfile is corrupt. Overwriting it would
        // permanently lose the ability to decrypt any previously-saved API keys,
        // so preserve it (as keyfile.corrupt-<timestamp>) and fail loudly.
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup = path.with_file_name(format!("keyfile.corrupt-{ts}"));
        fs::rename(&path, &backup).map_err(|e| {
            format!(
                "Keyfile at {} has wrong size ({} bytes, expected {}). \
                 Failed to back it up to {}: {e}. \
                 Refusing to overwrite — move or delete it manually to regenerate.",
                path.display(),
                bytes.len(),
                KEY_SIZE,
                backup.display()
            )
        })?;
        return Err(format!(
            "Keyfile at {} had wrong size ({} bytes, expected {}); \
             backed up to {}. A new keyfile will be generated on next save, \
             but previously-saved API keys will no longer decrypt and must be re-entered.",
            path.display(),
            bytes.len(),
            KEY_SIZE,
            backup.display()
        ));
    }

    let mut key = [0u8; KEY_SIZE];
    getrandom::getrandom(&mut key)
        .map_err(|e| format!("Failed to generate encryption key: {e}"))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create .pipeline dir: {e}"))?;
    }
    // Publish a complete key with no-clobber semantics. Two first-run
    // processes may both generate candidates; exactly one wins the atomic
    // persist and every loser reads that winner instead of caching its own
    // now-orphaned key.
    use std::io::Write as _;
    let parent = path
        .parent()
        .ok_or_else(|| format!("No parent directory for {}", path.display()))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("Failed to create keyfile temporary file: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("Failed to restrict keyfile permissions: {e}"))?;
    }
    temp.write_all(&key)
        .map_err(|e| format!("Failed to write keyfile: {e}"))?;
    temp.flush()
        .map_err(|e| format!("Failed to flush keyfile: {e}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync keyfile: {e}"))?;
    let created = match temp.persist_noclobber(&path) {
        Ok(_) => true,
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            let bytes =
                read_keyfile(&path).map_err(|e| format!("Failed to read winning keyfile: {e}"))?;
            if bytes.len() != KEY_SIZE {
                return Err(format!(
                    "Winning keyfile has wrong size ({} bytes, expected {KEY_SIZE})",
                    bytes.len()
                ));
            }
            key.copy_from_slice(&bytes);
            false
        }
        Err(error) => return Err(format!("Failed to publish keyfile: {}", error.error)),
    };

    #[cfg(windows)]
    if created {
        // Best-effort: restrict keyfile to current user via icacls.
        // Inheritance from %USERPROFILE% usually provides this already,
        // but this makes it explicit on non-standard directory layouts.
        // icacls is invoked without a shell, so %USERNAME% would be passed
        // literally — resolve it in Rust first.
        if let Some(path_str) = path.to_str() {
            if let Ok(username) = std::env::var("USERNAME") {
                if !username.is_empty() {
                    use std::os::windows::process::CommandExt;
                    let grant = format!("{username}:F");
                    let result = std::process::Command::new("icacls")
                        .args([path_str, "/inheritance:r", "/grant:r", &grant])
                        .creation_flags(0x08000000) // CREATE_NO_WINDOW
                        .output();
                    match result {
                        Ok(out) if !out.status.success() => eprintln!(
                            "WARNING: icacls could not restrict keyfile permissions: {}",
                            String::from_utf8_lossy(&out.stderr).trim()
                        ),
                        Err(e) => eprintln!(
                            "WARNING: could not run icacls to restrict keyfile permissions: {e}"
                        ),
                        _ => {}
                    }
                }
            }
        }
    }
    #[cfg(not(windows))]
    let _ = created;

    Ok(key)
}

/// Encrypt a string with AES-256-GCM. Returns "enc:<base64(nonce+ciphertext)>".
/// Empty strings pass through unchanged.
fn encrypt_string(plaintext: &str, key: &[u8; KEY_SIZE]) -> Result<String, String> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    getrandom::getrandom(&mut nonce_bytes).map_err(|e| format!("RNG failed: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {e}"))?;

    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(format!("{}{}", ENC_PREFIX, STANDARD.encode(&combined)))
}

/// Decrypt an "enc:..." string. Plaintext strings (no prefix) pass through
/// unchanged, providing backward compatibility with existing settings files.
fn decrypt_string(stored: &str, key: &[u8; KEY_SIZE]) -> Result<String, String> {
    if stored.is_empty() {
        return Ok(String::new());
    }
    if !stored.starts_with(ENC_PREFIX) {
        // Legacy plaintext value — return as-is (will be encrypted on next save)
        return Ok(stored.to_string());
    }

    let b64 = &stored[ENC_PREFIX.len()..];
    let combined = STANDARD
        .decode(b64)
        .map_err(|e| format!("Base64 decode failed: {e}"))?;
    if combined.len() < NONCE_SIZE + 1 {
        return Err("Encrypted data too short".into());
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_SIZE);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed — keyfile may have been deleted or replaced".to_string())?;

    String::from_utf8(plaintext).map_err(|e| format!("Decrypted text is not valid UTF-8: {e}"))
}

// ── File I/O ───────────────────────────────────────────────────────

/// Write to a uniquely-named temp sibling then rename, so a crash mid-write can't
/// corrupt the file and concurrent writers can't step on each other's temp file.
/// On Unix, tempfile creates the file with 0o600 by default (owner-only), which
/// is what we want since settings may contain encrypted API keys.
fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), String> {
    use std::io::Write as _;
    let dir = path
        .parent()
        .ok_or_else(|| format!("No parent dir for {}", path.display()))?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| format!("Failed to create temp file in {}: {e}", dir.display()))?;
    tmp.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write {}: {e}", tmp.path().display()))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush {}: {e}", tmp.path().display()))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync {}: {e}", tmp.path().display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Surface failures: on filesystems that can't honor 0o600, the settings
        // file would otherwise be world-readable silently.
        fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o600)).map_err(|e| {
            format!(
                "Failed to tighten permissions on {}: {e}",
                tmp.path().display()
            )
        })?;
    }
    tmp.persist(path)
        .map_err(|e| format!("Failed to save {}: {}", path.display(), e.error))?;
    #[cfg(unix)]
    fs::File::open(dir)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| format!("Failed to sync settings directory: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_reconciliation_is_opt_in_for_new_and_existing_settings() {
        assert!(!Settings::default().auto_revision_reconciliation);
        let legacy: Settings = serde_json::from_str("{}").unwrap();
        assert!(!legacy.auto_revision_reconciliation);
    }

    #[test]
    fn retired_marker_values_remain_loadable_for_explicit_repair() {
        let legacy: Settings = serde_json::from_str(
            r#"{
                "pdf_extractor": "marker",
                "marker_disable_ocr": true,
                "marker_force_ocr": true,
                "marker_lowres_dpi": 999
            }"#,
        )
        .unwrap();
        assert_eq!(legacy.pdf_extractor, "marker");
        assert!(legacy.validate().is_ok());
    }

    #[test]
    fn paddle_tuning_defaults_are_automatic_and_bounded() {
        let defaults = Settings::default();
        assert_eq!(defaults.paddle_page_concurrency, 0);
        assert_eq!(defaults.paddle_mtmd_batch_tokens, 0);
        assert_eq!(defaults.paddle_flash_attention, "auto");
        assert_eq!(defaults.paddle_max_output_tokens, 4096);
        assert_eq!(defaults.paddle_page_retries, 1);
        assert_eq!(defaults.paddle_render_dpi, 150);
        assert_eq!(defaults.pdf_extraction_timeout_secs, 1800);
        assert!(defaults.reuse_pdf_extraction_cache);

        let legacy: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.paddle_page_concurrency, 0);
        assert_eq!(legacy.paddle_mtmd_batch_tokens, 0);
        assert_eq!(legacy.paddle_flash_attention, "auto");
        assert_eq!(legacy.paddle_max_output_tokens, 4096);
        assert_eq!(legacy.paddle_page_retries, 1);
        assert_eq!(legacy.paddle_render_dpi, 150);
        assert_eq!(legacy.pdf_extraction_timeout_secs, 1800);
        assert!(legacy.reuse_pdf_extraction_cache);
    }

    #[test]
    fn paddle_automatic_settings_resolve_to_supported_values() {
        let defaults = Settings::default();
        assert!(matches!(resolved_paddle_page_concurrency(&defaults), 1 | 2));
        assert!(matches!(
            resolved_paddle_mtmd_batch_tokens(&defaults),
            1024 | 2048
        ));

        let explicit = Settings {
            paddle_page_concurrency: 1,
            paddle_mtmd_batch_tokens: 512,
            ..Default::default()
        };
        assert_eq!(resolved_paddle_page_concurrency(&explicit), 1);
        assert_eq!(resolved_paddle_mtmd_batch_tokens(&explicit), 512);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let mut key = [0u8; KEY_SIZE];
        getrandom::getrandom(&mut key).unwrap();

        let original = "sk-ant-api03-test-key-12345";
        let encrypted = encrypt_string(original, &key).unwrap();

        assert!(encrypted.starts_with(ENC_PREFIX));
        assert_ne!(encrypted, original);

        let decrypted = decrypt_string(&encrypted, &key).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_empty_string_passthrough() {
        let key = [0u8; KEY_SIZE];
        assert_eq!(encrypt_string("", &key).unwrap(), "");
        assert_eq!(decrypt_string("", &key).unwrap(), "");
    }

    #[test]
    fn quarantine_moves_corrupt_file_aside() {
        // Regression: a corrupt settings.json used to be silently replaced by
        // defaults, and the next save() (e.g. via switch_profile) overwrote the
        // user's settings — including encrypted API keys — permanently.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{not json").unwrap();

        let backup = quarantine_corrupt_file(&path).expect("quarantine should succeed");

        assert!(!path.exists());
        assert_eq!(backup, dir.path().join("settings.json.corrupt"));
        assert_eq!(fs::read_to_string(&backup).unwrap(), "{not json");
    }

    #[test]
    fn test_plaintext_passthrough() {
        let key = [0u8; KEY_SIZE];
        // Legacy plaintext value (no enc: prefix) should pass through decrypt unchanged
        assert_eq!(
            decrypt_string("sk-plain-key", &key).unwrap(),
            "sk-plain-key"
        );
    }

    #[test]
    fn test_wrong_key_fails() {
        let mut key1 = [0u8; KEY_SIZE];
        let mut key2 = [0u8; KEY_SIZE];
        getrandom::getrandom(&mut key1).unwrap();
        getrandom::getrandom(&mut key2).unwrap();

        let encrypted = encrypt_string("secret", &key1).unwrap();
        assert!(decrypt_string(&encrypted, &key2).is_err());
    }

    #[test]
    fn unreadable_ciphertext_is_preserved_during_unrelated_save() {
        let key = Ok([7u8; KEY_SIZE]);
        let raw = "enc:not-valid-base64";
        assert_eq!(prepare_secret_for_save("", raw, &key, "test").unwrap(), raw);

        let missing_key = Err("key unavailable".to_string());
        assert_eq!(
            prepare_secret_for_save("", raw, &missing_key, "test").unwrap(),
            raw
        );
        assert!(prepare_secret_for_save("new-key", raw, &missing_key, "test").is_err());
    }

    #[test]
    fn raw_profile_mutation_does_not_reencrypt_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings {
            active_profile: "quick-review".to_string(),
            anthropic_api_key: "enc:opaque-ciphertext".to_string(),
            ..Default::default()
        };
        save_raw_unlocked(&path, &settings).unwrap();

        settings = load_raw_settings_required(&path).unwrap();
        assert_eq!(settings.active_profile, "quick-review");
        assert_eq!(settings.anthropic_api_key, "enc:opaque-ciphertext");
    }

    #[test]
    fn settings_validation_rejects_unsafe_or_out_of_range_values() {
        assert!(Settings::default().validate().is_ok());

        let mut invalid = Settings {
            max_workers: 0,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        invalid = Settings {
            codex_cli_model_selection: ModelSelection::Pinned {
                model: "--dangerous-flag".into(),
            },
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        invalid = Settings {
            local_base_url: "file:///tmp/model".into(),
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        invalid = Settings {
            claude_model: "--dangerous-flag".into(),
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        invalid = Settings {
            active_profile: "profile.with.dots".into(),
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        let retired_marker_values = Settings {
            marker_disable_ocr: true,
            marker_force_ocr: true,
            marker_lowres_dpi: u32::MAX,
            marker_highres_dpi: 0,
            marker_pdftext_workers: u32::MAX,
            marker_layout_batch_size: u32::MAX,
            marker_recognition_batch_size: u32::MAX,
            ..Default::default()
        };
        assert!(
            retired_marker_values.validate().is_ok(),
            "retired Marker tuning must remain loadable but has no executable effect"
        );

        invalid = Settings {
            paddle_page_concurrency: 5,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        invalid = Settings {
            paddle_mtmd_batch_tokens: 1536,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        invalid = Settings {
            paddle_flash_attention: "sometimes".into(),
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        invalid = Settings {
            paddle_max_output_tokens: 3072,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        invalid = Settings {
            paddle_page_retries: 4,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        invalid = Settings {
            paddle_render_dpi: 160,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        invalid = Settings {
            pdf_extraction_timeout_secs: 60,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn plaintext_local_api_urls_are_loopback_only() {
        for url in [
            "http://localhost:11434/v1",
            "http://localhost.:11434/v1",
            "http://127.0.0.1:1234/v1",
            "http://127.42.0.9:1234/v1",
            "http://[::1]:1234/v1",
            "http://[::ffff:127.0.0.1]:1234/v1",
        ] {
            assert!(validate_local_base_url(url).is_ok(), "{url}");
        }
        for url in [
            "http://models.example.com/v1",
            "http://192.168.1.10:1234/v1",
            "http://10.0.0.2:1234/v1",
            "http://[fd00::1]:1234/v1",
        ] {
            assert!(validate_local_base_url(url).is_err(), "{url}");
        }
        assert!(validate_local_base_url("https://models.example.com/v1").is_ok());
    }

    #[test]
    fn settings_reader_rejects_oversized_files() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        temp.as_file()
            .set_len(MAX_SETTINGS_BYTES as u64 + 1)
            .unwrap();
        assert!(read_settings_file(temp.path()).is_err());
    }

    // ── sanitize_cli_arg ──────────────────────────────────────────

    #[test]
    fn sanitize_normal_values() {
        assert_eq!(sanitize_cli_arg("sonnet"), "sonnet");
        assert_eq!(sanitize_cli_arg("o4-mini"), "o4-mini");
        assert_eq!(sanitize_cli_arg("gemini-2.5-pro"), "gemini-2.5-pro");
        assert_eq!(sanitize_cli_arg("high"), "high");
    }

    #[test]
    fn sanitize_trims_whitespace() {
        assert_eq!(sanitize_cli_arg("  sonnet  "), "sonnet");
    }

    #[test]
    fn sanitize_rejects_flag_like() {
        assert_eq!(sanitize_cli_arg("--dangerouslySkipPermissions"), "");
        assert_eq!(sanitize_cli_arg("-p"), "");
    }

    #[test]
    fn sanitize_rejects_control_chars() {
        assert_eq!(sanitize_cli_arg("sonnet\n--bad"), "");
        assert_eq!(sanitize_cli_arg("sonnet\0"), "");
    }

    #[test]
    fn sanitize_rejects_empty() {
        assert_eq!(sanitize_cli_arg(""), "");
        assert_eq!(sanitize_cli_arg("   "), "");
    }

    #[test]
    fn legacy_models_migrate_to_roles_or_pins() {
        assert_eq!(
            ModelSelection::from_legacy("sonnet"),
            ModelSelection::Role {
                role: "sonnet".into()
            }
        );
        assert_eq!(
            ModelSelection::from_legacy("gpt-5.6-sol"),
            ModelSelection::Pinned {
                model: "gpt-5.6-sol".into()
            }
        );
        assert_eq!(ModelSelection::from_legacy(""), ModelSelection::Automatic);
        assert_eq!(
            ModelSelection::from_legacy("auto"),
            ModelSelection::Automatic
        );
    }

    #[test]
    fn api_key_switches_to_independent_api_selection() {
        let mut settings = Settings {
            codex_cli_model_selection: ModelSelection::Role {
                role: "balanced".into(),
            },
            codex_api_model_selection: ModelSelection::Pinned {
                model: "gpt-api-only".into(),
            },
            ..Default::default()
        };
        assert_eq!(settings.model_transport("codex"), "cli");
        assert_eq!(settings.model_selection("codex").label(), "balanced role");
        settings.openai_api_key = "secret".into();
        assert_eq!(settings.model_transport("codex"), "api");
        assert_eq!(settings.model_selection("codex").label(), "gpt-api-only");
    }
}

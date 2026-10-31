//! Ordered settings validation and native argument/endpoint restrictions.

use super::types::{ModelSelection, Settings};

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

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.codex_backend.as_str(), "legacy_cli" | "app_server") {
            return Err("Unsupported Workflow Codex backend".into());
        }
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
        ] {
            if !matches!(mode, "" | "subscription" | "api") {
                return Err(format!(
                    "Invalid {provider} access mode '{mode}'; choose subscription or api"
                ));
            }
        }
        if !matches!(self.antigravity_access_mode.as_str(), "" | "api") {
            return Err(
                "Antigravity subscription mode is disabled: Google's Antigravity terms do not \
                 permit third-party software to use an Antigravity sign-in. Use API mode with a \
                 Gemini API key."
                    .to_string(),
            );
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
        if self.default_model.trim() != self.default_model
            || sanitize_cli_arg(&self.default_model) != self.default_model
        {
            return Err("Invalid default model".to_string());
        }
        if sanitize_cli_arg(&self.default_effort) != self.default_effort {
            return Err("Invalid default thinking level".to_string());
        }
        validate_local_base_url(&self.local_base_url)?;
        Ok(())
    }
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

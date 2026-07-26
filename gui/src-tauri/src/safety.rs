//! Shared resource and filesystem safety primitives.
//!
//! Keep limits here so imported profiles, generated artifacts, and local file
//! handoffs are governed by one consistent policy across the GUI and CLI.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

static SPAN_TAG_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?is)<\s*/?\s*span\b(?:[^>"']|"[^"]*"|'[^']*')*>"#)
        .expect("span-tag regex must compile")
});

pub const MAX_EXPANDED_PROMPT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RUNTIME_VALUE_BYTES: usize = 1024 * 1024;
pub const MAX_RUNTIME_CONTEXT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RUN_STEP_UNITS: u64 = 2_000;
pub const MAX_RUN_PROVIDER_ATTEMPTS: u64 = 5_000;
pub const MAX_RUN_OUTPUT_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_RUNTIME_VALUES: usize = 256;

pub const MAX_WALK_ENTRIES: usize = 100_000;
pub const MAX_WALK_DIRS: usize = 10_000;
pub const MAX_WALK_TIME: Duration = Duration::from_secs(5);

/// Remove HTML span wrappers from model-facing text without altering their
/// contents. Span IDs/classes are presentational extraction noise and provide
/// no useful semantic context to a model.
pub fn strip_span_tags(input: &str) -> std::borrow::Cow<'_, str> {
    SPAN_TAG_RE.replace_all(input, "")
}

pub fn validate_runtime_context(
    values: &std::collections::HashMap<String, String>,
    label: &str,
) -> Result<(), String> {
    if values.len() > MAX_RUNTIME_VALUES {
        return Err(format!(
            "{label} contains {} values; the safety limit is {MAX_RUNTIME_VALUES}",
            values.len()
        ));
    }
    let mut total = 0usize;
    for (key, value) in values {
        if key.len() > 256 {
            return Err(format!("{label} key is longer than 256 bytes"));
        }
        if value.len() > MAX_RUNTIME_VALUE_BYTES {
            return Err(format!(
                "{label} value '{key}' exceeds the 1 MB safety limit"
            ));
        }
        total = total
            .checked_add(key.len())
            .and_then(|bytes| bytes.checked_add(value.len()))
            .ok_or_else(|| format!("{label} size overflow"))?;
        if total > MAX_RUNTIME_CONTEXT_BYTES {
            return Err(format!("{label} exceeds the 8 MB safety limit"));
        }
    }
    Ok(())
}

/// Reject profiles whose declared Cartesian products and retry policy can
/// create an unexpectedly large or costly run before any extraction or model
/// call starts.
pub fn validate_run_budget(
    config: &crate::pipeline_config::PipelineConfig,
    settings: &crate::settings::Settings,
) -> Result<(), String> {
    let mut units = 0u64;
    let mut merge_calls = 0u64;
    for step in config.steps.iter().filter(|step| step.enabled) {
        let items = if step.phase == crate::pipeline_config::Phase::Parallel {
            step.for_each
                .as_ref()
                .map(|fan_out| u64::from(fan_out.max.max(1)))
                .unwrap_or(1)
        } else {
            1
        };
        let agents = if step.phase == crate::pipeline_config::Phase::Parallel {
            step.agents.len().max(1) as u64
        } else {
            1
        };
        units = units
            .checked_add(
                items
                    .checked_mul(agents)
                    .ok_or_else(|| "Run unit count overflow".to_string())?,
            )
            .ok_or_else(|| "Run unit count overflow".to_string())?;
        if config.merge.enabled && agents > 1 {
            merge_calls = merge_calls
                .checked_add(items)
                .ok_or_else(|| "Merge call count overflow".to_string())?;
        }
    }
    if units > MAX_RUN_STEP_UNITS {
        return Err(format!(
            "Profile can dispatch {units} step units; the safety limit is {MAX_RUN_STEP_UNITS}"
        ));
    }
    let attempts_per_step = u64::from(settings.max_retries).saturating_add(1);
    let attempts = units
        .checked_mul(attempts_per_step)
        // Shared slots normally warm only once. A timed-out warm-up can leave
        // its slot unprepared, however, so the safe upper bound is one warm-up
        // for every dispatched logical attempt.
        .and_then(|value| {
            if config.context_cache.enabled {
                units
                    .checked_mul(attempts_per_step)
                    .and_then(|warmups| value.checked_add(warmups))
            } else {
                Some(value)
            }
        })
        .and_then(|value| value.checked_add(merge_calls))
        // Reserve a few calls for extraction, orientation, and reconciliation.
        .and_then(|value| value.checked_add(4))
        .ok_or_else(|| "Provider attempt count overflow".to_string())?;
    if attempts > MAX_RUN_PROVIDER_ATTEMPTS {
        return Err(format!(
            "Profile can make up to {attempts} provider attempts; the safety limit is {MAX_RUN_PROVIDER_ATTEMPTS}"
        ));
    }
    Ok(())
}

/// Open a path without following a final-component symlink or blocking on a
/// FIFO/device. The post-open type check closes the remaining TOCTOU window.
pub fn open_regular_file(path: &Path) -> Result<fs::File, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("Failed to open regular file {}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Failed to inspect {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    Ok(file)
}

/// Replace every occurrence while checking the final allocation before it is
/// attempted. Chaining this helper keeps every intermediate string bounded.
pub fn replace_all_limited(
    input: &str,
    needle: &str,
    replacement: &str,
    limit: usize,
    label: &str,
) -> Result<String, String> {
    if needle.is_empty() || !input.contains(needle) {
        if input.len() > limit {
            return Err(format!(
                "{label} exceeds the {} MB safety limit",
                limit / 1024 / 1024
            ));
        }
        return Ok(input.to_string());
    }
    let count = input.match_indices(needle).count();
    let removed = needle
        .len()
        .checked_mul(count)
        .ok_or_else(|| format!("{label} size overflow"))?;
    let added = replacement
        .len()
        .checked_mul(count)
        .ok_or_else(|| format!("{label} size overflow"))?;
    let final_len = input
        .len()
        .checked_sub(removed)
        .and_then(|length| length.checked_add(added))
        .ok_or_else(|| format!("{label} size overflow"))?;
    if final_len > limit {
        return Err(format!(
            "{label} would expand to {} MB; the safety limit is {} MB",
            final_len / 1024 / 1024,
            limit / 1024 / 1024
        ));
    }
    let mut output = String::with_capacity(final_len);
    let mut rest = input;
    while let Some(offset) = rest.find(needle) {
        output.push_str(&rest[..offset]);
        output.push_str(replacement);
        rest = &rest[offset + needle.len()..];
    }
    output.push_str(rest);
    Ok(output)
}

pub fn push_str_limited(
    output: &mut String,
    value: &str,
    limit: usize,
    label: &str,
) -> Result<(), String> {
    if value.len() > limit.saturating_sub(output.len()) {
        return Err(format!(
            "{label} exceeds the {} MB safety limit",
            limit / 1024 / 1024
        ));
    }
    output.push_str(value);
    Ok(())
}

/// State shared by recursive walkers. It independently bounds files, empty
/// directories, and wall time so none can evade the other limits.
pub struct WalkBudget {
    started: Instant,
    entries: usize,
    directories: usize,
    label: &'static str,
    cancellable: bool,
}

impl WalkBudget {
    pub fn new(label: &'static str) -> Self {
        Self {
            started: Instant::now(),
            entries: 0,
            directories: 1,
            label,
            cancellable: false,
        }
    }

    pub fn new_cancellable(label: &'static str) -> Self {
        Self {
            cancellable: true,
            ..Self::new(label)
        }
    }

    pub fn entry(&mut self) -> Result<(), String> {
        self.entries = self.entries.saturating_add(1);
        self.check()
    }

    pub fn directory(&mut self) -> Result<(), String> {
        self.directories = self.directories.saturating_add(1);
        self.check()
    }

    fn check(&self) -> Result<(), String> {
        if self.entries > MAX_WALK_ENTRIES {
            return Err(format!("{} exceeded the entry safety limit", self.label));
        }
        if self.directories > MAX_WALK_DIRS {
            return Err(format!(
                "{} exceeded the directory safety limit",
                self.label
            ));
        }
        if self.started.elapsed() > MAX_WALK_TIME {
            return Err(format!(
                "{} exceeded the scan-time safety limit",
                self.label
            ));
        }
        if self.cancellable && crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_rejects_multiplicative_expansion_before_allocating() {
        let input = "{x}".repeat(100);
        let error =
            replace_all_limited(&input, "{x}", &"y".repeat(100), 1_000, "prompt").unwrap_err();
        assert!(error.contains("expand"));
    }

    #[test]
    fn replacement_preserves_normal_text() {
        assert_eq!(
            replace_all_limited("a {x} b {x}", "{x}", "ok", 100, "prompt").unwrap(),
            "a ok b ok"
        );
    }

    #[test]
    fn span_cleanup_preserves_text_and_non_span_markup() {
        let input = r#"<SPAN id="equation>1">Estimate</SPAN> and <span class='x'>evidence</span> <div>kept</div>"#;
        assert_eq!(
            strip_span_tags(input),
            "Estimate and evidence <div>kept</div>"
        );
    }

    #[test]
    fn run_budget_rejects_large_agent_fan_out_products() {
        let step = crate::pipeline_config::StepConfig {
            id: "fan-out".to_string(),
            enabled: true,
            phase: crate::pipeline_config::Phase::Parallel,
            agents: vec![
                "claude".to_string(),
                "codex".to_string(),
                "gemini".to_string(),
            ],
            for_each: Some(crate::pipeline_config::ForEach {
                glob: "**/*".to_string(),
                max: 1_000,
            }),
            ..Default::default()
        };
        let config = crate::pipeline_config::PipelineConfig {
            steps: vec![step],
            merge: Default::default(),
            context_cache: Default::default(),
            use_orientation: false,
            orientation_prompt: String::new(),
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        };
        assert!(validate_run_budget(&config, &crate::settings::Settings::default()).is_err());
    }

    #[test]
    fn run_budget_accounts_for_shared_context_warmups() {
        let step = crate::pipeline_config::StepConfig {
            id: "fan-out".to_string(),
            enabled: true,
            phase: crate::pipeline_config::Phase::Parallel,
            agents: vec![
                "claude".to_string(),
                "codex".to_string(),
                "gemini".to_string(),
                "local".to_string(),
            ],
            for_each: Some(crate::pipeline_config::ForEach {
                glob: "**/*".to_string(),
                max: 5,
            }),
            ..Default::default()
        };
        let mut config = crate::pipeline_config::PipelineConfig {
            steps: (0..100).map(|_| step.clone()).collect(),
            merge: Default::default(),
            context_cache: Default::default(),
            use_orientation: false,
            orientation_prompt: String::new(),
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        };
        let settings = crate::settings::Settings {
            max_retries: 1,
            ..Default::default()
        };
        assert!(validate_run_budget(&config, &settings).is_ok());
        config.context_cache.enabled = true;
        let error = validate_run_budget(&config, &settings).unwrap_err();
        assert!(error.contains("provider attempts"), "{error}");
    }
}

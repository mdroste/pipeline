//! Names for models Pipeline does not pick itself.
//!
//! CLI Automatic omits the model flag, so the installed CLI chooses. Pipeline
//! still learns which model that is, either from a model-list lookup or from
//! the model a finished call reports. This keeps the last answer per provider
//! in memory only: CLI account identity has no stable fingerprint, so it is
//! never written to disk, and every use is labeled as expected rather than
//! confirmed.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static CLI_DEFAULTS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn cli_defaults() -> &'static Mutex<HashMap<String, String>> {
    CLI_DEFAULTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Remember the model a provider's CLI uses when Pipeline sends none.
pub fn remember_cli_default(provider: &str, model: &str) {
    let model = model.trim();
    if provider.trim().is_empty() || model.is_empty() {
        return;
    }
    cli_defaults()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(provider.to_string(), model.to_string());
}

/// The model this provider's CLI last chose or listed as its default, if
/// Pipeline has seen one since it started.
pub fn cli_default_model(provider: &str) -> Option<String> {
    cli_defaults()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(provider)
        .cloned()
}

/// Who picks the model when Pipeline sends none for this connection.
pub fn model_chooser(provider: &str, transport: &str) -> &'static str {
    match (provider, transport) {
        ("claude", "api") => "Anthropic",
        ("claude", _) => "Claude Code",
        ("codex", "api") => "OpenAI",
        ("codex", _) => "Codex",
        ("antigravity", _) => "Google",
        ("local", _) => "Local server",
        _ => "Unknown provider",
    }
}

/// A console-ready name for a resolved model. A CLI Automatic choice names the
/// expected model when known and always says who chose it.
pub fn describe_resolution(provider: &str, resolution: &super::ResolvedModel) -> String {
    if resolution.command_model.is_some() || resolution.transport == "api" {
        return resolution.resolved_model.clone();
    }
    let chooser = model_chooser(provider, &resolution.transport);
    match cli_default_model(provider) {
        Some(model) => format!("{model} ({chooser}'s choice)"),
        None => format!("{chooser}'s choice"),
    }
}

/// The placeholder older builds (and unreported CLI Automatic calls) store in
/// place of a model name.
const UNRECORDED_MODEL: &str = "Provider default";

/// A reader-facing name for a model a run recorded. Never returns a bare
/// "default": an unrecorded CLI choice says who chose, and an expected (not
/// confirmed) model says so.
pub fn recorded_model_label(provider: &str, transport: &str, model: &str, basis: &str) -> String {
    let model = model.trim();
    if model.is_empty() || model == UNRECORDED_MODEL || model.starts_with("Automatic") {
        return format!(
            "{}'s choice (not recorded)",
            model_chooser(provider, transport)
        );
    }
    if basis == "expected" {
        format!("{model} (expected)")
    } else {
        model.to_string()
    }
}

/// A reader-facing name for a recorded thinking level. Empty, and the
/// "default" older builds stored, name whose default applied.
pub fn recorded_effort_label(provider: &str, transport: &str, effort: &str) -> String {
    let effort = effort.trim();
    if effort.is_empty() || effort == "default" {
        format!("{} default", model_chooser(provider, transport))
    } else {
        effort.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_labels_never_say_a_bare_default() {
        assert_eq!(
            recorded_model_label("claude", "cli", "Provider default", ""),
            "Claude Code's choice (not recorded)"
        );
        assert_eq!(
            recorded_model_label("codex", "cli", "gpt-5.6-sol", "expected"),
            "gpt-5.6-sol (expected)"
        );
        assert_eq!(
            recorded_model_label("claude", "cli", "claude-opus-5-5", "reported"),
            "claude-opus-5-5"
        );
        assert_eq!(
            recorded_effort_label("claude", "cli", "default"),
            "Claude Code default"
        );
        assert_eq!(recorded_effort_label("codex", "api", ""), "OpenAI default");
        assert_eq!(recorded_effort_label("codex", "api", "high"), "high");
    }

    #[test]
    fn remembers_one_default_per_provider() {
        // A provider name no other test uses keeps this independent of the
        // process-wide map.
        assert_eq!(cli_default_model("choice-test"), None);
        remember_cli_default("choice-test", "  ");
        assert_eq!(cli_default_model("choice-test"), None);
        remember_cli_default("choice-test", "model-a");
        remember_cli_default("choice-test", "model-b");
        assert_eq!(cli_default_model("choice-test").as_deref(), Some("model-b"));
    }

    #[test]
    fn describes_who_chose_a_cli_model() {
        let resolution = crate::model_catalog::ResolvedModel {
            selection: crate::settings::ModelSelection::Automatic,
            command_model: None,
            resolved_model: "Provider default".into(),
            transport: "cli".into(),
            source: "provider_default".into(),
            catalog_updated_at: String::new(),
            supported_efforts: Vec::new(),
        };
        assert_eq!(
            describe_resolution("describe-test", &resolution),
            "Unknown provider's choice"
        );
        remember_cli_default("describe-test", "model-x");
        assert_eq!(
            describe_resolution("describe-test", &resolution),
            "model-x (Unknown provider's choice)"
        );
        let pinned = crate::model_catalog::ResolvedModel {
            command_model: Some("claude-opus-5-5".into()),
            resolved_model: "claude-opus-5-5".into(),
            ..resolution
        };
        assert_eq!(describe_resolution("claude", &pinned), "claude-opus-5-5");
    }
}

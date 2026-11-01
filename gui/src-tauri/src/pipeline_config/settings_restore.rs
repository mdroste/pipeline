//! What a settings backup replaces. Everything a backup records is restored
//! except the settings that belong to this computer. Settings an older backup
//! never recorded stay as they are, so that restoring it cannot lift a Review
//! limit or clear a chosen default.

use crate::settings::Settings;
use serde_json::{Map, Value};

/// Settings a restore leaves alone: API keys never leave a computer, and an
/// access mode or Codex backend depends on the sign-ins and tools found here.
pub(super) const DEVICE_SETTINGS: &[&str] = &[
    "anthropic_api_key",
    "openai_api_key",
    "google_api_key",
    "local_api_key",
    "claude_access_mode",
    "codex_access_mode",
    "antigravity_access_mode",
    "codex_backend",
    "codex_backend_preference_version",
];

/// Settings left out of a saved file while empty. A backup that records
/// `model_defaults_version` is new enough to have written them, so their
/// absence there means empty, not unknown.
const OMITTED_WHEN_EMPTY: &[&str] = &["default_model", "default_effort", "default_model_chosen"];

fn object(settings: &Settings) -> Result<Map<String, Value>, String> {
    match serde_json::to_value(settings) {
        Ok(Value::Object(fields)) => Ok(fields),
        Ok(_) => Err("Settings did not serialize as an object".to_string()),
        Err(error) => Err(format!("Could not read settings: {error}")),
    }
}

fn in_backup(key: &str, backup: &Map<String, Value>) -> bool {
    backup.contains_key(key)
        || (key == "preferred_provider" && backup.contains_key("llm_provider"))
        || (OMITTED_WHEN_EMPTY.contains(&key) && backup.contains_key("model_defaults_version"))
}

/// This computer's settings with a backup's applied. `imported` is the backup
/// after normalization, which can fill a field the file left out; `backup` is
/// the file's own settings object.
pub(super) fn restored_settings(
    current: &Settings,
    imported: &Settings,
    backup: &Value,
) -> Result<Settings, String> {
    let empty = Map::new();
    let backup = backup.as_object().unwrap_or(&empty);
    let local = object(current)?;
    let mut restored = object(imported)?;
    let unset: Settings = serde_json::from_value(Value::Object(Map::new()))
        .map_err(|error| format!("Could not read settings: {error}"))?;
    let unset = object(&unset)?;
    let keys: Vec<String> = local.keys().chain(restored.keys()).cloned().collect();
    for key in keys {
        let unrecorded = !in_backup(&key, backup) && restored.get(&key) == unset.get(&key);
        if !DEVICE_SETTINGS.contains(&key.as_str()) && !unrecorded {
            continue;
        }
        match local.get(&key) {
            Some(value) => restored.insert(key, value.clone()),
            None => restored.remove(&key),
        };
    }
    serde_json::from_value(Value::Object(restored))
        .map_err(|error| format!("Could not combine the backup's settings: {error}"))
}

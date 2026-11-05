//! Recovery paths for settings the app could not load as saved: diagnostics
//! for the Settings page, discarding a stored key that no longer decrypts,
//! and normalizing imported exports the way the loader would.
use super::crypto::{decrypt_string, load_or_create_key, ENC_PREFIX, KEY_SIZE};
use super::persistence::{
    acquire_settings_write_lock, load_raw_settings_required, load_with_warnings_inner,
    save_raw_unlocked, settings_path,
};
use super::types::Settings;

/// What the UI loader had to work around.
#[derive(Debug, Default)]
pub struct LoadDiagnostics {
    pub warnings: Vec<String>,
    /// Providers whose stored key did not decrypt with the current keyfile.
    pub undecryptable_keys: Vec<String>,
}

/// Load settings with any warnings about fallback behavior.
/// Warnings indicate the user's saved settings were partially or fully ignored.
pub fn load_with_warnings() -> (Settings, Vec<String>) {
    let (settings, diagnostics) = load_with_diagnostics();
    (settings, diagnostics.warnings)
}

pub fn load_with_diagnostics() -> (Settings, LoadDiagnostics) {
    let (settings, warnings, undecryptable_keys) = load_with_warnings_inner();
    (
        settings,
        LoadDiagnostics {
            warnings,
            undecryptable_keys,
        },
    )
}

/// Forget a stored API key that no longer decrypts with the current keyfile
/// (settings restored without ~/.pipeline/keyfile). Saving an empty field
/// deliberately preserves such ciphertext, so this is the one explicit way
/// out; a readable key is cleared from Settings instead.
pub fn discard_undecryptable_secret(provider: &str) -> Result<(), String> {
    let path = settings_path()?;
    let (_process_guard, lock_file) = acquire_settings_write_lock(&path)?;
    let result = load_or_create_key()
        .map_err(|e| format!("Could not load encryption key: {e}"))
        .and_then(|key| discard_undecryptable_secret_at(&path, &key, provider));
    let _ = fs2::FileExt::unlock(&lock_file);
    result
}

pub(super) fn discard_undecryptable_secret_at(
    path: &std::path::Path,
    key: &[u8; KEY_SIZE],
    provider: &str,
) -> Result<(), String> {
    let mut current = load_raw_settings_required(path)?;
    let field = match provider {
        "anthropic" => &mut current.anthropic_api_key,
        "openai" => &mut current.openai_api_key,
        "google" => &mut current.google_api_key,
        "local" => &mut current.local_api_key,
        other => return Err(format!("Unknown provider '{other}'")),
    };
    if !field.starts_with(ENC_PREFIX) || decrypt_string(field, key).is_ok() {
        return Err("The stored key is readable; clear it from Settings instead".into());
    }
    field.clear();
    save_raw_unlocked(path, &current)
}

impl Settings {
    /// Apply the compatibility normalization the persisted loader uses, so an
    /// imported export from an earlier build validates the way it would load.
    pub fn normalized_for_import(mut self) -> Self {
        self.migrate_legacy_model_fields();
        self.normalize_access_modes();
        self.drop_unknown_providers();
        self
    }
}

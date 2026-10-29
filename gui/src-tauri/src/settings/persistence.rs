//! Bounded settings reads and serialized, secret-preserving writes.

use super::crypto::{
    atomic_write, decrypt_string, encrypt_string, load_or_create_key, ENC_PREFIX, KEY_SIZE,
};
use super::quarantine::quarantine_corrupt_file;
use super::recovery::load_with_warnings;
use super::types::Settings;
use std::fs;
use std::path::PathBuf;

pub(super) const MAX_SETTINGS_BYTES: usize = 4 * 1024 * 1024;

pub(super) fn settings_path() -> Result<PathBuf, String> {
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

pub(super) fn load_with_warnings_inner() -> (Settings, Vec<String>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut undecryptable = Vec::new();

    let path = match settings_path() {
        Ok(p) => p,
        Err(e) => {
            warnings.push(format!(
                "Could not locate settings file: {e}. Using defaults."
            ));
            return (Settings::default(), warnings, undecryptable);
        }
    };
    if !path.exists() {
        return (Settings::default(), warnings, undecryptable);
    }
    let content = match read_settings_file(&path) {
        Ok(c) => c,
        Err(e) => {
            warnings.push(format!(
                "Could not read settings file: {e}. Using defaults."
            ));
            return (Settings::default(), warnings, undecryptable);
        }
    };
    let mut settings: Settings = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(e) => {
            // Move the unparseable file aside before falling back to defaults.
            // Callers like switch_profile() do load() -> mutate -> save(); without
            // the quarantine that save would overwrite the user's settings
            // (including encrypted API keys) with defaults.
            match quarantine_corrupt_file(&path, &content) {
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
                    undecryptable.push("anthropic".to_string());
                    String::new()
                });
            settings.openai_api_key = decrypt_string(&settings.openai_api_key, &key)
                .unwrap_or_else(|e| {
                    warnings.push(format!(
                        "Could not decrypt OpenAI API key: {e}. The key may need to be re-entered."
                    ));
                    undecryptable.push("openai".to_string());
                    String::new()
                });
            settings.google_api_key = decrypt_string(&settings.google_api_key, &key)
                .unwrap_or_else(|e| {
                    warnings.push(format!(
                        "Could not decrypt Google API key: {e}. The key may need to be re-entered."
                    ));
                    undecryptable.push("google".to_string());
                    String::new()
                });
            settings.local_api_key = decrypt_string(&settings.local_api_key, &key)
                .unwrap_or_else(|e| {
                    warnings.push(format!(
                        "Could not decrypt local-server API key: {e}. The key may need to be re-entered."
                    ));
                    undecryptable.push("local".to_string());
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

    (settings, warnings, undecryptable)
}

pub(super) fn read_settings_file(path: &std::path::Path) -> Result<String, String> {
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
pub(super) fn load_raw_settings_required(path: &std::path::Path) -> Result<Settings, String> {
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

static SETTINGS_WRITE_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(super) fn acquire_settings_write_lock(
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

pub(super) fn save_raw_unlocked(path: &std::path::Path, settings: &Settings) -> Result<(), String> {
    settings.validate()?;
    let mut to_save = settings.clone();
    to_save.migrate_legacy_model_fields();
    to_save.normalize_access_modes();
    to_save.sync_legacy_model_fields();
    let json =
        serde_json::to_string_pretty(&to_save).map_err(|e| format!("Failed to serialize: {e}"))?;
    atomic_write(path, &json)
}

pub(super) fn prepare_secret_for_save(
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

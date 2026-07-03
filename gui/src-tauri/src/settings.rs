use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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

    /// Thinking effort level. Empty = Claude Code default.
    /// Options: "low", "medium", "high", "max".
    #[serde(default)]
    pub claude_effort: String,

    /// Codex model to use. Empty = Codex default.
    /// Examples: "o3", "o4-mini", "gpt-4.1", or a full model ID.
    #[serde(default)]
    pub codex_model: String,

    /// Codex reasoning effort level. Empty = Codex default.
    /// Options: "low", "medium", "high".
    #[serde(default)]
    pub codex_effort: String,

    /// Gemini model to use. Empty = Gemini CLI default.
    /// Examples: "gemini-2.5-pro", "gemini-2.5-flash", or a full model ID.
    #[serde(default)]
    pub gemini_model: String,

    /// PDF extraction method: "llm", "auto" (marker → pdftotext), "marker", or "pdftotext".
    #[serde(default = "default_pdf_extractor")]
    pub pdf_extractor: String,

    /// Disable OCR when using marker-pdf. Faster for native-text PDFs.
    #[serde(default)]
    pub marker_disable_ocr: bool,

    /// Disable image extraction when using marker-pdf.
    #[serde(default = "default_true")]
    pub marker_disable_images: bool,

    /// Show verbose LLM output in the console (command lines, stdout, stderr).
    #[serde(default)]
    pub verbose_logging: bool,

    /// Timeout in seconds for each LLM subprocess call. Default = 1200 (20 min).
    #[serde(default = "default_timeout")]
    pub step_timeout_secs: u64,

    /// Number of times to retry a failed step before giving up. Default = 1.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

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

fn default_true() -> bool {
    true
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
            claude_effort: String::new(),
            codex_model: String::new(),
            codex_effort: String::new(),
            gemini_model: String::new(),
            pdf_extractor: "llm".to_string(),
            marker_disable_ocr: false,
            marker_disable_images: true,
            verbose_logging: false,
            step_timeout_secs: 1200,
            max_retries: 1,
            anthropic_api_key: String::new(),
            openai_api_key: String::new(),
            google_api_key: String::new(),
            local_base_url: default_local_base_url(),
            local_model: String::new(),
            local_api_key: String::new(),
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
            eprintln!("WARNING: could not tighten permissions on {}: {e}", dir.display());
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
        Err(_) => return (Settings::default(), warnings),
    };
    if !path.exists() {
        return (Settings::default(), warnings);
    }
    let content = match fs::read_to_string(&path) {
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

/// Load settings, discarding any warnings. Used by non-UI callers
/// (pipeline execution, etc.) where fallback to defaults is fine.
pub fn load() -> Settings {
    load_with_warnings().0
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

pub fn save(settings: &Settings) -> Result<(), String> {
    let path = settings_path()?;
    let key = load_or_create_key()?;

    // Encrypt API keys before writing to disk
    let mut to_save = settings.clone();
    to_save.anthropic_api_key = encrypt_string(&settings.anthropic_api_key, &key)?;
    to_save.openai_api_key = encrypt_string(&settings.openai_api_key, &key)?;
    to_save.google_api_key = encrypt_string(&settings.google_api_key, &key)?;
    to_save.local_api_key = encrypt_string(&settings.local_api_key, &key)?;

    let json = serde_json::to_string_pretty(&to_save)
        .map_err(|e| format!("Failed to serialize: {e}"))?;
    atomic_write(&path, &json)
}

// ── Encryption helpers ─────────────────────────────────────────────

use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Key, Nonce};
use base64::{Engine, engine::general_purpose::STANDARD};

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

fn load_or_create_key_inner() -> Result<[u8; KEY_SIZE], String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let path = home.join(".pipeline").join("keyfile");

    if path.exists() {
        let bytes = fs::read(&path)
            .map_err(|e| format!("Failed to read keyfile: {e}"))?;
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
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create .pipeline dir: {e}"))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        use std::io::Write as _;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("Failed to write keyfile: {e}"))?;
        file.write_all(&key)
            .map_err(|e| format!("Failed to write keyfile: {e}"))?;
    }
    #[cfg(windows)]
    {
        fs::write(&path, &key)
            .map_err(|e| format!("Failed to write keyfile: {e}"))?;
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
    #[cfg(not(any(unix, windows)))]
    {
        fs::write(&path, &key)
            .map_err(|e| format!("Failed to write keyfile: {e}"))?;
    }

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
    getrandom::getrandom(&mut nonce_bytes)
        .map_err(|e| format!("RNG failed: {e}"))?;
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

    String::from_utf8(plaintext)
        .map_err(|e| format!("Decrypted text is not valid UTF-8: {e}"))
}

// ── File I/O ───────────────────────────────────────────────────────

/// Write to a uniquely-named temp sibling then rename, so a crash mid-write can't
/// corrupt the file and concurrent writers can't step on each other's temp file.
/// On Unix, tempfile creates the file with 0o600 by default (owner-only), which
/// is what we want since settings may contain encrypted API keys.
fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), String> {
    use std::io::Write as _;
    let dir = path.parent().ok_or_else(|| format!("No parent dir for {}", path.display()))?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| format!("Failed to create temp file in {}: {e}", dir.display()))?;
    tmp.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write {}: {e}", tmp.path().display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Surface failures: on filesystems that can't honor 0o600, the settings
        // file would otherwise be world-readable silently.
        fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("Failed to tighten permissions on {}: {e}", tmp.path().display()))?;
    }
    tmp.persist(path)
        .map_err(|e| format!("Failed to save {}: {}", path.display(), e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(decrypt_string("sk-plain-key", &key).unwrap(), "sk-plain-key");
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
}

// ── Encryption helpers ─────────────────────────────────────────────

use super::*;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};

pub(super) const NONCE_SIZE: usize = 12;
pub(super) const KEY_SIZE: usize = 32;
pub(super) const ENC_PREFIX: &str = "enc:";

/// Cached encryption key to prevent TOCTOU race conditions.
/// The Mutex ensures only one thread loads/creates the key at a time.
pub(super) static CACHED_KEY: std::sync::Mutex<Option<[u8; KEY_SIZE]>> =
    std::sync::Mutex::new(None);

/// Load the encryption key from ~/.pipeline/keyfile, creating it on first use.
/// Uses an in-memory cache to prevent race conditions when multiple async
/// tasks call load() or save() concurrently.
pub(super) fn load_or_create_key() -> Result<[u8; KEY_SIZE], String> {
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

pub(super) fn read_keyfile(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(path)?;
    let mut bytes = Vec::with_capacity(KEY_SIZE + 1);
    file.take(KEY_SIZE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read keyfile: {error}"))?;
    Ok(bytes)
}

pub(super) fn load_or_create_key_inner() -> Result<[u8; KEY_SIZE], String> {
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
    getrandom::fill(&mut key).map_err(|e| format!("Failed to generate encryption key: {e}"))?;

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
pub(super) fn encrypt_string(plaintext: &str, key: &[u8; KEY_SIZE]) -> Result<String, String> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }
    let key: Key<Aes256Gcm> = (*key).into();
    let cipher = Aes256Gcm::new(&key);
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    getrandom::fill(&mut nonce_bytes).map_err(|e| format!("RNG failed: {e}"))?;
    let nonce = nonce_bytes.into();

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {e}"))?;

    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(format!("{}{}", ENC_PREFIX, STANDARD.encode(&combined)))
}

/// Decrypt an "enc:..." string. Plaintext strings (no prefix) pass through
/// unchanged, providing backward compatibility with existing settings files.
pub(super) fn decrypt_string(stored: &str, key: &[u8; KEY_SIZE]) -> Result<String, String> {
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
    let key: Key<Aes256Gcm> = (*key).into();
    let cipher = Aes256Gcm::new(&key);
    let nonce = Nonce::try_from(nonce_bytes)
        .map_err(|_| "Encrypted nonce has an invalid length".to_string())?;

    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| "Decryption failed — keyfile may have been deleted or replaced".to_string())?;

    String::from_utf8(plaintext).map_err(|e| format!("Decrypted text is not valid UTF-8: {e}"))
}

// ── File I/O ───────────────────────────────────────────────────────

/// Write to a uniquely-named temp sibling then rename, so a crash mid-write can't
/// corrupt the file and concurrent writers can't step on each other's temp file.
/// On Unix, tempfile creates the file with 0o600 by default (owner-only), which
/// is what we want since settings may contain encrypted API keys.
pub(super) fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), String> {
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

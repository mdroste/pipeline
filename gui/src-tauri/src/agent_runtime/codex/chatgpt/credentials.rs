//! Private credential I/O. Secrets never implement Debug or cross the UI boundary.
use super::super::session::private_write;
use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_AUTH_BYTES: u64 = 128 * 1024;

pub(super) struct Credentials {
    bytes: Vec<u8>,
    access_token: String,
    pub identity: String,
    pub account_id: String,
    pub email: Option<String>,
    pub plan: Option<String>,
}

impl Credentials {
    pub fn external_tokens(&self) -> Value {
        json!({"accessToken":self.access_token,"chatgptAccountId":self.account_id,"chatgptPlanType":self.plan})
    }
}

pub(super) fn read(path: &Path) -> Result<Option<Credentials>, String> {
    if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("Saved ChatGPT sign-in must not be a symlink".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Cannot read saved ChatGPT sign-in".into()),
    };
    let metadata = file
        .metadata()
        .map_err(|_| "Cannot inspect saved ChatGPT sign-in")?;
    if !metadata.is_file() || metadata.len() > MAX_AUTH_BYTES {
        return Err("Saved ChatGPT sign-in is not a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_AUTH_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read saved ChatGPT sign-in")?;
    if bytes.len() as u64 > MAX_AUTH_BYTES {
        return Err("Saved ChatGPT sign-in is too large".into());
    }
    parse(bytes).map(Some)
}

fn parse(bytes: Vec<u8>) -> Result<Credentials, String> {
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Saved ChatGPT sign-in is invalid; sign in again")?;
    let tokens = &value["tokens"];
    let field = |name: &str| {
        tokens[name]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| "Saved sign-in is not a renewable ChatGPT account".to_string())
    };
    let access_token = field("access_token")?;
    let account_id = field("account_id")?;
    field("refresh_token")?;
    let claims = access_token
        .split('.')
        .nth(1)
        .and_then(|part| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(part)
                .ok()
        })
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .ok_or("Saved ChatGPT token has invalid claims")?;
    let subject = claims["sub"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Saved ChatGPT token has no account identity")?;
    let identity = format!("{account_id}:{subject}");
    let email = claims["email"]
        .as_str()
        .or_else(|| claims["https://api.openai.com/profile"]["email"].as_str())
        .map(str::to_string);
    let plan = claims["https://api.openai.com/auth"]["chatgpt_plan_type"]
        .as_str()
        .map(str::to_string);
    Ok(Credentials {
        bytes,
        access_token,
        account_id,
        identity,
        email,
        plan,
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExistingAccount {
    pub source: String,
    pub label: String,
    pub email: Option<String>,
}

pub(super) fn legacy_paths(user_home: &Path) -> [(String, String, PathBuf); 2] {
    [
        (
            "workspace".into(),
            "Conversations".into(),
            user_home.join(".pipeline/workbench/codex/auth.json"),
        ),
        (
            "reviews".into(),
            "Reviews".into(),
            user_home.join(".pipeline/providers/workflows/codex/auth.json"),
        ),
    ]
}

/// Called only before the managed auth process starts and while its OS lock is
/// held. A durable marker prevents an intentional logout from re-importing auth.
pub(super) fn migrate(
    root: &Path,
    user_home: &Path,
    selected: Option<&str>,
) -> Result<Vec<ExistingAccount>, String> {
    let destination = root.join("codex/auth.json");
    let marker = root.join("account-migration-v1");
    if marker.exists() || destination.exists() {
        if !marker.exists() {
            private_write(&marker, b"complete")?;
        }
        return Ok(Vec::new());
    }
    let mut candidates = Vec::new();
    for (source, label, path) in legacy_paths(user_home) {
        if let Ok(Some(auth)) = read(&path) {
            candidates.push((
                ExistingAccount {
                    source,
                    label,
                    email: auth.email.clone(),
                },
                auth,
            ));
        }
    }
    let choice = if let Some(source) = selected {
        Some(
            candidates
                .iter()
                .position(|(account, _)| account.source == source)
                .ok_or("That saved ChatGPT account is no longer available")?,
        )
    } else if candidates.len() == 1
        || (candidates.len() == 2 && candidates[0].1.identity == candidates[1].1.identity)
    {
        Some(0)
    } else {
        None
    };
    if let Some(index) = choice {
        private_write(&destination, &candidates[index].1.bytes)?;
        private_write(&marker, b"complete")?;
        return Ok(Vec::new());
    }
    if candidates.is_empty() {
        private_write(&marker, b"complete")?;
    }
    Ok(candidates.into_iter().map(|(account, _)| account).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(subject: &str) -> Vec<u8> {
        let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            json!({"sub":subject,"email":format!("{subject}@example.invalid")}).to_string(),
        );
        serde_json::to_vec(&json!({"tokens":{"access_token":format!("e30.{claims}.fixture"),"account_id":"org","refresh_token":"fixture"}})).unwrap()
    }
    #[test]
    fn migration_requires_choice_for_distinct_users_and_does_not_undo_logout() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("shared");
        std::fs::create_dir_all(root.join("codex")).unwrap();
        for ((_, _, path), subject) in legacy_paths(temp.path()).into_iter().zip(["alice", "bob"]) {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, fixture(subject)).unwrap();
        }
        assert_eq!(migrate(&root, temp.path(), None).unwrap().len(), 2);
        assert!(!root.join("codex/auth.json").exists());
        migrate(&root, temp.path(), Some("reviews")).unwrap();
        assert_eq!(
            read(&root.join("codex/auth.json"))
                .unwrap()
                .unwrap()
                .identity,
            "org:bob"
        );
        std::fs::remove_file(root.join("codex/auth.json")).unwrap();
        assert!(migrate(&root, temp.path(), None).unwrap().is_empty());
        assert!(!root.join("codex/auth.json").exists());
    }
    #[test]
    fn migration_reuses_one_account_and_keeps_source_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("shared");
        std::fs::create_dir_all(root.join("codex")).unwrap();
        let source = &legacy_paths(temp.path())[0].2;
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(source, fixture("alice")).unwrap();
        assert!(migrate(&root, temp.path(), None).unwrap().is_empty());
        assert_eq!(
            std::fs::read(source).unwrap(),
            std::fs::read(root.join("codex/auth.json")).unwrap()
        );
    }
    #[test]
    fn malformed_credentials_fail_without_including_secrets() {
        let error = parse(b"secret-bad-json".to_vec()).err().unwrap();
        assert!(!error.contains("secret-bad-json"));
    }
    #[cfg(unix)]
    #[test]
    fn credential_reader_rejects_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::write(&source, fixture("alice")).unwrap();
        let link = temp.path().join("auth.json");
        std::os::unix::fs::symlink(source, &link).unwrap();
        assert!(read(&link).is_err());
    }
}

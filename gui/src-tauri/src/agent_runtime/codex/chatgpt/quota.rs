//! Account-owned quota observations. The identity comes from the already-owned
//! login; no credentials are stored in or returned with this cache.
use super::*;
use crate::agent_runtime::codex::RateLimits;
use sha2::{Digest, Sha256};

pub(super) async fn read(owner: &ChatgptAccount) -> Result<RateLimits, String> {
    let _lease = owner.activity.try_read().map_err(|_| BUSY)?;
    let client = owner.client().await?;
    if client
        .account_state(false)
        .await
        .map_err(|_| "Cannot verify the current ChatGPT account")?
        .status
        != AccountStatus::Chatgpt
    {
        return Err("Sign in to ChatGPT to see usage".into());
    }
    let identity = credentials::read(&owner.root.join("codex/auth.json"))?
        .ok_or("ChatGPT account is unavailable")?
        .identity;
    let path = cache_path(&owner.root, &identity);
    match client.rate_limits().await {
        Ok(mut limits) => {
            limits.observed_at = Some(chrono::Utc::now().timestamp_millis());
            limits.cached = false;
            if let Ok(bytes) = serde_json::to_vec(&limits) {
                let _ = private_write(&path, &bytes);
            }
            Ok(limits)
        }
        Err(error) => {
            if let Some(mut limits) = cached(&path) {
                limits.cached = true;
                Ok(limits)
            } else {
                Err(error.to_string())
            }
        }
    }
}

fn cache_path(root: &std::path::Path, identity: &str) -> std::path::PathBuf {
    root.join(format!("usage-{:x}.json", Sha256::digest(identity)))
}
fn cached(path: &std::path::Path) -> Option<RateLimits> {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|m| m.is_file() && m.len() <= 64 * 1024)
        .and_then(|_| std::fs::read(path).ok())
        .and_then(|v| serde_json::from_slice::<RateLimits>(&v).ok())
        .filter(|v| v.observed_at.is_some_and(|at| at > 0))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_usage_is_identity_scoped_bounded_and_requires_a_timestamp() {
        let temp = tempfile::tempdir().unwrap();
        let first = cache_path(temp.path(), "account-one");
        let second = cache_path(temp.path(), "account-two");
        let report =
            serde_json::json!({"source":"perBucket","buckets":[],"observedAt":123,"cached":false});
        private_write(&first, &serde_json::to_vec(&report).unwrap()).unwrap();
        assert_eq!(cached(&first).unwrap().observed_at, Some(123));
        assert!(cached(&second).is_none());
        let mut missing = report;
        missing.as_object_mut().unwrap().remove("observedAt");
        private_write(&first, &serde_json::to_vec(&missing).unwrap()).unwrap();
        assert!(cached(&first).is_none());
        private_write(&first, &vec![b' '; 65537]).unwrap();
        assert!(cached(&first).is_none());
    }
}

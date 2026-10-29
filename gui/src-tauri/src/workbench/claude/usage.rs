//! Account-scoped cached subscription observations. Every window keeps its own
//! timestamp and reset; expired reports remain evidence, never a fresh balance.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

static REPORTS: OnceLock<Mutex<BTreeMap<String, Vec<ClaudeRateLimit>>>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeRateLimit {
    /// `allowed`, `allowed_warning`, or `rejected`.
    pub status: String,
    /// Unix seconds when the reported window resets.
    pub resets_at: Option<i64>,
    /// `five_hour`, `seven_day`, `seven_day_opus`, `seven_day_sonnet`, `overage`, or another
    /// window name that Claude Code reports.
    pub rate_limit_type: Option<String>,
    /// Share of the reported window used, 0-100.
    pub used_percent: Option<f64>,
    pub is_using_overage: bool,
    /// Unix milliseconds when Pipeline received the report.
    pub observed_at: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<ClaudeRateLimit>,
}

/// Validates one stream frame. Unknown statuses and malformed values are
/// ignored rather than shown as usage.
pub(super) fn parse(frame: &Value, observed_at: i64) -> Option<ClaudeRateLimit> {
    if frame["type"] != "rate_limit_event" {
        return None;
    }
    let info = frame.get("rate_limit_info")?;
    let status = info["status"].as_str()?;
    if !matches!(status, "allowed" | "allowed_warning" | "rejected") {
        return None;
    }
    let rate_limit_type = info["rateLimitType"]
        .as_str()
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 64
                && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .map(str::to_owned);
    let resets_at = info["resetsAt"]
        .as_f64()
        .or_else(|| {
            rate_limit_type
                .as_deref()
                .and_then(|name| info["unifiedWindows"][name]["resetsAt"].as_f64())
        })
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.round() as i64);
    // Claude Code 2.1.312 reports each window's fraction under `unifiedWindows`,
    // keyed by window name, rather than at the top level. Tolerate a percentage.
    let window = rate_limit_type
        .as_deref()
        .map(|name| &info["unifiedWindows"][name]["utilization"]);
    let used_percent = info["utilization"]
        .as_f64()
        .or_else(|| window.and_then(Value::as_f64))
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| if value <= 1.0 { value * 100.0 } else { value })
        .map(|value| value.min(100.0));
    Some(ClaudeRateLimit {
        status: status.into(),
        resets_at,
        rate_limit_type,
        used_percent,
        is_using_overage: info["isUsingOverage"] == true,
        observed_at,
        windows: vec![],
    })
}

/// Stable, non-secret identity reported by the authenticated CLI. Without it
/// we cannot safely reuse observations after sign-in changes.
pub(super) fn account_key(status: &Value) -> Option<String> {
    let user = status["accountUuid"]
        .as_str()
        .or_else(|| status["email"].as_str())?;
    if user.is_empty() {
        return None;
    }
    Some(hex::encode(Sha256::digest(format!(
        "{}\0{}",
        user,
        status["orgId"]
            .as_str()
            .or_else(|| status["organizationId"].as_str())
            .unwrap_or_default()
    ))))
}

fn path(key: &str) -> Option<std::path::PathBuf> {
    Some(
        crate::storage::local_root()
            .ok()?
            .join("providers/claude/usage")
            .join(format!("{key}.json")),
    )
}
fn read(key: &str) -> Vec<ClaudeRateLimit> {
    let Some(path) = path(key) else {
        return vec![];
    };
    let Ok(meta) = std::fs::symlink_metadata(&path) else {
        return vec![];
    };
    if !meta.is_file() || meta.len() > 64 * 1024 {
        return vec![];
    }
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<ClaudeRateLimit>>(&b).ok())
        .filter(|v| v.len() <= 16 && v.iter().all(|r| r.windows.is_empty()))
        .unwrap_or_default()
}
fn merge(reports: &mut Vec<ClaudeRateLimit>, frame: &Value, now: i64) {
    let Some(primary) = parse(frame, now) else {
        return;
    };
    let mut updates = vec![primary.clone()];
    if let Some(windows) = frame["rate_limit_info"]["unifiedWindows"].as_object() {
        for (name, value) in windows.iter().take(16) {
            if primary.rate_limit_type.as_deref() == Some(name) {
                continue;
            }
            let mut info = value.clone();
            if !info.is_object() {
                continue;
            }
            if info.get("status").is_none() {
                info["status"] = Value::String(primary.status.clone());
            }
            info["rateLimitType"] = Value::String(name.clone());
            if let Some(window) = parse(
                &serde_json::json!({"type":"rate_limit_event","rate_limit_info":info}),
                now,
            ) {
                updates.push(window);
            }
        }
    }
    for report in updates {
        reports.retain(|old| old.rate_limit_type != report.rate_limit_type);
        reports.push(report);
    }
    reports.sort_by(|a, b| a.rate_limit_type.cmp(&b.rate_limit_type));
    reports.truncate(16);
}

pub(super) fn record(frame: &Value, account: Option<&str>) {
    if frame["type"] != "rate_limit_event" {
        return;
    }
    let Some(key) = account else {
        return;
    };
    let now = chrono::Utc::now().timestamp_millis();
    let mut cache = REPORTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let reports = cache.entry(key.into()).or_insert_with(|| read(key));
    merge(reports, frame, now);
    if let Some(path) = path(key) {
        if let (Some(parent), Ok(bytes)) = (path.parent(), serde_json::to_vec(reports)) {
            if crate::agent_runtime::codex::session::private_directory(parent).is_ok() {
                let _ = crate::agent_runtime::codex::session::private_write(&path, &bytes);
            }
        }
    }
}

#[tauri::command]
pub async fn workbench_claude_rate_limits() -> Option<ClaudeRateLimit> {
    let status = if let Some(environment) = super::wsl::detect_async().await {
        tokio::task::spawn_blocking(move || environment.status())
            .await
            .ok()?
            .ok()?
    } else {
        crate::agent_runtime::claude::verified_subscription_with_status()
            .await
            .ok()?
            .1
    };
    let key = account_key(&status)?;
    let reports = REPORTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(key.clone())
        .or_insert_with(|| read(&key))
        .clone();
    let mut latest = reports.iter().max_by_key(|r| r.observed_at)?.clone();
    latest.windows = reports;
    Some(latest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caches_distinct_windows_without_inventing_missing_resets() {
        let mut reports = vec![];
        merge(
            &mut reports,
            &serde_json::json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed","rateLimitType":"five_hour","resetsAt":2000000000,"unifiedWindows":{"five_hour":{"status":"allowed","utilization":0.3},"seven_day":{"status":"allowed_warning","utilization":0.9}}}}),
            100,
        );
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].used_percent, Some(30.0));
        assert_eq!(reports[1].resets_at, None);
        assert_eq!(reports[1].used_percent, Some(90.0));
        assert!(account_key(&serde_json::json!({})).is_none());
        assert_ne!(
            account_key(&serde_json::json!({"email":"a","organizationId":"one"})),
            account_key(&serde_json::json!({"email":"a","organizationId":"two"}))
        );
    }
}

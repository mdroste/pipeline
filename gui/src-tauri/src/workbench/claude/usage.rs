//! Last subscription usage that Claude Code reported in a `rate_limit_event`.
//! Claude Code sends the event when its usage state changes, so this is the
//! latest report in this app session, not a live meter.
use serde::Serialize;
use serde_json::Value;
use std::sync::Mutex;

static LAST: Mutex<Option<ClaudeRateLimit>> = Mutex::new(None);

#[derive(Clone, Debug, PartialEq, Serialize)]
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
    let resets_at = info["resetsAt"]
        .as_f64()
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.round() as i64);
    let rate_limit_type = info["rateLimitType"]
        .as_str()
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 64
                && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .map(str::to_owned);
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
    })
}

pub(super) fn record(frame: &Value) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0);
    if let Some(report) = parse(frame, now) {
        *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(report);
    }
}

#[tauri::command]
pub fn workbench_claude_rate_limits() -> Option<ClaudeRateLimit> {
    LAST.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

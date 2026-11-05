//! Model thinking options shared by catalogs and both API callers.
use serde_json::{json, Value};

pub fn openai_effort(effort: &str) -> Option<&'static str> {
    match effort {
        "none" => Some("none"),
        "minimal" => Some("minimal"),
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" | "xhigh" => Some("high"),
        _ => None,
    }
}

pub fn google_config(model: &str, effort: &str) -> Result<Option<Value>, String> {
    if effort.trim().is_empty() {
        return Ok(None);
    }
    if !google_efforts(model).iter().any(|value| value == effort) {
        return Err("Choose a thinking level listed for this model.".into());
    }
    Ok(Some(google_thinking(model, effort)))
}

pub fn anthropic_config(model: &str, effort: &str) -> Option<Value> {
    if model.starts_with("claude-haiku") {
        return None;
    }
    match effort {
        "low" | "medium" | "high" | "max" => Some(json!({"effort":effort})),
        _ => None,
    }
}

pub(crate) fn google_efforts(model: &str) -> Vec<String> {
    let model = model.trim_start_matches("models/");
    let levels: &[&str] = if model.starts_with("gemini-2.5-pro") {
        &["low", "medium", "high"]
    } else if model.starts_with("gemini-2.5-flash") {
        &["none", "low", "medium", "high"]
    } else if model.starts_with("gemini-3-pro") {
        &["low", "high"]
    } else if model.starts_with("gemini-3.1-flash-lite-image") {
        &["minimal", "high"]
    } else if model.starts_with("gemini-3.8-flash")
        || model.starts_with("gemini-4-argon")
        || model.starts_with("gemini-3.1-pro")
    {
        &["low", "medium", "high"]
    } else if [
        "gemini-3-flash",
        "gemini-3.1-flash-lite",
        "gemini-3.5-flash-lite",
        "gemini-3.6-flash",
    ]
    .iter()
    .any(|prefix| model.starts_with(prefix))
    {
        &["minimal", "low", "medium", "high"]
    } else {
        &[]
    };
    levels.iter().map(|s| (*s).into()).collect()
}

pub(crate) fn google_thinking(model: &str, effort: &str) -> serde_json::Value {
    if model
        .trim_start_matches("models/")
        .starts_with("gemini-2.5-")
    {
        serde_json::json!({"thinkingBudget":match effort { "none"=>0, "low"=>1024, "medium"=>4096, _=>8192 }})
    } else {
        serde_json::json!({"thinkingLevel":effort})
    }
}

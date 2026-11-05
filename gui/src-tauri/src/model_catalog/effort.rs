//! Model-specific API effort metadata. Anthropic and custom endpoints can
//! advertise their own capabilities; Gemini's generation families use distinct
//! wire controls. Unknown models keep provider defaults rather than guessed
//! capabilities.
use super::*;

pub(super) fn anthropic(item: &serde_json::Value) -> ModelCatalogEntry {
    let id = item["id"].as_str().unwrap_or_default();
    let mut model = entry(id);
    model.display_name = item["display_name"].as_str().unwrap_or(id).into();
    for effort in ["low", "medium", "high", "xhigh", "max"] {
        if item["capabilities"]["effort"][effort]["supported"] == true {
            model.supported_efforts.push(effort.into());
        }
    }
    if item["capabilities"]["thinking"]["types"]["adaptive"]["supported"] == true {
        model.capabilities.push("adaptive_thinking".into());
    }
    model
}

#[cfg(test)]
use crate::connections::thinking::{google_efforts, google_thinking};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capability_metadata_and_google_families_do_not_invent_support() {
        let model = anthropic(
            &serde_json::json!({"id":"claude-future","capabilities":{"effort":{"low":{"supported":true},"max":{"supported":false}},"thinking":{"types":{"adaptive":{"supported":true}}}}}),
        );
        assert_eq!(model.supported_efforts, ["low"]);
        assert!(model.capabilities.contains(&"adaptive_thinking".into()));
        assert!(anthropic(&serde_json::json!({"id":"older"}))
            .supported_efforts
            .is_empty());
        assert!(google_efforts("unknown").is_empty());
        assert!(!google_efforts("gemini-2.5-pro").contains(&"none".into()));
        assert!(!google_efforts("gemini-3.8-flash").contains(&"minimal".into()));
        assert_eq!(
            google_thinking("gemini-2.5-flash", "low")["thinkingBudget"],
            1024
        );
        assert_eq!(
            google_thinking("gemini-3.8-flash", "low")["thinkingLevel"],
            "low"
        );
    }
}

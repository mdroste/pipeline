//! Convert the application account catalog without starting either product runtime.
pub(super) async fn chatgpt_catalog() -> Result<crate::model_catalog::ModelCatalog, String> {
    let account = crate::agent_runtime::codex::chatgpt::connect().await?;
    let status = account.status(false).await?;
    if status.account.status != crate::agent_runtime::codex::AccountStatus::Chatgpt {
        return Err("Sign in to ChatGPT in Settings to use Conversations and Reviews".into());
    }
    let native = crate::agent_runtime::codex::chatgpt::chatgpt_model_catalog().await?;
    Ok(from_native(native, status.version))
}

fn from_native(
    native: crate::connections::models::ModelCatalog,
    version: String,
) -> crate::model_catalog::ModelCatalog {
    let default_model = native
        .models
        .iter()
        .find(|m| m.is_default)
        .map(|m| m.model.clone());
    crate::model_catalog::ModelCatalog {
        provider: "codex".into(),
        transport: "cli".into(),
        source: "codex_app_server".into(),
        source_version: version,
        fetched_at: chrono::Utc::now().to_rfc3339(),
        default_model,
        models: native
            .models
            .into_iter()
            .map(|m| crate::model_catalog::ModelCatalogEntry {
                id: m.model,
                display_name: m.display_name,
                description: m.description,
                is_default: m.is_default,
                default_effort: (!m.default_reasoning_effort.trim().is_empty())
                    .then(|| m.default_reasoning_effort.trim().to_string()),
                supported_efforts: m
                    .supported_reasoning_efforts
                    .into_iter()
                    .map(|e| e.reasoning_effort)
                    .collect(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_live_model_and_thinking_defaults_without_guessing_missing_values() {
        let models = serde_json::from_value(serde_json::json!([
            {"id":"menu-id", "model":"model-a", "displayName":"Model A",
             "description":"", "isDefault":true, "defaultReasoningEffort":"medium",
             "supportedReasoningEfforts":[{"reasoningEffort":"high","description":"High"},
                                          {"reasoningEffort":"medium","description":"Medium"}]},
            {"id":"model-b", "model":"model-b", "displayName":"Model B",
             "description":"", "isDefault":false, "defaultReasoningEffort":"",
             "supportedReasoningEfforts":[{"reasoningEffort":"high","description":"High"}]}
        ]))
        .unwrap();
        let catalog = from_native(
            crate::connections::models::ModelCatalog { models },
            "fixture".into(),
        );
        assert_eq!(catalog.default_model.as_deref(), Some("model-a"));
        assert_eq!(catalog.models[0].default_effort.as_deref(), Some("medium"));
        assert_eq!(catalog.models[1].default_effort, None);
        let wire = serde_json::to_value(catalog).unwrap();
        assert_eq!(wire["models"][0]["default_effort"], "medium");
        assert!(wire["models"][1]["default_effort"].is_null());
    }

    #[test]
    fn older_catalog_entries_have_no_known_thinking_default() {
        let old = serde_json::json!({"id":"old", "display_name":"Old"});
        let entry: crate::model_catalog::ModelCatalogEntry = serde_json::from_value(old).unwrap();
        assert_eq!(entry.default_effort, None);
    }
}

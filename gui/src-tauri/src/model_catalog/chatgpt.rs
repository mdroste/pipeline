//! Convert the application account catalog without starting either product runtime.
pub(super) async fn chatgpt_catalog() -> Result<crate::model_catalog::ModelCatalog, String> {
    let account = crate::agent_runtime::codex::chatgpt::connect().await?;
    let status = account.status(false).await?;
    if status.account.status != crate::agent_runtime::codex::AccountStatus::Chatgpt {
        return Err("Sign in to ChatGPT in Settings to use Conversations and Reviews".into());
    }
    let native = crate::agent_runtime::codex::chatgpt::chatgpt_model_catalog().await?;
    let default_model = native
        .models
        .iter()
        .find(|m| m.is_default)
        .map(|m| m.model.clone());
    Ok(crate::model_catalog::ModelCatalog {
        provider: "codex".into(),
        transport: "cli".into(),
        source: "codex_app_server".into(),
        source_version: status.version,
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
                supported_efforts: m
                    .supported_reasoning_efforts
                    .into_iter()
                    .map(|e| e.reasoning_effort)
                    .collect(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    })
}

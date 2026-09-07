//! Default Workflow Codex subscription runtime. No Workspace state is used here.

pub mod connection;
mod invocation;
mod journal;
pub mod probe;
mod tools;

pub use connection::{
    shutdown, workflow_codex_login_cancel, workflow_codex_login_start, workflow_codex_logout,
    workflow_codex_rate_limits, workflow_codex_status,
};
pub use invocation::call_codex;

pub async fn catalog() -> Result<crate::model_catalog::ModelCatalog, String> {
    let c = connection::connect().await?;
    let _lease = c.activity.read().await;
    if c.native
        .client
        .account_state(false)
        .await
        .map_err(|e| e.to_string())?
        .status
        != crate::agent_runtime::codex::AccountStatus::Chatgpt
    {
        return Err("Workflow ChatGPT is not signed in. Open Settings → API Keys → ChatGPT → App Server connection.".into());
    }
    let native = c
        .native
        .client
        .model_catalog()
        .await
        .map_err(|e| e.to_string())?;
    let default_model = native
        .models
        .iter()
        .find(|m| m.is_default)
        .map(|m| m.model.clone());
    Ok(crate::model_catalog::ModelCatalog {
        provider: "codex".into(),
        transport: "cli".into(),
        source: "codex_app_server".into(),
        source_version: c.native.version.clone(),
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

#[cfg(test)]
mod tests;

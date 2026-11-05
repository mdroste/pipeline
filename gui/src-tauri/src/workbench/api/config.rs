//! Conversation selection and device snapshot loading. Shared resolution takes
//! this snapshot directly and never reads Review run settings.
use super::*;
pub(crate) fn selected(overrides: &Value) -> WorkbenchResult<Option<Provider>> {
    let runtime = match overrides.get("runtime") {
        None => return Ok(None),
        Some(Value::String(runtime)) => runtime,
        _ => return Err(WorkbenchError::invalid("Choose a connection.")),
    };
    crate::connections::ConnectionId::from_runtime(runtime)
        .map(|connection| connection.api())
        .map_err(WorkbenchError::invalid)
}

pub(crate) async fn load(
    provider: Provider,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<Connection, String> {
    let settings = crate::settings::load_persisted_required()?;
    Connection::from_settings(provider, model, effort, &settings).await
}

#[tauri::command]
pub async fn workbench_api_model_catalog(
    runtime: String,
) -> Result<crate::connections::models::ModelCatalog, String> {
    let provider = selected(&json!({"runtime":runtime}))
        .map_err(|e| e.message)?
        .ok_or("Choose an API connection")?;
    let settings = crate::settings::load_persisted_required()?;
    let catalog =
        crate::model_catalog::discover_connection(provider.connection(), &settings, false).await?;
    Ok(crate::connections::models::ModelCatalog {
        models: catalog
            .models
            .into_iter()
            .map(|m| crate::connections::models::WorkspaceModel {
                id: m.id.clone(),
                model: m.id,
                display_name: m.display_name,
                description: m.description,
                is_default: m.is_default,
                default_reasoning_effort: String::new(),
                supported_reasoning_efforts: {
                    m.supported_efforts
                        .into_iter()
                        .filter(|e| {
                            matches!(
                                e.as_str(),
                                "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
                            )
                        })
                        .map(|e| crate::connections::models::ReasoningEffortOption {
                            reasoning_effort: e.clone(),
                            description: e,
                        })
                        .collect()
                },
            })
            .collect(),
    })
}

//! Workflow-owned adapter for durable cross-mode tasks. No Workspace runtime state.
use super::*;
mod pins;
#[cfg(test)]
mod tests;
pub use pins::{reconcile_failed_pins, release_abandoned_pin};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinnedReview {
    pub profile_id: String,
    pub profile_name: String,
    pub workflow_json: String,
    pub settings: crate::settings::Settings,
    pub catalog_revision: String,
}
/// The automation operation that owns the running Review, with its run scope.
type ReviewOwner = (String, std::sync::Arc<crate::run_scope::RunScope>);
static OWNER: OnceLock<Mutex<Option<ReviewOwner>>> = OnceLock::new();
tokio::task_local! {static OPERATION:String;}
fn owner() -> &'static Mutex<Option<ReviewOwner>> {
    OWNER.get_or_init(Mutex::default)
}
struct OwnerGuard;
impl Drop for OwnerGuard {
    fn drop(&mut self) {
        *owner().lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
pub fn pin(profile_id: &str) -> Result<PinnedReview, String> {
    let snapshot = load_run_snapshot_for_profile(Some(profile_id))?;
    // Secret variables cannot be persisted in a portable task definition.
    // Bind them through the Review owner's saved defaults at dispatch time.
    let mut profile = crate::pipeline_config::ProfileData::from_config(
        snapshot.profile_name.clone(),
        &snapshot.config,
    );
    for variable in &mut profile.variables {
        if variable.secret {
            variable.default.clear();
        }
    }
    let document = crate::pipeline_config::WorkflowDocument::from_profile_data(profile)?;
    let mut settings = snapshot.settings;
    settings.anthropic_api_key.clear();
    settings.openai_api_key.clear();
    settings.google_api_key.clear();
    settings.local_api_key.clear();
    Ok(PinnedReview {
        profile_id: profile_id.into(),
        profile_name: snapshot.profile_name,
        workflow_json: document.canonical_json,
        settings,
        catalog_revision: snapshot.specialist_catalog_revision,
    })
}
pub fn available() -> bool {
    !pipeline_is_running()
}
pub fn cancel(operation: &str) -> bool {
    let owned = owner().lock().unwrap_or_else(|e| e.into_inner());
    match owned.as_ref() {
        Some((owner, scope)) if owner == operation => {
            scope.cancel();
            true
        }
        _ => false,
    }
}
pub(super) fn mark_run(dir: &std::path::Path) -> Result<(), String> {
    if let Ok(operation) = OPERATION.try_with(Clone::clone) {
        std::fs::write(dir.join(".task-pin"), &operation).map_err(|e| e.to_string())?;
        std::fs::write(
            dir.join("task-operation.json"),
            json!({"operation":operation}).to_string(),
        )
        .map_err(|e| e.to_string())?;
        let root = crate::orchestration::store::Store::default_root()?;
        crate::orchestration::adapters::atomic_json(
            &root
                .join("actions")
                .join(&operation)
                .join("review-run.json"),
            &json!({"runId":dir.file_name().ok_or("Invalid Review run directory")?.to_string_lossy()}),
        )?;
    }
    Ok(())
}
pub fn recovered_result(run_id: &str) -> Result<Option<Value>, String> {
    let manifest = crate::runs::load_manifest(run_id)?;
    if !["done", "degraded"].contains(&manifest.status.as_str()) {
        pins::release_settled_failure(run_id)?;
        return Ok(None);
    }
    let run_dir = crate::runs::runs_dir()?.join(run_id);
    recovered_value(&run_dir, run_id, &manifest).map(Some)
}

/// The adoptable result of a finished run. The captured text is optional
/// downstream and lives at the run's captured document path (never a
/// root-level document.md), so a missing or oversized copy must not keep a
/// completed Review from being reconciled.
fn recovered_value(
    run_dir: &std::path::Path,
    run_id: &str,
    manifest: &crate::runs::RunManifest,
) -> Result<Value, String> {
    let extracted_text = crate::runs::captured_document_rel_path(run_dir, manifest)
        .ok()
        .and_then(|rel| super::rerun::read_run_file_in(run_dir, rel).ok());
    Ok(json!({
        "run_id": run_id,
        "report": super::rerun::load_run_report_in(run_dir)?,
        "markdown": super::rerun::read_run_file_in(run_dir, "report.md")?,
        "extracted_text": extracted_text,
    }))
}
pub fn release_pin(run_id: &str) -> Result<(), String> {
    crate::runs::validate_run_id(run_id)?;
    let path = crate::runs::runs_dir()?.join(run_id).join(".task-pin");
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub async fn execute(
    operation: String,
    pinned: PinnedReview,
    input: String,
    interpretation: String,
    variables: std::collections::BTreeMap<String, String>,
    bus: crate::emit::EventBus,
    stop: tokio::sync::watch::Receiver<bool>,
) -> Result<Value, String> {
    let mut document =
        crate::pipeline_config::parse_workflow_document_strict(&pinned.workflow_json)?;
    if document.config.variables.iter().any(|v| v.secret) {
        let live = load_run_snapshot_for_profile(Some(&pinned.profile_id))?;
        for variable in &mut document.config.variables {
            if variable.secret {
                if variables.contains_key(&variable.key) {
                    return Err(
                        "Secret Review variables must come from the Review owner's configuration"
                            .into(),
                    );
                }
                variable.default = live
                    .config
                    .variables
                    .iter()
                    .find(|v| v.secret && v.key == variable.key)
                    .map(|v| v.default.clone())
                    .unwrap_or_default();
            }
        }
    }
    if !pinned.catalog_revision.is_empty()
        && pinned.catalog_revision != crate::auto_review::catalog_revision()
    {
        return Err(
            "The saved adaptive review catalog changed. Prepare a new task version.".into(),
        );
    }
    let current = crate::settings::load_persisted_required()?;
    let mut settings = pinned.settings;
    settings.anthropic_api_key = current.anthropic_api_key;
    settings.openai_api_key = current.openai_api_key;
    settings.google_api_key = current.google_api_key;
    settings.local_api_key = current.local_api_key;
    let fingerprint = stable_snapshot_fingerprint(&settings_for_snapshot_fingerprint(&settings))
        .map_err(|e| e.to_string())?;
    let snapshot = RunSnapshot {
        settings,
        config: document.config,
        profile_name: pinned.profile_name,
        config_fingerprint: fingerprint.clone(),
        fingerprint,
        workflow_source: format!("task-profile:{}", pinned.profile_id),
        workflow_fingerprint: document.fingerprint,
        workflow_json: document.canonical_json,
        specialist_catalog_revision: pinned.catalog_revision,
    };
    let vars = variables.into_iter().collect();
    let extra = std::collections::HashMap::new();
    validate_runtime_bindings(&snapshot.config, &vars, &extra, true)?;
    validate_primary_input_selection(&snapshot.config, Some(&input), Some(&interpretation))?;
    let dependencies = check_snapshot_dependencies(&snapshot, false, Some(&input), &extra).await?;
    require_snapshot_dependencies(&dependencies)?;
    if *stop.borrow() {
        return Err("Task stopped before Review dispatch".into());
    }
    let guard = acquire_pipeline_guard().map_err(|e| {
        if e.contains("already running") {
            format!("task_busy: {e}")
        } else {
            e
        }
    })?;
    let run = crate::run_scope::ActiveScope::begin(super::run_queue::next_run_key());
    *owner().lock().unwrap_or_else(|e| e.into_inner()) =
        Some((operation.clone(), run.scope().clone()));
    let owned = OwnerGuard;
    PipelineTask::spawn_future(run.scope().clone(), async move {
        let _guard = guard;
        let _owned = owned;
        let result = OPERATION
            .scope(
                operation,
                run_pipeline_inner_with_snapshot(
                    &bus,
                    &input,
                    Some(&interpretation),
                    false,
                    vars,
                    extra,
                    Some(snapshot),
                ),
            )
            .await;
        drop(_owned);
        if result.is_err() {
            if let Err(error) = reconcile_failed_pins() {
                eprintln!("Review retention reconciliation needs attention: {error}");
            }
        }
        result
    })
    .join()
    .await
}

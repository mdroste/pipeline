// ── Batch queue (1.3.1) ─────────────────────────────────────────────
//
// A batch is a list of input paths run one at a time through the normal
// pipeline (parallelism stays *inside* a run). The worker holds the same
// PIPELINE_RUNNING guard as a single run, so the two can never overlap. Job
// state is published to the frontend via `batch:progress` events and pollable
// via `get_batch_status`.

/// One job in a batch. Serialized to the frontend as-is.
#[derive(Clone, serde::Serialize)]
pub struct BatchJob {
    pub path: String,
    pub name: String,
    /// "pending" | "running" | "done" | "failed" | "cancelled".
    pub status: String,
    pub run_id: Option<String>,
    pub error: Option<String>,
    pub duration_secs: u64,
    /// Immutable profile used by this batch job.
    pub profile_id: String,
    /// Content fingerprint shared by every job in one batch.
    pub profile_snapshot_id: String,
}

pub(super) static BATCH: std::sync::Mutex<Vec<BatchJob>> = std::sync::Mutex::new(Vec::new());
pub(super) static BATCH_CANCEL: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(super) fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

pub(super) fn extra_input_artifact_path(index: usize, key: &str) -> String {
    let slug: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "context/inputs/{index:02}_{}.txt",
        if slug.is_empty() { "input" } else { &slug }
    )
}

pub(super) fn emit_batch(app: &crate::emit::EventBus) {
    let jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let _ = app.emit_event(
        "batch:progress",
        serde_json::to_value(&jobs).unwrap_or_default(),
    );
}

/// Stop every queued and running review and force-stop the child processes of
/// every run scope, including the shared fallback. The app is exiting, and
/// process-group children such as `llama-server` outlive it unless they are
/// signalled here: nothing else runs their drop guards.
pub(crate) fn stop_all_runs() {
    super::run_queue::cancel_all();
    crate::cancellation::stop_scopes(crate::run_scope::all());
}

#[tauri::command]
pub async fn cancel_pass(pass_key: String, run_key: Option<String>) -> Result<(), String> {
    crate::cancellation::cancel_pass(pass_key, run_key).await
}

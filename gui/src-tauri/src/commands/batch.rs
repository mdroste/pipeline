use super::*;

// --- Batch queue ---

/// Start a batch: run each input path through the pipeline sequentially with
/// the active profile and (optionally) shared variable values. Returns
/// immediately; progress arrives via `batch:progress` events and
/// `get_batch_status`. Fails if any run (single or batch) is already active.
#[tauri::command]
pub async fn start_batch(
    app: AppHandle,
    paths: Vec<String>,
    variables: Option<std::collections::HashMap<String, String>>,
    extra_inputs: Option<std::collections::HashMap<String, String>>,
    expected_profile_config_snapshot_id: Option<String>,
    run_parallel_overrides: Option<RunParallelOverrides>,
) -> Result<(), String> {
    let paths: Vec<String> = paths.into_iter().filter(|p| !p.trim().is_empty()).collect();
    if paths.is_empty() {
        return Err("No inputs to run".into());
    }
    // Capture and validate the complete run definition once. Later profile,
    // provider, or prompt edits apply only to a subsequent batch.
    let snapshot = load_run_snapshot()?;
    let expected_profile_config_snapshot_id = expected_profile_config_snapshot_id
        .as_deref()
        .filter(|expected| !expected.is_empty())
        .ok_or("Batch setup has not been prepared. Reload the active workflow before starting.")?;
    if expected_profile_config_snapshot_id != snapshot.config_fingerprint {
        return Err(
            "The active profile or settings changed while batch inputs were being collected. Reload the batch setup and try again."
                .to_string(),
        );
    }
    let snapshot = bind_parallel_overrides(snapshot, run_parallel_overrides.as_ref())?;
    if snapshot.config.extraction.input_mode.trim() == "none" {
        return Err("Batch processing requires a workflow that accepts input".to_string());
    }
    for path in &paths {
        validate_primary_input_selection(&snapshot.config, Some(path), Some("document"))?;
    }
    let vars = variables.unwrap_or_default();
    let inputs = extra_inputs.unwrap_or_default();
    crate::safety::validate_runtime_context(&vars, "Batch variables")?;
    crate::safety::validate_runtime_context(&inputs, "Batch named input paths")?;
    validate_runtime_bindings(&snapshot.config, &vars, &inputs, true)?;
    validate_named_input_paths(&snapshot.config, &inputs, true)?;
    let dependency_input = paths
        .iter()
        .find(|path| {
            !std::path::Path::new(path)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("tex") || extension.eq_ignore_ascii_case("docx")
                })
        })
        .or_else(|| paths.first());
    let dependencies = check_snapshot_dependencies(
        &snapshot,
        false,
        dependency_input.map(String::as_str),
        &inputs,
    )
    .await?;
    require_snapshot_dependencies(&dependencies)?;
    let snapshot = bind_runtime_snapshot(snapshot, &vars, &inputs)?;
    let guard = acquire_pipeline_guard()?;
    let batch_profile_id = snapshot.settings.active_profile.clone();
    let batch_snapshot_id = snapshot.fingerprint.clone();
    BATCH_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    {
        let mut jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner());
        *jobs = paths
            .iter()
            .map(|p| BatchJob {
                path: p.clone(),
                name: basename(p),
                status: "pending".to_string(),
                run_id: None,
                error: None,
                duration_secs: 0,
                profile_id: batch_profile_id.clone(),
                profile_snapshot_id: batch_snapshot_id.clone(),
            })
            .collect();
    }
    let bus = crate::emit::from_app(app);
    let run_bus = crate::emit::background(&bus);
    let _ = bus.emit_event(
        "pipeline:log",
        serde_json::json!({"line": format!(
            "Batch snapshot: profile '{}' ({})",
            batch_profile_id, batch_snapshot_id
        )}),
    );
    emit_batch(&bus);

    tauri::async_runtime::spawn(async move {
        // RAII guard clears PIPELINE_RUNNING and per-run state even on panic.
        let _guard = guard;
        // Clear the batch-stop flag on every exit path (normal, error, panic)
        // so it can never leak into a later foreground run, whose
        // begin_run_state now honors it to close the batch-cancel start race.
        struct BatchCancelReset;
        impl Drop for BatchCancelReset {
            fn drop(&mut self) {
                BATCH_CANCEL.store(false, std::sync::atomic::Ordering::Release);
            }
        }
        let _batch_cancel_reset = BatchCancelReset;
        for (i, path) in paths.iter().enumerate() {
            if BATCH_CANCEL.load(std::sync::atomic::Ordering::Acquire) {
                mark_remaining_cancelled(i);
                break;
            }
            set_job(i, |j| j.status = "running".to_string());
            emit_batch(&bus);

            let started = std::time::Instant::now();
            let result = run_pipeline_inner_with_snapshot(
                &run_bus,
                path,
                Some("document"),
                false,
                vars.clone(),
                inputs.clone(),
                Some(snapshot.clone()),
                // Batch cancellation is covered by the BATCH_CANCEL re-assert
                // inside the run; capture the epoch at dispatch so the
                // foreground-cancel check stays inert for batch jobs.
                current_cancel_epoch(),
            )
            .await;
            let secs = started.elapsed().as_secs();

            match result {
                Ok(v) => {
                    let run_id = v
                        .get("run_id")
                        .and_then(|r| r.as_str())
                        .map(|s| s.to_string());
                    let partial = v.get("status").and_then(|s| s.as_str()) == Some("partial");
                    set_job(i, |j| {
                        j.status = if partial { "failed" } else { "done" }.to_string();
                        j.run_id = run_id.clone();
                        if partial {
                            j.error = Some("One or more pipeline steps failed".to_string());
                        }
                        j.duration_secs = secs;
                    });
                }
                Err(e) => {
                    let cancelled = is_pipeline_cancellation_error(&e);
                    set_job(i, |j| {
                        j.status = if cancelled { "cancelled" } else { "failed" }.to_string();
                        j.error = Some(e.clone());
                        j.duration_secs = secs;
                    });
                }
            }
            emit_batch(&bus);
        }
        let _ = bus.emit_event("batch:done", serde_json::Value::Null);
    });

    Ok(())
}

pub(super) fn set_job(i: usize, f: impl FnOnce(&mut BatchJob)) {
    let mut jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(j) = jobs.get_mut(i) {
        f(j);
    }
}

pub(super) fn mark_remaining_cancelled(from: usize) {
    let mut jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner());
    for j in jobs.iter_mut().skip(from) {
        if j.status == "pending" || j.status == "running" {
            j.status = "cancelled".to_string();
        }
    }
}

/// Current batch job list (empty if no batch has run this session).
#[tauri::command]
pub async fn get_batch_status() -> Result<Vec<BatchJob>, String> {
    Ok(BATCH.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

/// Cancel a running batch: stop the current run and skip the rest.
#[tauri::command]
pub async fn cancel_batch() -> Result<(), String> {
    BATCH_CANCEL.store(true, std::sync::atomic::Ordering::Release);
    CANCEL_FLAG.store(true, std::sync::atomic::Ordering::Release);
    signal_cancellation();
    kill_all_children();
    Ok(())
}

/// Input files (PDF/LaTeX/Word) directly under `dir`, non-recursive and sorted;
/// hidden files skipped. Used by "queue this folder".
pub fn scan_input_files(dir: &str) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Cannot read folder: {e}"))?;
    let mut files: Vec<String> = Vec::new();
    let mut walk = crate::safety::WalkBudget::new("Input folder scan");
    for entry in entries.flatten() {
        walk.entry()?;
        let path = entry.path();
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if matches!(ext.as_str(), "pdf" | "tex" | "docx") {
            files.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
    files.sort();
    Ok(files)
}

/// List input files (PDF/LaTeX/Word) directly under `dir`, for "queue this folder".
#[tauri::command]
pub async fn list_input_files(dir: String) -> Result<Vec<String>, String> {
    scan_input_files(&dir)
}

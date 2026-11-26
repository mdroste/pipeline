//! Default GPU setup for parser owners, including upgrades from app-bundled
//! runtimes. This uses the same exclusive installer, progress and cancellation
//! as a manual install; failures leave the CPU parser intact.
use super::*;

#[derive(Debug, Serialize, Deserialize)]
struct SetupState {
    runtime_version: String,
    model_revision: String,
    retry_after: u64,
    dismissed: bool,
    message: Option<String>,
}

fn state_path() -> Result<PathBuf, String> {
    Ok(pipeline_home()?.join("native").join("gpu-setup.json"))
}

fn load_state() -> Result<Option<SetupState>, String> {
    let file = state_path()?;
    if !path_present(&file) {
        return Ok(None);
    }
    if !bounded_regular_file(&file, 64 * 1024) {
        return Err("Invalid graphics card setup state".into());
    }
    serde_json::from_slice(&std::fs::read(file).map_err(|e| e.to_string())?)
        .map(Some)
        .map_err(|e| e.to_string())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn due(state: Option<&SetupState>, now: u64) -> bool {
    state.is_none_or(|state| {
        !state.dismissed
            && (state.runtime_version != LLAMA_CPP_VERSION
                || state.model_revision != PADDLE_MODEL_REVISION
                || now >= state.retry_after)
    })
}

fn preferred() -> bool {
    crate::settings::load_persisted_required().is_ok_and(|settings| settings.paddle_use_gpu)
}

fn should_prepare() -> bool {
    if !gpu_recognition_listed() || !preferred() || paddle_full_parser_status().is_err() {
        return false;
    }
    let Ok(native) = pipeline_home().map(|home| home.join("native")) else {
        return false;
    };
    if gpu_install_committed(&gpu_recognition_root_at(&native)) && gpu_runtime_committed() {
        return false;
    }
    // Unreadable state does not authorize a new automatic download.
    load_state().is_ok_and(|state| due(state.as_ref(), now()))
}

pub(super) fn gpu_setup_message() -> Option<String> {
    load_state().ok().flatten().and_then(|state| state.message)
}

fn save_state(dismissed: bool, retry_secs: u64, message: Option<String>) -> Result<(), String> {
    let file = state_path()?;
    let parent = file.parent().ok_or("Missing graphics setup directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let state = SetupState {
        runtime_version: LLAMA_CPP_VERSION.into(),
        model_revision: PADDLE_MODEL_REVISION.into(),
        retry_after: now().saturating_add(retry_secs),
        dismissed,
        message,
    };
    let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    staged
        .write_all(&serde_json::to_vec(&state).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    crate::safety::sync_file(staged.as_file()).map_err(|e| e.to_string())?;
    staged.persist(&file).map_err(|e| e.to_string())?;
    Ok(())
}

/// How long a graphics-card step that yielded to a review waits before it
/// resumes, and how quiet the engine must be before automatic setup starts.
const YIELD_RETRY_SECS: u64 = 15 * 60;

pub(super) fn record_gpu_setup(result: &Result<bool, String>) -> Result<(), String> {
    record_gpu_setup_outcome(result, false)
}

/// `yielded`: the step stopped so a review could use the parser. That is a
/// pause, never the user's cancellation, which would end automatic setup.
fn record_gpu_setup_outcome(result: &Result<bool, String>, yielded: bool) -> Result<(), String> {
    match result {
        Err(_) if yielded => save_state(false, YIELD_RETRY_SECS,
            Some("Graphics card setup paused so a review could use the PDF parser. It resumes automatically; the processor remains available.".into())),
        Ok(true) => save_state(false, 0, None),
        Ok(false) => save_state(false, 7 * 24 * 60 * 60,
            Some("No supported graphics card was found. The parser uses the processor. You can retry after updating your graphics driver.".into())),
        Err(_) if INSTALL_CANCEL.load(Ordering::Acquire) => save_state(true, 0,
            Some("Graphics card setup was cancelled. Choose Install or Repair to resume; the processor remains available.".into())),
        Err(error) => save_state(false, 24 * 60 * 60,
            Some(format!("Graphics card setup did not finish: {error} Choose Install or Repair to retry. The processor remains available."))),
    }
}

pub(super) fn dismiss_gpu_setup() -> Result<(), String> {
    save_state(
        true,
        0,
        Some("Graphics card support was removed. Choose Install to enable it again.".into()),
    )
}

pub(super) fn engines_changed(app: &crate::emit::EventBus) {
    let _ = app.emit_event("engines:changed", serde_json::json!({}));
}

/// Caller owns the installer lock. The parser's successful installation is
/// not turned into a failure by this optional second step.
pub(super) async fn prepare_default_gpu_locked(app: &crate::emit::EventBus, progress_engine: &str) {
    if !should_prepare() {
        return;
    }
    let step = GpuStep::begin();
    emit_phase(app, progress_engine, "gpu", "running");
    let install = async {
        cleanup_managed_engine_storage_locked()?;
        if gpu_recognition_sound().await? {
            return Ok(true);
        }
        install_gpu_recognition(app, engine(GPU_RECOGNITION_ID)?).await
    };
    tokio::pin!(install);
    let mut yielded = false;
    let result = loop {
        tokio::select! {
            result = &mut install => break result,
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                // A document waiting for the parser outranks this optional step.
                yielded |= step.yield_requested();
                if yielded || !preferred() {
                    // Let the installer unwind through its normal cancellation
                    // path, including any activation rollback already in flight.
                    INSTALL_CANCEL.store(true, Ordering::Release);
                }
            }
        }
    };
    if let Err(error) = record_gpu_setup_outcome(&result, yielded) {
        log(
            app,
            format!("Could not save graphics card setup status: {error}"),
        );
    }
    if let Err(error) = &result {
        log(
            app,
            format!("PDF parser is installed; graphics card setup did not finish: {error}"),
        );
    }
    emit_phase(
        app,
        progress_engine,
        "gpu",
        if result.is_ok() { "done" } else { "failed" },
    );
}

/// Also handles existing parser owners after an app upgrade, driver changes,
/// or opting back in. Busy engines are left alone until the next check.
pub fn schedule_default_gpu_setup(app: crate::emit::EventBus) {
    if !gpu_recognition_listed() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        loop {
            // Never start the download while documents are using the parser,
            // including between the documents of a batch or another process.
            let prepare = tokio::task::spawn_blocking(|| {
                unix_millis().saturating_sub(last_engine_demand()) >= YIELD_RETRY_SECS * 1000
                    && should_prepare()
            })
            .await
            .unwrap_or(false);
            if prepare {
                if let Ok(guard) = acquire_install_guard() {
                    reset_install_progress(GPU_RECOGNITION_ID);
                    INSTALL_CANCEL.store(false, Ordering::Release);
                    engines_changed(&app);
                    prepare_default_gpu_locked(&app, GPU_RECOGNITION_ID).await;
                    drop(guard);
                    engines_changed(&app);
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> SetupState {
        SetupState {
            runtime_version: LLAMA_CPP_VERSION.into(),
            model_revision: PADDLE_MODEL_REVISION.into(),
            retry_after: 100,
            dismissed: false,
            message: None,
        }
    }
    #[test]
    fn first_install_and_runtime_upgrade_prepare_by_default() {
        assert!(due(None, 0));
        let mut saved = state();
        assert!(!due(Some(&saved), 99));
        assert!(due(Some(&saved), 100));
        saved.runtime_version = "previous".into();
        assert!(due(Some(&saved), 0));
    }
    #[test]
    fn explicit_removal_or_cancellation_survives_upgrades() {
        let mut saved = state();
        saved.dismissed = true;
        saved.runtime_version = "previous".into();
        assert!(!due(Some(&saved), 1000));
    }
}

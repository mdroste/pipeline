//! Tauri command handlers, grouped by lifecycle responsibility.

use crate::models::PipelineReport;
use crate::output;
use crate::pipeline::{executor, extract, orient, reconcile};
use crate::pipeline_config::{self, PipelineConfig, ProfileSummary};
use std::io::{Read, Write};
use tauri::AppHandle;

pub(crate) mod artifacts;
pub(crate) mod batch;
pub(crate) mod config;
pub(crate) mod export;
mod guard;
pub(crate) mod lifecycle;
pub(crate) mod orchestration;
mod profile_import;
pub(crate) mod rerun;
mod reuse_artifacts;
mod run;
mod run_context;
pub(crate) mod run_entry;
mod run_storage;
mod task;

pub use artifacts::*;
pub use batch::*;
pub use config::*;
pub use export::*;
pub use lifecycle::{
    await_or_cancel, cancel_pass, is_cancelled, is_pass_cancelled, is_pipeline_cancellation_error,
    register_child_pid, unregister_child_pid, wait_for_cancellation,
};
pub(crate) use lifecycle::{
    kill_all_children, kill_pass_children, kill_process, register_engine_child_pid,
    unregister_engine_child_pid,
};
pub use rerun::*;
pub use run_entry::{
    check_headless_dependencies, check_headless_plan, run_headless, run_headless_batch_item,
    run_headless_with_options, run_pipeline, HeadlessBatchSnapshot, HeadlessCheckReport,
    HeadlessRunOptions, HeadlessWorkflow,
};

use guard::*;
use lifecycle::*;
use profile_import::*;
use run::*;
use run_context::*;
use run_entry::*;
use run_storage::*;
use task::*;

/// Open a native "Save file" dialog and write `bytes` to the path the user
/// chooses, returning that path (or `None` if the user cancelled).
///
/// SECURITY: the destination is chosen by the OS dialog invoked from trusted
/// backend code — the webview supplies only a suggested file name, never a
/// path. This keeps a compromised renderer from turning an export command into
/// a write-anywhere primitive (overwriting `~/.pipeline/prompts/*`, shell
/// dotfiles, the encrypted keyfile, etc.).
pub(super) async fn save_via_dialog(
    app: &AppHandle,
    suggested_name: &str,
    filter_label: &str,
    extension: &str,
    bytes: Vec<u8>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    // A suggested name is only a dialog hint; strip any directory components so
    // it cannot pre-seed a path in the file-name field.
    let suggested = std::path::Path::new(suggested_name)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("export.{extension}"));
    let app = app.clone();
    let filter_label = filter_label.to_string();
    let extension = extension.to_string();
    // blocking_save_file dispatches the native panel to the main thread and
    // blocks the caller; run it off the async reactor.
    let chosen = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_file_name(suggested)
            .add_filter(filter_label, &[extension.as_str()])
            .blocking_save_file()
    })
    .await
    .map_err(|error| format!("Save dialog failed: {error}"))?;
    let Some(file) = chosen else {
        return Ok(None);
    };
    let path = file
        .into_path()
        .map_err(|error| format!("Invalid save location: {error}"))?;
    std::fs::write(&path, bytes).map_err(|error| format!("Failed to write file: {error}"))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

#[cfg(test)]
mod tests;

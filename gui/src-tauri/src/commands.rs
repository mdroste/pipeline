//! Tauri command handlers, grouped by lifecycle responsibility.

use crate::models::{PipelineReport, ReportSummary};
use crate::output;
use crate::pipeline::{executor, extract, orient, reconcile};
use crate::pipeline_config::{self, PipelineConfig, ProfileSummary};
use crate::storage;
use std::io::{Read, Write};
use tauri::AppHandle;

pub(crate) mod artifacts;
pub(crate) mod batch;
pub(crate) mod config;
pub(crate) mod export;
mod guard;
pub(crate) mod lifecycle;
mod profile_import;
pub(crate) mod rerun;
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
    await_or_cancel, cancel_pass, is_cancelled, is_pass_cancelled, register_child_pid,
    unregister_child_pid, wait_for_cancellation,
};
pub(crate) use lifecycle::{kill_all_children, kill_pass_children, kill_process};
pub use rerun::*;
pub use run_entry::{
    check_headless_dependencies, run_headless, run_headless_with_options, run_pipeline,
    HeadlessRunOptions,
};

use guard::*;
use lifecycle::*;
use profile_import::*;
use run::*;
use run_context::*;
use run_entry::*;
use run_storage::*;
use task::*;

#[cfg(test)]
mod tests;

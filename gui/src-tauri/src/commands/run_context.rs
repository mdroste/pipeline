//! Capture immutable Review launch settings and check concrete run inputs.

mod bindings;
mod models;
mod preflight;
mod snapshot;

pub(super) use bindings::{
    bind_runtime_snapshot, durable_variables, rerun_variables, secrets_to_reenter,
    validate_runtime_bindings,
};
pub(super) use models::{apply_model_overrides, bind_model_overrides, bind_review_limits};
pub use models::{ReviewLimitOverrides, ReviewModelChoice, RunModelOverrides};
#[cfg(test)]
pub(super) use preflight::{apply_workflow_codex_status, validate_primary_input_path};
pub(super) use preflight::{
    check_snapshot_dependencies, require_snapshot_dependencies, resolved_input_interpretation,
    validate_named_input_paths, validate_primary_input_selection,
};
#[cfg(test)]
pub(super) use snapshot::bind_style_examples_from;
pub(super) use snapshot::{
    bind_author_note, bind_foreground_launch, bind_style_examples, load_run_snapshot,
    load_run_snapshot_for_profile, load_run_snapshot_for_workflow,
    settings_for_snapshot_fingerprint, stable_content_fingerprint, stable_snapshot_fingerprint,
    RunSnapshot,
};

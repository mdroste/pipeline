//! Persisted settings and compatibility policy, with run-scoped snapshot access.
//! Serialization, model resolution, validation, and locked persistence retain
//! separate owners while the existing settings API remains available here.

mod crypto;
mod extraction;
mod model_policy;
mod persistence;
mod quarantine;
mod recovery;
mod run_snapshot;
mod types;
mod validation;

pub(crate) use extraction::resolve_pdf_extractor;
pub use extraction::{resolved_paddle_mtmd_batch_tokens, resolved_paddle_page_concurrency};
pub use persistence::{
    load_persisted, load_persisted_required, replace_active_profile_if, save,
    save_preserving_active, set_active_profile,
};
pub use recovery::{
    discard_undecryptable_secret, load_with_diagnostics, load_with_warnings, LoadDiagnostics,
};
pub use run_snapshot::{freeze_for_run, load, RunSettingsGuard};
pub use types::{ModelSelection, Settings};
pub use validation::sanitize_cli_arg;
pub(crate) use validation::validate_local_base_url;

#[cfg(test)]
mod tests;

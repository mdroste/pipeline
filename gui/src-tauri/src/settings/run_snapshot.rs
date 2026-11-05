//! Immutable Review settings and their run-scoped guard lifetime.

use super::recovery::load_with_warnings;
use super::types::Settings;

/// Releases a run's frozen settings snapshot when the run ends.
pub struct RunSettingsGuard {
    scope: std::sync::Arc<crate::run_scope::RunScope>,
    token: u64,
}

impl Drop for RunSettingsGuard {
    fn drop(&mut self) {
        self.scope.release_settings(self.token);
    }
}

/// Freeze `settings` for the run executing on this task.
pub fn freeze_for_run(settings: Settings) -> RunSettingsGuard {
    let scope = crate::run_scope::current();
    let token = scope.freeze_settings(settings);
    RunSettingsGuard { scope, token }
}

/// Load settings, discarding any warnings. Inside a run this is the run's
/// immutable snapshot; other callers (where fallback to defaults is fine) read
/// the persisted file.
pub fn load() -> Settings {
    crate::run_scope::current()
        .frozen_settings()
        .unwrap_or_else(|| load_with_warnings().0)
}

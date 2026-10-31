//! Default Workflow Codex subscription runtime. No Workspace state is used here.
//!
//! The App Server process starts lazily and lives as long as the app, not one run,
//! so sign-in, model discovery and calls never compete for one credential home.
//! Each call still gets a fresh thread and exactly one turn. The server receives
//! no research roots: file authority sits in the host tool handlers, and the
//! permission profile denies native writes and command networking. State lives
//! under ~/.pipeline/providers/workflows/, owned through `runtime.lock`.

pub mod connection;
mod invocation;
mod journal;
pub mod probe;
mod tools;

pub use connection::{
    shutdown, workflow_codex_login_cancel, workflow_codex_login_start, workflow_codex_logout,
    workflow_codex_rate_limits, workflow_codex_status,
};
pub use invocation::call_codex;

#[cfg(test)]
mod tests;

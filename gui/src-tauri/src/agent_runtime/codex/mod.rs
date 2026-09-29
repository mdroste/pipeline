//! Shared Codex protocol; runtime and storage ownership belong to callers.
pub(crate) mod account;
pub(crate) mod chatgpt;
pub(crate) mod compatibility;
pub(crate) mod invocation;
pub(crate) mod process;
#[cfg(test)]
pub(crate) mod simulator;
pub(crate) mod transport;
pub(crate) mod wire;
pub use account::*;
pub use transport::{AppServerClient, RequestError, DEFAULT_REQUEST_TIMEOUT};
pub use wire::{InitializeResult, NormalizedEvent};
pub(crate) mod session;

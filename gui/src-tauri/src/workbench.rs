//! App-owned research workspace services.
//!
//! Workspace is deliberately separate from the deterministic Pipeline/Review
//! run engine. `workbench` remains the internal compatibility namespace. The
//! service owns its identities, conversations, store, runtime, and cancellation.

pub(crate) mod api;
pub(crate) mod browser;
pub(crate) mod capabilities;
pub(crate) mod claude;
pub mod codex;
pub mod commands;
pub(crate) mod conversation_tools;
pub(crate) mod discovery;
pub(crate) mod history;
pub(crate) mod missions;
pub mod project;
pub mod release;
pub mod research;
pub(crate) mod shell;
mod shell_policy;
pub mod store;
pub(crate) mod submission;
pub(crate) mod tasks;
pub mod titles;
pub(crate) mod web;

pub mod acquisition;
pub mod data;
pub mod desk;
pub mod search;

pub mod programs;

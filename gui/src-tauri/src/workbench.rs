//! App-owned research workspace services.
//!
//! Workspace is deliberately separate from the deterministic Pipeline/Review
//! run engine. `workbench` remains the internal compatibility namespace. The
//! service owns its identities, conversations, store, runtime, and cancellation.

pub mod codex;
pub mod commands;
pub(crate) mod missions;
pub mod project;
pub mod release;
pub mod research;
pub mod store;
pub(crate) mod tasks;
pub mod titles;

pub mod acquisition;
pub mod data;
pub mod desk;
pub mod search;

pub mod programs;

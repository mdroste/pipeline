//! Deterministic host analyzers for Reviews. Each analyzer reads run inputs
//! and products and records what it checked; none calls a model or edits
//! model text.

pub mod fold;
pub mod quotes;

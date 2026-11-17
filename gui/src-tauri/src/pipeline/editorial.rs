//! Declaude's host-owned editorial contract. Models propose edits and judge
//! fidelity; the host binds quotations to one immutable readable-text snapshot,
//! checks lineage and edit interactions, and renders accepted records literally.
//! An exact text match is not a claim that PDF extraction or meaning is correct.
//! Unmarked legacy workflows retain their original behavior.

mod context;
mod render;
pub(crate) mod schemas;
mod validate;

pub(crate) use context::{Context, Inputs};
use serde::{Deserialize, Serialize};

pub const STAGE_KEY: &str = "x-pipeline-editorial-stage";
pub const STAGES: [&str; 3] = ["candidates-v1", "consolidated-v1", "verified-v1"];

pub fn stage(schema: &serde_json::Value) -> Option<&str> {
    schema.get(STAGE_KEY).and_then(serde_json::Value::as_str)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Coverage {
    reviewed: Vec<String>,
    limitations: Vec<String>,
    incomplete: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Anchor {
    original: String,
    prefix: String,
    suffix: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Meaning {
    source_claim: String,
    preserved_as: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Edit {
    id: String,
    location: String,
    anchor: Anchor,
    replacement: String,
    operation: String,
    reason: String,
    priority: String,
    meaning: Vec<Meaning>,
    referee_ids: Vec<String>,
    source_ids: Vec<String>,
    requires: Vec<String>,
    deletion_basis: String,
    retained_at: Vec<Anchor>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Unresolved {
    location: String,
    original: String,
    issue: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Candidates {
    stage: String,
    source_hash: String,
    coverage: Coverage,
    edits: Vec<Edit>,
    unresolved: Vec<Unresolved>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Decision {
    candidate_id: String,
    decision: String,
    reason: String,
    meaning: Vec<Meaning>,
    interaction_check: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Verification {
    stage: String,
    source_hash: String,
    coverage: Coverage,
    decisions: Vec<Decision>,
    unresolved: Vec<Unresolved>,
}

#[cfg(test)]
mod tests;

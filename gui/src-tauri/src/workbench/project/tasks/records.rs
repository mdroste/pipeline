//! Saved project task, checkpoint, and application records.

use crate::workbench::project::FileEntry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchTask {
    pub objective: String,
    pub anchor_id: Option<String>,
    pub expected_outputs: Vec<String>,
    pub expected_checks: Vec<String>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub task_id: String,
    pub backend: String,
    pub root_identity: String,
    pub task_root: String,
    pub task_root_identity: String,
    pub base: BTreeMap<String, FileEntry>,
    pub proposed: BTreeMap<String, FileEntry>,
    pub state: String,
    pub instruction: String,
    pub git: Option<Value>,
    pub limitations: Vec<String>,
    pub checked_execution_ids: Vec<String>,
    pub captured_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedFile {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub before_executable: bool,
    pub after_executable: bool,
    pub claim: String,
    pub state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Application {
    pub checkpoint_id: String,
    pub root_identity: String,
    pub state: String,
    pub files: Vec<AppliedFile>,
    pub direction: String,
    pub error: Option<String>,
    pub checks_valid: bool,
}

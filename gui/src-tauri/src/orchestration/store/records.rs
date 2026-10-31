//! Durable task and schedule records.
use crate::orchestration::{
    definition::Chain,
    state::{Progress, Receipt},
    triggers::Trigger,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub session_cursor: Option<String>,
    pub harness_fingerprint: Option<String>,
    #[serde(default)]
    pub connection_id: Option<crate::connections::ConnectionId>,
    pub runtime_root: Option<String>,
    #[serde(default)]
    pub root_identity: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, crate::commands::orchestration::PinnedReview>,
    #[serde(default)]
    pub checks: BTreeMap<String, String>,
    #[serde(default)]
    pub captured_checks: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRun {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission_id: Option<String>,
    pub revision: i64,
    pub name: String,
    pub state: String,
    pub reason: Option<String>,
    pub chain: Chain,
    pub scope: Scope,
    pub inputs: Value,
    pub progress: Progress,
    pub created_at: i64,
    pub updated_at: i64,
    pub due_at: Option<i64>,
    pub deadline_at: i64,
    pub schedule_id: Option<String>,
    pub occurrence: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub revision: i64,
    pub name: String,
    pub state: String,
    pub reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub due_at: Option<i64>,
    pub session_id: Option<String>,
    pub schedule_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub id: String,
    pub revision: i64,
    pub name: String,
    pub enabled: bool,
    pub chain: Chain,
    pub scope: Scope,
    pub inputs: Value,
    pub trigger: Trigger,
    pub next_due_at: Option<i64>,
    pub created_at: i64,
    pub tzdb_version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub sequence: i64,
    pub at: i64,
    pub kind: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempts: Vec<Receipt>,
}

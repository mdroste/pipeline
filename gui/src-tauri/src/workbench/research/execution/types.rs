//! Host execution profiles, receipts, process outputs, and structured-result DTOs.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionProfile {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub adapter: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub environment: Value,
    pub inputs: Vec<String>,
    pub timeout_seconds: u64,
    pub outputs: Vec<String>,
    pub tested_at: Option<String>,
    pub test_status: Option<String>,
    pub revision: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveExecutionProfileRequest {
    pub profile_id: Option<String>,
    pub workspace_id: String,
    pub name: String,
    pub adapter: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub environment: Value,
    pub inputs: Vec<String>,
    pub timeout_seconds: u64,
    pub outputs: Vec<String>,
    pub expected_revision: Option<i64>,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchExecution {
    pub id: String,
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub profile_id: Option<String>,
    pub adapter: String,
    pub command: Vec<String>,
    pub cwd: String,
    pub input_manifest: Value,
    pub dependency_hash: String,
    pub outcome: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub exit_status: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub output_manifest: Value,
    pub validation: Value,
    pub snapshot_consistency: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunExecutionRequest {
    #[serde(default)]
    pub plan_id: Option<String>,
    pub profile_id: String,
    pub session_id: Option<String>,
    pub test_only: bool,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelExecutionRequest {
    pub execution_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExecutionOutput {
    pub(super) outcome: String,
    pub(super) status: Option<i32>,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
    pub(super) stdout_truncated: bool,
    pub(super) stderr_truncated: bool,
    pub(super) wrapper_completed_after_stop: bool,
    pub(super) process_started: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchResultV1 {
    pub result_id: String,
    pub estimand: String,
    pub specification_id: String,
    pub sample_id: String,
    pub estimate: f64,
    pub standard_error: Option<f64>,
    pub confidence_interval: Option<[f64; 2]>,
    pub n: Option<u64>,
    pub units: String,
    pub transformation: Option<String>,
    pub uncertainty_method: Option<String>,
    pub source_execution_id: String,
    pub artifact_locator: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareResultsRequest {
    pub left: ResearchResultV1,
    pub right: ResearchResultV1,
    pub absolute_tolerance: f64,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResultComparison {
    pub comparable: bool,
    pub passed: bool,
    pub absolute_difference: Option<f64>,
    pub tolerance: f64,
    pub incompatibilities: Vec<String>,
    pub limitations: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordStructuredResultRequest {
    pub workspace_id: String,
    pub result: ResearchResultV1,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationRecord {
    pub id: String,
    pub evidence_link_id: String,
    pub method: String,
    pub checker_identity: String,
    pub input_hashes: Value,
    pub observed_result: Value,
    pub limitations: String,
    pub passed: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyEvidenceResultsRequest {
    pub evidence_id: String,
    pub left_result_id: String,
    pub right_result_id: String,
    pub absolute_tolerance: f64,
    pub rationale: Option<String>,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostExecutionPreview {
    pub profile_id: String,
    pub fingerprint: String,
    pub command: Vec<String>,
    pub cwd: String,
    pub inputs: Value,
    pub launch: Value,
    pub outputs: Vec<String>,
    pub authorized: bool,
    pub boundary: String,
}

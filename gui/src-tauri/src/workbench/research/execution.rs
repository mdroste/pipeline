//! Explicitly authorized host execution profiles, receipts, and validation.

use super::{open_connection, validate_id};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::OptionalExtension as _;
use serde_json::Value;

mod adapters;
mod authorization;
mod file_identity;
mod launch;
mod lifecycle;
mod outputs;
mod process;
mod profiles;
mod results;
mod toolchains;
mod types;

pub(crate) use authorization::input_manifest;
pub use authorization::{authorize_host_execution, preview_host_execution};
pub(super) use file_identity::{hash_regular_file, SNAPSHOT_BUDGET};
pub(super) use launch::launch_manifest;
#[cfg(test)]
pub use lifecycle::run_execution;
pub use lifecycle::{cancel_execution, cancel_executions_for_provider_turn};
pub(crate) use lifecycle::{
    finalize_execution, prepare_execution, start_execution, wait_execution, PreparedExecution,
    PreparedOrExisting,
};
pub(super) use outputs::validate_execution_outputs;
pub(crate) use process::{cancelled_executions, run_profile_process};
pub(crate) use profiles::get_execution_profile;
#[cfg(test)]
use profiles::latex_engine;
pub(super) use profiles::safe_relative_path;
pub use profiles::{list_execution_profiles, save_execution_profile};
pub(crate) use results::validate_result;
pub use results::{
    compare_results, list_structured_results, record_structured_result, verify_evidence_results,
};
pub use toolchains::{
    detect_toolchains, DetectedToolchain, EnvironmentFile, ProjectEnvironment, PythonEnvironment,
    ToolchainReport,
};
pub(crate) use types::ExecutionOutput;
pub use types::{
    CancelExecutionRequest, CompareResultsRequest, ExecutionProfile, HostExecutionPreview,
    RecordStructuredResultRequest, ResearchExecution, ResearchResultV1, ResultComparison,
    RunExecutionRequest, SaveExecutionProfileRequest, VerificationRecord,
    VerifyEvidenceResultsRequest,
};

#[cfg(test)]
mod adapter_tests;
#[cfg(all(test, unix))]
mod contract_tests;
#[cfg(test)]
mod platform_tests;

fn execution_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchExecution> {
    let command: String = row.get(5)?;
    let input: String = row.get(7)?;
    let output: String = row.get(15)?;
    let validation: String = row.get(16)?;
    Ok(ResearchExecution {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        session_id: row.get(2)?,
        profile_id: row.get(3)?,
        adapter: row.get(4)?,
        command: serde_json::from_str(&command).unwrap_or_default(),
        cwd: row.get(6)?,
        input_manifest: serde_json::from_str(&input).unwrap_or(Value::Null),
        dependency_hash: row.get(8)?,
        outcome: row.get(9)?,
        started_at: row.get(10)?,
        ended_at: row.get(11)?,
        exit_status: row.get(12)?,
        stdout: row.get(13)?,
        stderr: row.get(14)?,
        output_manifest: serde_json::from_str(&output).unwrap_or(Value::Null),
        validation: serde_json::from_str(&validation).unwrap_or(Value::Null),
        snapshot_consistency: row.get(17)?,
        created_at: row.get(18)?,
    })
}

const EXECUTION_SELECT: &str = "SELECT id, workspace_id, session_id, profile_id, adapter, command_json, cwd, input_manifest_json, dependency_hash, outcome, started_at, ended_at, exit_status, stdout_text, stderr_text, output_manifest_json, validation_json, snapshot_consistency, created_at FROM research_executions";

/// Lists are polled and each run's output can reach 8 MiB, so they carry none.
/// `jobs::execution_output` reads it for the one run someone opens.
const EXECUTION_LIST_SELECT: &str = "SELECT id, workspace_id, session_id, profile_id, adapter, command_json, cwd, input_manifest_json, dependency_hash, outcome, started_at, ended_at, exit_status, NULL, NULL, output_manifest_json, validation_json, snapshot_consistency, created_at FROM research_executions";

pub(crate) fn get_execution(
    store: &Store,
    execution_id: &str,
) -> WorkbenchResult<ResearchExecution> {
    validate_id("execution id", execution_id)?;
    open_connection(store)?
        .query_row(
            &format!("{EXECUTION_SELECT} WHERE id = ?1"),
            [execution_id],
            execution_from_row,
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to read research execution", error))?
        .ok_or_else(|| WorkbenchError::invalid("Research execution was not found"))
}

pub fn list_executions(
    store: &Store,
    workspace_id: &str,
) -> WorkbenchResult<Vec<ResearchExecution>> {
    validate_id("workspace id", workspace_id)?;
    let connection = open_connection(store)?;
    let mut statement = connection
        .prepare(&format!(
            "{EXECUTION_LIST_SELECT} WHERE workspace_id = ?1 ORDER BY created_at DESC LIMIT 200"
        ))
        .map_err(|error| WorkbenchError::storage("Failed to prepare execution list", error))?;
    let values = statement
        .query_map([workspace_id], execution_from_row)
        .map_err(|error| WorkbenchError::storage("Failed to list executions", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode executions", error))?;
    Ok(values)
}

//! Structured-result storage, declared compatibility, and verification receipts.

use super::get_execution;
use super::types::{
    CompareResultsRequest, RecordStructuredResultRequest, ResearchResultV1, ResultComparison,
    VerificationRecord, VerifyEvidenceResultsRequest,
};
use crate::workbench::research::{
    append_change, new_id, now, open_connection, validate_id, validate_text,
};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _, TransactionBehavior};
use serde_json::{json, Value};

pub(crate) fn validate_result(result: &ResearchResultV1) -> WorkbenchResult<()> {
    for (label, value) in [
        ("result id", &result.result_id),
        ("specification id", &result.specification_id),
        ("sample id", &result.sample_id),
        ("source execution id", &result.source_execution_id),
    ] {
        validate_id(label, value)?;
    }
    for value in [result.estimate]
        .into_iter()
        .chain(result.standard_error)
        .chain(result.confidence_interval.into_iter().flatten())
    {
        if !value.is_finite() {
            return Err(WorkbenchError::invalid(
                "Structured results require finite numeric values",
            ));
        }
    }
    if result.standard_error.is_some_and(|s| s < 0.)
        || result.confidence_interval.is_some_and(|v| v[0] > v[1])
    {
        return Err(WorkbenchError::invalid(
            "Invalid uncertainty: standard errors must be nonnegative and intervals ordered",
        ));
    }
    validate_text("Estimand", &result.estimand, 1_000)?;
    validate_text("Units", &result.units, 300)?;
    validate_text("Artifact locator", &result.artifact_locator, 4_096)?;
    Ok(())
}

pub fn record_structured_result(
    store: &Store,
    request: RecordStructuredResultRequest,
) -> WorkbenchResult<ResearchResultV1> {
    validate_id("workspace id", &request.workspace_id)?;
    validate_result(&request.result)?;
    let execution = get_execution(store, &request.result.source_execution_id)?;
    if execution.workspace_id != request.workspace_id || execution.outcome != "completed" {
        return Err(WorkbenchError::invalid(
            "Structured results require a completed execution in this workspace",
        ));
    }
    let locator_path = request
        .result
        .artifact_locator
        .split('#')
        .next()
        .unwrap_or_default();
    let locator_valid = execution
        .output_manifest
        .get("artifacts")
        .and_then(Value::as_array)
        .is_some_and(|artifacts| {
            artifacts.iter().any(|artifact| {
                artifact.get("path").and_then(Value::as_str) == Some(locator_path)
                    || artifact.get("artifactId").and_then(Value::as_str) == Some(locator_path)
            })
        });
    if !locator_valid {
        return Err(WorkbenchError::invalid(
            "Structured result artifact locator is not present in the execution output manifest",
        ));
    }
    let id = new_id("structured")?;
    let body = serde_json::to_string(&request.result)
        .map_err(|error| WorkbenchError::storage("Failed to encode structured result", error))?;
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            WorkbenchError::storage("Failed to start structured result recording", error)
        })?;
    transaction.execute("INSERT INTO structured_results (id, execution_id, result_id, body_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)", params![id, request.result.source_execution_id, request.result.result_id, body, now()]).map_err(|error| WorkbenchError::storage("Failed to record structured result", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "structured_result",
        &id,
        (Some(&request.workspace_id), None),
        "recorded",
        &json!({"resultId":request.result.result_id,"executionId":request.result.source_execution_id}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit structured result", error))?;
    Ok(request.result)
}

pub fn list_structured_results(
    store: &Store,
    workspace_id: &str,
) -> WorkbenchResult<Vec<ResearchResultV1>> {
    validate_id("workspace id", workspace_id)?;
    let connection = open_connection(store)?;
    let mut statement = connection.prepare("SELECT sr.body_json FROM structured_results sr JOIN research_executions e ON e.id=sr.execution_id WHERE e.workspace_id=?1 ORDER BY sr.created_at DESC LIMIT 1000").map_err(|error| WorkbenchError::storage("Failed to prepare structured result list", error))?;
    let bodies = statement
        .query_map([workspace_id], |row| row.get::<_, String>(0))
        .map_err(|error| WorkbenchError::storage("Failed to list structured results", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            WorkbenchError::storage("Failed to decode structured result rows", error)
        })?;
    bodies
        .into_iter()
        .map(|body| {
            serde_json::from_str(&body).map_err(|error| {
                WorkbenchError::storage("Stored structured result is invalid", error)
            })
        })
        .collect()
}

fn verification_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<VerificationRecord> {
    let hashes: String = row.get(4)?;
    let observed: String = row.get(5)?;
    Ok(VerificationRecord {
        id: row.get(0)?,
        evidence_link_id: row.get(1)?,
        method: row.get(2)?,
        checker_identity: row.get(3)?,
        input_hashes: serde_json::from_str(&hashes).unwrap_or(Value::Null),
        observed_result: serde_json::from_str(&observed).unwrap_or(Value::Null),
        limitations: row.get(6)?,
        passed: row.get(7)?,
        created_at: row.get(8)?,
    })
}

pub fn verify_evidence_results(
    store: &Store,
    request: VerifyEvidenceResultsRequest,
) -> WorkbenchResult<VerificationRecord> {
    validate_id("evidence id", &request.evidence_id)?;
    let connection = open_connection(store)?;
    let workspace_id: String = connection
        .query_row(
            "SELECT workspace_id FROM evidence_links WHERE id=?1",
            [&request.evidence_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            WorkbenchError::storage("Failed to inspect evidence for verification", error)
        })?
        .ok_or_else(|| WorkbenchError::invalid("Evidence link was not found"))?;
    let load_result = |result_id: &str| -> WorkbenchResult<(ResearchResultV1, String)> {
        validate_id("result id", result_id)?;
        let (body, dependency): (String, String) = connection.query_row("SELECT sr.body_json, e.dependency_hash FROM structured_results sr JOIN research_executions e ON e.id=sr.execution_id WHERE sr.result_id=?1 AND e.workspace_id=?2 ORDER BY sr.created_at DESC LIMIT 1", params![result_id, workspace_id], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(|error| WorkbenchError::storage("Failed to load verification input", error))?.ok_or_else(|| WorkbenchError::invalid("Verification result was not found in this workspace"))?;
        Ok((
            serde_json::from_str(&body).map_err(|error| {
                WorkbenchError::storage("Stored verification result is invalid", error)
            })?,
            dependency,
        ))
    };
    let (left, left_hash) = load_result(&request.left_result_id)?;
    let (right, right_hash) = load_result(&request.right_result_id)?;
    let comparison = compare_results(CompareResultsRequest {
        left,
        right,
        absolute_tolerance: request.absolute_tolerance,
        rationale: request.rationale,
    })?;
    let id = new_id("verification")?;
    let timestamp = now();
    let hashes = json!({"leftDependencyHash":left_hash,"rightDependencyHash":right_hash});
    let observed = serde_json::to_value(&comparison)
        .map_err(|error| WorkbenchError::storage("Failed to encode verification result", error))?;
    let mut write_connection = open_connection(store)?;
    let transaction = write_connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start evidence verification", error))?;
    transaction.execute("INSERT INTO verification_records (id, evidence_link_id, method, checker_identity, input_hashes_json, observed_result_json, limitations, passed, created_at) VALUES (?1, ?2, 'research-results-v1 comparison', 'workspace-host-v1', ?3, ?4, ?5, ?6, ?7)", params![id, request.evidence_id, serde_json::to_string(&hashes).unwrap_or_default(), serde_json::to_string(&observed).unwrap_or_default(), comparison.limitations, comparison.passed, timestamp]).map_err(|error| WorkbenchError::storage("Failed to record evidence verification", error))?;
    transaction.execute("UPDATE evidence_links SET assessment=?2, assessor='workspace-host-v1', freshness='current', stale_reason=NULL, updated_at=?3 WHERE id=?1", params![request.evidence_id, if comparison.passed {"check_passed"} else {"check_failed"}, timestamp]).map_err(|error| WorkbenchError::storage("Failed to apply evidence verification", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "verification",
        &id,
        (Some(&workspace_id), None),
        "recorded",
        &json!({"evidenceId":request.evidence_id,"passed":comparison.passed}),
    )?;
    transaction.commit().map_err(|error| {
        WorkbenchError::storage("Failed to commit evidence verification", error)
    })?;
    open_connection(store)?.query_row("SELECT id, evidence_link_id, method, checker_identity, input_hashes_json, observed_result_json, limitations, passed, created_at FROM verification_records WHERE id=?1", [&id], verification_from_row).map_err(|error| WorkbenchError::storage("Failed to read verification record", error))
}

pub fn compare_results(request: CompareResultsRequest) -> WorkbenchResult<ResultComparison> {
    validate_result(&request.left)?;
    validate_result(&request.right)?;
    if !request.absolute_tolerance.is_finite() || request.absolute_tolerance < 0.0 {
        return Err(WorkbenchError::invalid(
            "Result comparison tolerance must be finite and nonnegative",
        ));
    }
    let mut incompatibilities = Vec::new();
    for (label, left, right) in [
        (
            "estimand",
            request.left.estimand.as_str(),
            request.right.estimand.as_str(),
        ),
        (
            "specification",
            request.left.specification_id.as_str(),
            request.right.specification_id.as_str(),
        ),
        (
            "sample",
            request.left.sample_id.as_str(),
            request.right.sample_id.as_str(),
        ),
        (
            "units",
            request.left.units.as_str(),
            request.right.units.as_str(),
        ),
        (
            "transformation",
            request.left.transformation.as_deref().unwrap_or(""),
            request.right.transformation.as_deref().unwrap_or(""),
        ),
    ] {
        if left != right {
            incompatibilities.push(label.to_string());
        }
    }
    let rationale = request
        .rationale
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let quantity_compatible = request.left.units == request.right.units
        && request.left.transformation == request.right.transformation
        && request.left.estimand == request.right.estimand;
    let comparable = quantity_compatible && (incompatibilities.is_empty() || rationale.is_some());
    let difference = comparable.then_some((request.left.estimate - request.right.estimate).abs());
    Ok(ResultComparison { comparable, passed: difference.is_some_and(|value| value <= request.absolute_tolerance), absolute_difference:difference, tolerance:request.absolute_tolerance, incompatibilities, limitations:"This check compares declared numeric results only; it does not establish identification, causal interpretation, or theoretical validity.".to_string() })
}

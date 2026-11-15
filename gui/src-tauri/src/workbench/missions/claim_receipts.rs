//! The Workspace owner supplies a bounded, read-only projection of terminal
//! project executions. A result recorded for a different check cannot enter the packet.
use super::{json, Store, Value, WorkbenchError, WorkbenchResult};
fn structured(store: &Store, id: &str) -> WorkbenchResult<Vec<Value>> {
    let connection = store.connection()?;
    let mut q = connection
        .prepare(
            "SELECT body_json FROM structured_results WHERE execution_id=?1 ORDER BY id LIMIT 32",
        )
        .map_err(super::error)?;
    let rows = q
        .query_map([id], |r| r.get::<_, String>(0))
        .map_err(super::error)?;
    let mut out = Vec::new();
    for row in rows {
        let body = row.map_err(super::error)?;
        if body.len() > 32 * 1024 {
            return Err(WorkbenchError::invalid(
                "Structured result exceeds the assessment limit",
            ));
        }
        let v: Value = serde_json::from_str(&body).map_err(super::error)?;
        out.push(json!({"id":v["resultId"],"estimate":v["estimate"],"sample":v["sampleId"],"specification":v["specificationId"],"location":v["artifactLocator"]}));
    }
    Ok(out)
}
/// Read terminal receipts from this project without granting or launching any computation.
pub(crate) fn execution_evidence(
    store: &Store,
    ws: &str,
    ids: Vec<String>,
) -> WorkbenchResult<Vec<Value>> {
    let mut checks = Vec::new();
    for id in ids {
        let r = crate::workbench::research::get_execution(store, &id)?;
        if r.workspace_id != ws
            || !matches!(
                r.outcome.as_str(),
                "completed" | "failed" | "interrupted" | "timed_out"
            )
        {
            return Err(WorkbenchError::invalid(
                "Execution is not a finished check in this project",
            ));
        }
        let results = structured(store, &r.id)?;
        let value = json!({"kind":"executionReceipt","result":{"passed":r.outcome=="completed","receipt":{"id":r.id,"outcome":r.outcome,"inputManifest":r.input_manifest,"command":r.command,"outputManifest":r.output_manifest,"dependencyHash":r.dependency_hash,"structuredResults":results,"stdout":r.stdout.as_deref().map(|s|s.chars().take(4000).collect::<String>())}}});
        if value.to_string().len() > 128 * 1024 {
            return Err(WorkbenchError::invalid(
                "Execution evidence exceeds its size limit",
            ));
        }
        checks.push(value);
    }
    Ok(checks)
}

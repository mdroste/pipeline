//! Materialize coordinator-owned artifacts for Workspace, without giving it a
//! path into the producer's store. Existing receipts are upgraded before GC.
use super::export_file;
use crate::orchestration::store::Store;
use crate::orchestration::store::{err, Result};
use crate::workbench::{
    commands::run_store,
    tasks::delivery::{self, DeliveryFile},
};
use serde_json::Value;
use std::collections::HashSet;

fn artifacts(value: &Value) -> Vec<&Value> {
    let mut artifacts = Vec::new();
    let mut visit = vec![value];
    while let Some(item) = visit.pop() {
        match item {
            Value::Object(fields)
                if matches!(item["kind"].as_str(), Some("artifact" | "artifactTree"))
                    && ["hash", "filename", "path"]
                        .iter()
                        .all(|key| fields.contains_key(*key)) =>
            {
                artifacts.push(item)
            }
            Value::Object(fields) => visit.extend(fields.values().rev()),
            Value::Array(items) => visit.extend(items.iter().rev()),
            _ => {}
        }
    }
    artifacts
}

/// Agent-generated JSON cannot confer read access to other Automation files.
/// Accept a captured step output or a handle explicitly included in the
/// researcher's inputs/definition (including Discovery's host-built delivery).
pub(crate) fn action_files(
    store: &Store,
    run: &crate::orchestration::store::TaskRun,
    binding: &crate::orchestration::definition::Binding,
    value: &Value,
) -> Result<Vec<DeliveryFile>> {
    use crate::orchestration::definition::{Action, Binding};
    fn literal_artifacts<'a>(binding: &'a Binding, grants: &mut Vec<&'a Value>) {
        match binding {
            Binding::Literal { value } => grants.extend(artifacts(value)),
            Binding::FirstAvailable { values } => {
                for item in values {
                    literal_artifacts(item, grants);
                }
            }
            _ => {}
        }
    }
    let mut steps = Vec::new();
    super::flatten(&run.chain.steps, &mut steps);
    let mut grants = artifacts(&run.inputs);
    literal_artifacts(binding, &mut grants);
    for step in &steps {
        let output = &run.progress.outputs[&step.id];
        match &step.action {
            Action::Snapshot { .. } => grants.push(output),
            Action::Review { .. } => grants.push(&output["reviewedArtifact"]),
            Action::Deliver { input } => literal_artifacts(input, &mut grants),
            _ => {}
        }
    }
    // A later loop pass may have replaced the step's latest full output.
    // Immutable receipt previews still identify earlier captured snapshots.
    for receipt in run
        .progress
        .receipts
        .values()
        .filter(|r| r.state == "completed")
    {
        if let (Some(step), Some(output)) = (
            steps.iter().find(|s| s.id == receipt.step_id),
            receipt.output.as_ref(),
        ) {
            match step.action {
                Action::Snapshot { .. } => grants.push(output),
                Action::Review { .. } => grants.push(&output["reviewedArtifact"]),
                _ => {}
            }
        }
    }
    for artifact in artifacts(value) {
        if !grants.iter().any(|g| {
            ["kind", "filename", "hash"]
                .iter()
                .all(|k| g[*k] == artifact[*k])
        }) {
            return Err("Only a captured or explicitly supplied file can be delivered".into());
        }
    }
    files(store, value)
}

pub(crate) fn files(store: &Store, value: &Value) -> Result<Vec<DeliveryFile>> {
    if value.to_string().len() > 512 * 1024 {
        return Err("Task delivery exceeds 512 KiB".into());
    }
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0usize;
    for item in artifacts(value) {
        let identity = (
            item["kind"].as_str(),
            item["hash"].as_str(),
            item["filename"].as_str(),
        );
        if !seen.insert(identity) {
            continue;
        }
        let file = export_file(store, item)?;
        total += file.bytes.len();
        if total > 256 * 1024 * 1024 || files.len() >= 1024 {
            return Err("Delivered files exceed 256 MiB or 1,024 attachments".into());
        }
        files.push(file);
    }
    Ok(files)
}

#[derive(Default)]
pub(crate) struct Backfill {
    pub warning: Option<String>,
    pub pending_operations: HashSet<String>,
    pub pending_digests: HashSet<String>,
}

fn legacy_files(
    source: Option<&Store>,
    session: &str,
    op: &str,
    value: &Value,
) -> Result<Vec<DeliveryFile>> {
    if artifacts(value).is_empty() {
        return Ok(Vec::new());
    }
    let source = source.ok_or("The original automation library is unavailable")?;
    let (run, step_id) = source
        .delivery_producer(session, op)?
        .ok_or("The original delivery record is unavailable")?;
    let mut steps = Vec::new();
    super::flatten(&run.chain.steps, &mut steps);
    let input = steps
        .iter()
        .find(|s| s.id == step_id)
        .and_then(|step| match &step.action {
            crate::orchestration::definition::Action::Deliver { input } => Some(input),
            _ => None,
        })
        .ok_or("The recorded action did not deliver files")?;
    action_files(source, &run, input, value)
}

/// Read old receipts in bounded pages. An unresolved receipt keeps its source
/// and named bytes until the next pass, without blocking unrelated cleanup.
pub(crate) async fn backfill(store: Option<&Store>, session: Option<String>) -> Result<Backfill> {
    let mut outcome = Backfill::default();
    #[cfg(test)]
    if std::env::var_os("PIPELINE_WORKBENCH_DEV_ROOT").is_none() {
        return Ok(outcome);
    }
    if !crate::workbench::store::Store::default_exists().map_err(|e| e.message)? {
        return Ok(outcome);
    }
    let mut after = String::new();
    loop {
        let scope = session.clone();
        let rows = run_store(move |w| delivery::pending(&w, scope.as_deref(), &after))
            .await
            .map_err(|e| e.message)?;
        let Some(last) = rows.last() else { break };
        after = last.0.clone();
        for (key, session, value) in rows {
            let op = key
                .strip_prefix("task-result-")
                .ok_or("Invalid legacy delivery identity")?
                .to_string();
            let digests = artifacts(&value)
                .into_iter()
                .filter_map(|v| v["hash"].as_str().map(str::to_owned))
                .collect::<Vec<_>>();
            let source = store.cloned();
            let operation = op.clone();
            let result = tokio::task::spawn_blocking(move || {
                let files = legacy_files(source.as_ref(), &session, &operation, &value)?;
                Ok::<_, String>((session, operation, value, files))
            })
            .await
            .map_err(err)?;
            let result = match result {
                Ok((session, op, value, files)) => {
                    run_store(move |w| delivery::deliver(&w, &session, &op, value, files))
                        .await
                        .map(|_| ())
                        .map_err(|e| e.message)
                }
                Err(e) => Err(e),
            };
            if let Err(e) = result {
                outcome
                    .warning
                    .get_or_insert(format!("Could not recover files for {key}: {e}"));
                outcome.pending_operations.insert(op);
                outcome.pending_digests.extend(digests);
            }
        }
    }
    Ok(outcome)
}

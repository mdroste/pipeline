//! Existing durable-result search and reconciliation order.
use super::adapters::atomic_json;
use super::store::{err, now, Result, TaskRun};
use super::{adapters, Coordinator};
use serde_json::Value;

impl Coordinator {
    pub(crate) async fn recover_results(&self, run: &mut TaskRun) -> Result<()> {
        for receipt in run
            .progress
            .receipts
            .values_mut()
            .filter(|r| r.state == "unknown")
        {
            let path = self
                .store
                .root
                .join("actions")
                .join(&receipt.operation)
                .join("result.json");
            let mut result = None;
            if path.is_file() {
                let local = path.clone();
                result = self
                    .db(move |_| {
                        let bytes = std::fs::read(local).map_err(err)?;
                        serde_json::from_slice(&bytes).map(Some).map_err(err)
                    })
                    .await?;
            }
            if result.is_none() {
                let operation = receipt.operation.clone();
                let root = self.store.clone();
                result = self
                    .db(move |_| adapters::recover_review(&root, &operation))
                    .await?;
            }
            if result.is_none() {
                if let Some(session) = run.scope.session_id.clone() {
                    let operation = receipt.operation.clone();
                    result = crate::workbench::commands::run_store(move |s| {
                        crate::workbench::tasks::delivery_outcome(&s, &session, &operation)
                    })
                    .await
                    .map_err(|e| e.message)?;
                }
            }
            if result.is_none() {
                if let Some(session) = run.scope.session_id.clone() {
                    let operation = format!("task-{}", receipt.operation);
                    if let Some(value) = crate::workbench::commands::run_store(move |s| {
                        crate::workbench::tasks::outcome(&s, &session, &operation)
                    })
                    .await
                    .map_err(|e| e.message)?
                    {
                        if value["state"] == "completed" || adapters::terminal_failure(&value) {
                            result = Some(value);
                        }
                    }
                }
            }
            if result.is_none() {
                if let Some(workspace) = run.scope.workspace_id.clone() {
                    let operation = receipt.operation.clone();
                    result = crate::workbench::commands::run_store(move |s| {
                        crate::workbench::tasks::check_outcome(&s, &workspace, &operation)
                    })
                    .await
                    .map_err(|e| e.message)?;
                }
            }
            if let Some(value) = result {
                atomic_json(&path, &value)?;
                if let Some(id) = value["runId"].as_str() {
                    let id = id.to_string();
                    self.db(move |_| crate::commands::orchestration::release_pin(&id))
                        .await?;
                }
                if let Some(cursor) = value["cursor"].as_str() {
                    run.scope.session_cursor = Some(cursor.into());
                }
                if let Some(fingerprint) = value.get("harnessFingerprint").and_then(Value::as_str) {
                    run.scope.harness_fingerprint = Some(fingerprint.into());
                }
                if adapters::terminal_failure(&value) {
                    receipt.state = "failed".into();
                    receipt.error = Some(format!("Workspace turn {}", value["state"]));
                    receipt.finished_at = Some(now());
                    continue;
                }
                receipt.output = Some(adapters::output_preview(&value));
                receipt.state = "completed".into();
                receipt.error = None;
                receipt.finished_at = Some(now());
                run.progress.outputs[&receipt.step_id] = value;
            }
        }
        Ok(())
    }
}

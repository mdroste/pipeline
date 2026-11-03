//! Existing durable-result search and reconciliation order.
use super::adapters::atomic_json;
use super::store::{err, now, Result, TaskRun};
use super::{adapters, Coordinator};
use serde_json::Value;

impl Coordinator {
    /// An automation's conversation is rarely open, and role conversations are
    /// hidden, so no window asks the runtime to settle a turn that outlived its
    /// receipt. An unreachable runtime leaves the turn open, and the action takes
    /// the same path as any turn with no live owner.
    pub(crate) async fn settle_turns(&self, run: &TaskRun) {
        if !run.progress.receipts.values().any(|r| r.state == "unknown") {
            return;
        }
        if let Some(session) = run.scope.session_id.clone() {
            adapters::settle_turn(self.app.clone(), session).await;
        }
    }

    /// Before Retry replaces an attempt, takes up the turn it left in the
    /// conversation. Any other turn stays a change the researcher must review.
    pub(crate) async fn adopt_own_turn(&self, run: &mut TaskRun) -> Result<()> {
        let Some(session) = run.scope.session_id.clone() else {
            return Ok(());
        };
        let operations: Vec<_> = run
            .progress
            .receipts
            .values()
            .filter(|r| matches!(r.state.as_str(), "unknown" | "failed"))
            .map(|r| r.operation.clone())
            .collect();
        if operations.is_empty() {
            return Ok(());
        }
        let newest = || {
            let (session, operations) = (session.clone(), operations.clone());
            crate::workbench::commands::run_store(move |s| {
                crate::workbench::tasks::own_newest_turn(&s, &session, &operations)
            })
        };
        let mut turn = newest().await.map_err(|e| e.message)?;
        if turn.as_ref().is_some_and(|(_, ended)| !ended) {
            self.settle_turns(run).await;
            turn = newest().await.map_err(|e| e.message)?;
        }
        if let Some((cursor, true)) = turn {
            run.scope.session_cursor = Some(cursor);
        }
        Ok(())
    }

    /// Settles and reads a managed child's interrupted action without replaying
    /// it. A failure found here is saved before the owner classifies it.
    pub(crate) async fn recover_child(&self, child: &TaskRun) -> Result<TaskRun> {
        self.settle_turns(child).await;
        let mut recovered = child.clone();
        self.recover_results(&mut recovered).await?;
        if recovered.progress.receipts.iter().any(|(key, receipt)| {
            receipt.state == "failed"
                && child
                    .progress
                    .receipts
                    .get(key)
                    .is_some_and(|old| old.state == "unknown")
        }) {
            self.save(
                &mut recovered,
                "reconciledFailure",
                "Recovered a terminal action failure without replaying it",
            )
            .await?;
        }
        Ok(recovered)
    }

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
                    let expected = adapters::task_binding(&run.scope).ok();
                    result = crate::workbench::commands::run_store(move |s| {
                        use crate::workbench::tasks;
                        match (tasks::delivery_outcome(&s, &session, &operation)?, expected) {
                            (Some(value), Some(expected)) => {
                                tasks::with_delivery_context(&s, value, &expected, &operation)
                                    .map(Some)
                            }
                            (value, _) => Ok(value),
                        }
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
                // Keep the time recorded when the app stopped: the hours until
                // this check are not action time.
                receipt.finished_at = receipt.finished_at.or(Some(now()));
                receipt.output = Some(adapters::output_preview(&value));
                if adapters::terminal_failure(&value) {
                    receipt.state = "failed".into();
                    receipt.error = Some(adapters::failure_message(&value));
                    continue;
                }
                receipt.state = "completed".into();
                receipt.error = None;
                run.progress.outputs[&receipt.step_id] = value;
            }
        }
        Ok(())
    }
}

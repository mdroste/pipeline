//! Existing durable-result search and reconciliation order.
use super::adapters::atomic_json;
use super::store::{now, Result, TaskRun};
use super::{adapters, Coordinator};
use serde_json::Value;

impl Coordinator {
    /// An automation's conversation is rarely open, and role conversations are
    /// hidden, so no window asks the runtime to settle a turn that outlived its
    /// receipt. An unreachable runtime leaves the turn open, and the action takes
    /// the same path as any turn with no live owner.
    pub(crate) async fn settle_turns(&self, run: &TaskRun) {
        let Some(session) = run.scope.session_id.clone() else {
            return;
        };
        let mut steps = Vec::new();
        adapters::flatten(&run.chain.steps, &mut steps);
        let operations = run
            .progress
            .receipts
            .values()
            .filter(|r| {
                r.state == "unknown"
                    && steps.iter().any(|s| {
                        s.id == r.step_id
                            && matches!(s.action, super::definition::Action::Workspace { .. })
                    })
            })
            .map(|r| r.operation.clone())
            .collect::<Vec<_>>();
        if operations.is_empty() {
            return;
        }
        let key = session.clone();
        let own = crate::workbench::commands::run_store(move |s| {
            crate::workbench::tasks::own_newest_turn(&s, &key, &operations)
        })
        .await;
        if matches!(own, Ok(Some((_, false)))) {
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

    pub(crate) async fn recover_results(&self, run: &mut TaskRun) -> Result<bool> {
        let mut running = false;
        let mut steps = Vec::new();
        adapters::flatten(&run.chain.steps, &mut steps);
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
            use adapters::contract::{lookup, Outcome};
            let action = &steps
                .iter()
                .find(|step| step.id == receipt.step_id)
                .ok_or("Recovery receipt has no matching step")?
                .action;
            let result = lookup(&self.store, &run.scope, action, receipt).await?;
            match result {
                Outcome::Running => {
                    running = true;
                    receipt.error = Some("The original action is still running. Check saved results when it finishes.".into());
                    continue;
                }
                Outcome::NotSubmitted => {
                    receipt.state = "failed".into();
                    receipt.error =
                        Some("This attempt stopped before dispatch; no work was submitted.".into());
                    continue;
                }
                Outcome::Unknown => continue,
                Outcome::Completed(value) | Outcome::Failed(value) => {
                    atomic_json(&path, &value)?;
                    if let Some(cursor) = value["cursor"].as_str() {
                        run.scope.session_cursor = Some(cursor.into());
                    }
                    if let Some(fingerprint) =
                        value.get("harnessFingerprint").and_then(Value::as_str)
                    {
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
        }
        Ok(running)
    }
}

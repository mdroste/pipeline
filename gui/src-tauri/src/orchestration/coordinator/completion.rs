//! Saved transitions and completion adoption; distinct from restart recovery.
use super::store::{now, Result, TaskRun};
use super::{adapters, discovery, missions, state, Coordinator};
use serde_json::Value;

impl Coordinator {
    pub(crate) async fn save(&self, run: &mut TaskRun, kind: &str, detail: &str) -> Result<()> {
        if matches!(run.state.as_str(), "running" | "waiting" | "queued") {
            run.due_at = run.progress.next_wake(run.due_at);
        }
        let mut copy = run.clone();
        let stored_kind = kind.to_string();
        let detail = detail.to_string();
        let (stored, mission_owned) = self
            .db(move |s| {
                if state::context_within_budget(&copy.progress) {
                    s.save(&mut copy, &stored_kind, &detail)?;
                } else {
                    // Full action results already live in their immutable journals.
                    // Preserve the last saved state and require reconciliation. A
                    // stop stands: no action is left to finish it, so it ends here.
                    copy.progress = s.get(&copy.id)?.progress;
                    let reason = "Task context budget reached; inspect durable action results";
                    if copy.state == "cancelling" {
                        copy.state = "cancelled".into();
                    } else if copy.state != "cancelled" {
                        copy.state = "attention".into();
                        copy.reason = Some(reason.into());
                    }
                    copy.due_at = None;
                    for receipt in copy
                        .progress
                        .receipts
                        .values_mut()
                        .filter(|r| r.state == "running")
                    {
                        receipt.state = "unknown".into();
                        receipt.error = Some(reason.into());
                    }
                    // The saved state may be over the budget too, and this adds to
                    // neither.
                    s.write(&mut copy, &stored_kind, &detail)?;
                }
                if matches!(
                    stored_kind.as_str(),
                    "stepCompleted" | "reconciled" | "reconcile" | "missionReconcile"
                ) {
                    for receipt in copy.progress.receipts.values().filter(|r| {
                        r.state == "completed"
                            && r.output.as_ref().is_some_and(|v| v.get("runId").is_some())
                    }) {
                        if let Err(e) =
                            adapters::journaled_review(&s, &receipt.operation).and_then(|id| {
                                match id {
                                    Some(id) => {
                                        crate::commands::orchestration::release_abandoned_pin(
                                            &id,
                                            &receipt.operation,
                                        )
                                    }
                                    None => Ok(()),
                                }
                            })
                        {
                            eprintln!("Completed Review pin needs reconciliation: {e}");
                        }
                    }
                }
                if copy.state == "cancelled" {
                    if let Err(error) = adapters::reconcile_abandoned_reviews(&s) {
                        eprintln!("Abandoned Review retention needs attention: {error}");
                    }
                }
                let owned = missions::storage::wake_owner(&s, &copy.id)?
                    || discovery::storage::wake_owner(&s, &copy.id)?;
                Ok((copy, owned))
            })
            .await?;
        *run = stored;
        if run.state == "attention" {
            self.stop_children(&run.id);
        }
        self.notify();
        let needs_input =
            kind == "stepStarted" && run.progress.receipts.values().any(|r| r.state == "input");
        if !mission_owned
            && (matches!(kind, "finished" | "attention" | "deadline" | "budget")
                || run.state == "attention"
                || needs_input)
        {
            // The shell applies notification preferences to this notice,
            // including when the main window is hidden in background mode.
            self.announce("tasks:notice", run.notice());
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) async fn complete(
        &self,
        task: &str,
        address: &str,
        result: Result<Value>,
    ) -> Result<()> {
        self.complete_attempt(task, address, None, result).await
    }

    pub(super) async fn publish_completion(
        &self,
        task: &str,
        address: &str,
        operation: &str,
        result: Result<Value>,
    ) {
        // Release the execution slot once the owner ends. The durable running
        // receipt prevents repeat dispatch while publication is retried.
        let mut delay = 1;
        loop {
            match self
                .complete_attempt(task, address, Some(operation), result.clone())
                .await
            {
                Ok(()) => break,
                Err(error) => {
                    if let Some(app) = &self.app {
                        use tauri::Emitter;
                        let _ = app.emit("tasks:error", error);
                    }
                    let (id, op) = (task.to_owned(), operation.to_owned());
                    if matches!(
                        self.db(move |s| s.completion_pending(&id, &op)).await,
                        Ok(false)
                    ) {
                        break;
                    }
                    if self.stopping.load(std::sync::atomic::Ordering::Acquire) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    delay = (delay * 2).min(30);
                }
            }
        }
    }

    async fn complete_attempt(
        &self,
        task: &str,
        address: &str,
        operation: Option<&str>,
        mut result: Result<Value>,
    ) -> Result<()> {
        let _gate = self.gate.lock().await;
        let task = task.to_string();
        let mut run = self.db(move |s| s.get(&task)).await?;
        let Some(previous) = run.progress.receipts.get(address) else {
            return Ok(());
        };
        if previous.state != "running" || operation.is_some_and(|op| op != previous.operation) {
            return Ok(());
        }
        let mut not_submitted = false;
        if result.as_ref().is_err_and(|e| {
            !e.starts_with("task_busy:") && e != super::scheduler::TIME_LIMIT_REACHED
        }) {
            use adapters::contract::{lookup, Outcome};
            let mut steps = Vec::new();
            adapters::flatten(&run.chain.steps, &mut steps);
            if let Some(step) = steps.iter().find(|s| s.id == previous.step_id) {
                match lookup(&self.store, &run.scope, &step.action, previous).await {
                    Ok(Outcome::Completed(v) | Outcome::Failed(v)) => {
                        adapters::atomic_json(
                            &self
                                .store
                                .root
                                .join("actions")
                                .join(&previous.operation)
                                .join("result.json"),
                            &v,
                        )?;
                        result = Ok(v);
                    }
                    Ok(Outcome::NotSubmitted) => not_submitted = true,
                    Err(error) => {
                        result = Err(format!(
                            "Could not read the original action receipt: {error}"
                        ))
                    }
                    _ => {}
                }
            }
        }
        let receipt = run
            .progress
            .receipts
            .get_mut(address)
            .ok_or("Action receipt disappeared")?;
        if receipt.state != "running" {
            return Ok(());
        }
        receipt.finished_at = Some(now());
        match result {
            Ok(value) if adapters::terminal_failure(&value) => {
                let message = adapters::failure_message(&value);
                receipt.state = "failed".into();
                receipt.output = Some(adapters::output_preview(&value));
                receipt.error = Some(message.clone());
                // This run's own turn moved the conversation past the recorded
                // cursor, so a Retry starts from where that turn left it.
                if let Some(cursor) = value.get("cursor").and_then(Value::as_str) {
                    run.scope.session_cursor = Some(cursor.into());
                }
                if !["cancelled", "cancelling"].contains(&run.state.as_str()) {
                    run.state = "attention".into();
                    run.reason = Some(message.clone());
                }
                self.save(&mut run, "attention", &message).await?;
            }
            Ok(value) => {
                receipt.state = "completed".into();
                receipt.output = Some(adapters::output_preview(&value));
                if let Some(cursor) = value.get("cursor").and_then(Value::as_str) {
                    run.scope.session_cursor = Some(cursor.into());
                }
                if let Some(fingerprint) = value.get("harnessFingerprint").and_then(Value::as_str) {
                    run.scope.harness_fingerprint = Some(fingerprint.into());
                }
                run.progress.outputs[&receipt.step_id] = value.clone();
                if !["paused", "cancelled", "cancelling", "attention"].contains(&run.state.as_str())
                {
                    run.state = "queued".into();
                    run.reason = None;
                    run.due_at = Some(now());
                }
                self.save(&mut run, "stepCompleted", "Action completed")
                    .await?;
            }
            Err(message) if message.starts_with("task_busy:") => {
                run.progress.receipts.remove(address);
                run.progress.actions = run.progress.actions.saturating_sub(1);
                if !["paused", "cancelled", "cancelling", "attention"].contains(&run.state.as_str())
                {
                    run.state = "queued".into();
                    run.due_at = Some(now() + 2);
                    run.reason = Some("Waiting for an execution slot".into());
                }
                self.save(&mut run, "queued", "Waiting for an execution slot")
                    .await?;
            }
            Err(message) => {
                receipt.state = if not_submitted { "failed" } else { "unknown" }.into();
                receipt.error = Some(message.clone());
                if run.state != "cancelled" && run.state != "cancelling" {
                    run.state = "attention".into();
                    run.reason = Some(message.clone());
                }
                self.save(&mut run, "attention", &message).await?;
            }
        }
        if run.state == "cancelling"
            && !run.progress.receipts.values().any(|r| r.state == "running")
        {
            run.state = "cancelled".into();
            self.save(&mut run, "cancelled", "Task stopped").await?;
        }
        Ok(())
    }
}

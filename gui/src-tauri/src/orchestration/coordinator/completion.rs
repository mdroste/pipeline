//! Saved transitions and completion adoption; distinct from restart recovery.
use super::store::{now, Result, TaskRun};
use super::{adapters, discovery, missions, state, Coordinator};
use serde_json::{json, Value};
use tauri::Emitter;

impl Coordinator {
    pub(crate) async fn save(&self, run: &mut TaskRun, kind: &str, detail: &str) -> Result<()> {
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
            if let Some(app) = &self.app {
                let _ = app.emit(
                    "tasks:notice",
                    json!({"id":run.id,"name":run.name,"state":run.state,"reason":run.reason}),
                );
                // The shell applies notification preferences to this notice,
                // including when the main window is hidden in background mode.
            }
        }
        Ok(())
    }

    pub(super) async fn complete(
        &self,
        task: &str,
        address: &str,
        result: Result<Value>,
    ) -> Result<()> {
        let _gate = self.gate.lock().await;
        let task = task.to_string();
        let mut run = self.db(move |s| s.get(&task)).await?;
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
                if adapters::oversized_reply(&value) {
                    // That turn did finish, so the conversation moved past it.
                    if let Some(cursor) = value.get("cursor").and_then(Value::as_str) {
                        run.scope.session_cursor = Some(cursor.into());
                    }
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
                let run_id = value
                    .get("runId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if !["paused", "cancelled", "cancelling", "attention"].contains(&run.state.as_str())
                {
                    run.state = "queued".into();
                    run.reason = None;
                    run.due_at = Some(now());
                }
                self.save(&mut run, "stepCompleted", "Action completed")
                    .await?;
                if let Some(id) = run_id {
                    self.db(move |_| crate::commands::orchestration::release_pin(&id))
                        .await?;
                }
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
                receipt.state = "unknown".into();
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

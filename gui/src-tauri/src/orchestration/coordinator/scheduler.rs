//! Ordered scheduling, admission, durable receipts and worker dispatch.
use super::adapters::execute;
use super::definition::Action;
use super::state::{Advance, Receipt};
use super::store::{err, now, Result};
use super::{adapters, discovery, missions, state, store, Coordinator};
use serde_json::json;
use std::sync::{atomic::Ordering, Arc};
use tauri::Emitter;
use tokio::sync::watch;

impl Coordinator {
    pub(super) async fn run(self: Arc<Self>) {
        loop {
            if self.stopping.load(Ordering::Acquire) {
                break;
            }
            if let Err(e) = self.tick().await {
                if let Some(app) = &self.app {
                    let _ = app.emit("tasks:error", e);
                }
            }
            let wait = self
                .db(|s| s.next_due())
                .await
                .ok()
                .flatten()
                .map(|t| t.saturating_sub(now()).clamp(1, 60) as u64)
                .unwrap_or(60);
            tokio::select! {_=self.wake.notified()=>{},_=tokio::time::sleep(std::time::Duration::from_secs(wait))=>{}}
        }
    }

    pub(super) async fn tick(self: &Arc<Self>) -> Result<()> {
        let _gate = self.gate.lock().await;
        missions::tick(self).await?;
        discovery::tick(self).await?;
        self.db(|s| s.fire_schedules(now())).await?;
        let ids = self.db(|s| s.ready_ids(now())).await?;
        for id in ids {
            let mut run = self.db(move |s| s.get(&id)).await?;
            if run.schedule_id.is_some()
                && !run.progress.decisions.contains_key("occurrenceSession")
                && run.scope.session_id.is_some()
            {
                let expected = adapters::task_binding(&run.scope)?;
                let operation = run.id.clone();
                let title = run.name.clone();
                match crate::workbench::commands::run_store(move |s| {
                    crate::workbench::tasks::occurrence_session(&s, &expected, &operation, &title)
                })
                .await
                {
                    Ok(binding) => {
                        run.scope.root_identity = Some(binding.root_identity);
                        run.scope.session_id = Some(binding.session_id);
                        run.scope.session_cursor = Some(binding.cursor);
                        run.scope.runtime_root = Some(binding.runtime_root);
                        run.scope.harness_fingerprint = Some(binding.harness_fingerprint);
                        run.progress
                            .decisions
                            .insert("occurrenceSession".into(), json!(true));
                        self.save(
                            &mut run,
                            "conversation",
                            "Created this occurrence's conversation",
                        )
                        .await?;
                    }
                    Err(e) => {
                        run.state = "attention".into();
                        run.reason = Some(e.message.clone());
                        self.save(&mut run, "attention", &e.message).await?;
                        continue;
                    }
                }
            }
            if run.deadline_at <= now() {
                run.state = "attention".into();
                run.reason = Some("Task deadline reached".into());
                self.save(&mut run, "deadline", "Task deadline reached")
                    .await?;
                self.stop_children(&run.id);
                continue;
            }
            let previous_state = run.state.clone();
            let previous_due = run.due_at;
            let previous_decisions = run.progress.decisions.len();
            let mut changed = false;
            for receipt in run.progress.receipts.values_mut() {
                if ["timer", "input"].contains(&receipt.state.as_str())
                    && receipt.wake_at.is_some_and(|t| t <= now())
                {
                    let output = json!({"timedOut":receipt.state=="input","at":now()});
                    receipt.state = "completed".into();
                    receipt.finished_at = Some(now());
                    receipt.output = Some(output.clone());
                    run.progress.outputs[&receipt.step_id] = output;
                    changed = true;
                }
            }
            let outcome = state::advance(&run.chain, &mut run.progress, &run.inputs);
            match outcome {
                Advance::Finished => {
                    run.state = "finished".into();
                    run.reason = Some(
                        if run.progress.limit_reached {
                            "Iteration limit reached; inspect remaining issues"
                        } else {
                            "All steps completed"
                        }
                        .into(),
                    );
                    run.due_at = None;
                    self.save(&mut run, "finished", "Task finished").await?;
                }
                Advance::Attention(message) => {
                    run.state = "attention".into();
                    run.reason = Some(message.clone());
                    run.due_at = None;
                    self.save(&mut run, "attention", &message).await?;
                }
                Advance::Waiting => {
                    let running = run.progress.receipts.values().any(|r| r.state == "running");
                    run.state = if running { "running" } else { "waiting" }.into();
                    run.due_at = run
                        .progress
                        .receipts
                        .values()
                        .filter_map(|r| {
                            if matches!(r.state.as_str(), "timer" | "input") {
                                r.wake_at
                            } else {
                                None
                            }
                        })
                        .min();
                    if !running {
                        run.reason = Some(
                            run.progress
                                .receipts
                                .values()
                                .find(|r| r.state == "input" || r.state == "timer")
                                .map(|r| r.label.clone())
                                .unwrap_or_else(|| "Waiting for a child action".into()),
                        );
                    }
                    if changed
                        || previous_state != run.state
                        || previous_due != run.due_at
                        || previous_decisions != run.progress.decisions.len()
                    {
                        self.save(&mut run, "waiting", "Waiting for the next step")
                            .await?;
                    }
                }
                Advance::Ready(ready) => {
                    let was_queued = run.state == "queued"
                        && run.reason.as_deref() == Some("Waiting for an execution slot");
                    let mut queued_due = None;
                    let mut blocked = false;
                    for pending in ready {
                        let control = matches!(
                            pending.step.action,
                            Action::Delay { .. } | Action::Until { .. } | Action::Input { .. }
                        );
                        let kind = match pending.step.action {
                            Action::Workspace { .. } => "workspace",
                            Action::Review { .. } => "review",
                            _ => "other",
                        };
                        let busy = {
                            let active = self.active.lock().unwrap_or_else(|e| e.into_inner());
                            active.len() >= 4
                                || (kind != "other" && active.values().any(|(k, _)| k == kind))
                        };
                        if !control
                            && (busy || !adapters::available(&pending.step.action, &run.scope))
                        {
                            queued_due = Some(now() + 2);
                            run.due_at = queued_due;
                            if run.state != "running" {
                                run.state = "queued".into();
                            }
                            run.reason = Some("Waiting for an execution slot".into());
                            blocked = true;
                            continue;
                        }
                        if run.progress.actions >= run.chain.limits.max_actions
                            || serde_json::to_vec(&run.progress).map_err(err)?.len()
                                > 8 * 1024 * 1024
                        {
                            run.state = "attention".into();
                            run.reason = Some(
                                "Task action or retained context budget reached (8 MiB)".into(),
                            );
                            self.save(&mut run, "budget", "Action budget reached")
                                .await?;
                            break;
                        }
                        let operation = store::id();
                        let timestamp = now();
                        let (status, wake) = match pending.step.action {
                            Action::Delay { seconds } => {
                                ("timer", Some(timestamp + seconds as i64))
                            }
                            Action::Until { at } => ("timer", Some(at)),
                            Action::Input {
                                timeout_seconds, ..
                            } => ("input", timeout_seconds.map(|v| timestamp + v as i64)),
                            _ => ("running", None),
                        };
                        let label = match &pending.step.action {
                            Action::Input { prompt, .. } => prompt.clone(),
                            _ => pending.step.label.clone(),
                        };
                        let receipt = Receipt {
                            sequence: run.progress.actions + 1,
                            address: pending.address.clone(),
                            step_id: pending.step.id.clone(),
                            label: label.clone(),
                            state: status.into(),
                            operation: operation.clone(),
                            started_at: timestamp,
                            finished_at: None,
                            wake_at: wake,
                            output: None,
                            error: None,
                            child: None,
                        };
                        run.progress
                            .receipts
                            .insert(pending.address.clone(), receipt);
                        run.progress.actions += 1;
                        run.state = if run.progress.receipts.values().any(|r| r.state == "running")
                        {
                            "running"
                        } else if queued_due.is_some() {
                            "queued"
                        } else {
                            "waiting"
                        }
                        .into();
                        run.reason = Some(label.clone());
                        run.due_at = run
                            .progress
                            .receipts
                            .values()
                            .filter(|r| matches!(r.state.as_str(), "input" | "timer"))
                            .filter_map(|r| r.wake_at)
                            .chain(queued_due)
                            .min();
                        self.save(&mut run, "stepStarted", &label).await?;
                        changed = false;
                        if run.state == "attention" {
                            break;
                        }
                        if !control {
                            let (stop, receiver) = watch::channel(false);
                            let key = format!("{}/{}", run.id, pending.address);
                            self.active
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .insert(key.clone(), (kind.into(), stop));
                            let coordinator = self.clone();
                            let owned = run.clone();
                            let store = self.store.clone();
                            let app = self.app.clone();
                            tokio::spawn(async move {
                                let timeout = owned.chain.limits.action_timeout_secs;
                                let (deadline_stop, deadline_receiver) = watch::channel(false);
                                let mut parent_stop = receiver;
                                let cancellation = tokio::spawn(async move {
                                    tokio::select! {_=parent_stop.changed()=>{},_=tokio::time::sleep(std::time::Duration::from_secs(timeout.into()))=>{}}
                                    let _ = deadline_stop.send(true);
                                });
                                let result = tokio::spawn(execute(
                                    app,
                                    store,
                                    owned.clone(),
                                    pending.clone(),
                                    operation.clone(),
                                    deadline_receiver,
                                ))
                                .await
                                .unwrap_or_else(|e| {
                                    Err(format!("Action worker stopped unexpectedly: {e}"))
                                });
                                cancellation.abort();
                                if let Err(error) = coordinator
                                    .complete(&owned.id, &pending.address, result)
                                    .await
                                {
                                    if let Some(app) = &coordinator.app {
                                        let _ = app.emit("tasks:error", error);
                                    }
                                }
                                coordinator
                                    .active
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .remove(&key);
                                coordinator.notify();
                            });
                        }
                    }
                    if changed || (blocked && !was_queued) {
                        self.save(&mut run, "queued", "Waiting for an execution slot")
                            .await?;
                    } else if blocked {
                        // Already recorded as waiting: keep the durable due time
                        // fresh for restart recovery without rewriting the body
                        // or appending an identical event every poll.
                        let id = run.id.clone();
                        self.db(move |s| s.touch_due(&id, queued_due)).await?;
                    }
                }
            }
        }
        Ok(())
    }
}

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

pub(super) const TIME_LIMIT_REACHED: &str = "This step reached its time limit";

/// Runs one action until it ends, its parent stops it, or its time runs out.
/// The action stops the same way in the last two cases, so only this envelope
/// can say which one happened.
pub(super) async fn time_limited<F>(
    timeout: std::time::Duration,
    mut parent_stop: watch::Receiver<bool>,
    action: impl FnOnce(watch::Receiver<bool>) -> F,
) -> Result<serde_json::Value>
where
    F: std::future::Future<Output = Result<serde_json::Value>>,
{
    if timeout.is_zero() {
        return Err(TIME_LIMIT_REACHED.into());
    }
    let (stop, receiver) = watch::channel(*parent_stop.borrow());
    let end = tokio::time::Instant::now() + timeout;
    let worker = action(receiver);
    tokio::pin!(worker);
    let expired = tokio::select! {
        biased;
        result=&mut worker => {
            return if tokio::time::Instant::now()>=end && !result.as_ref().is_err_and(|e|e.starts_with("task_busy:")) { Err(TIME_LIMIT_REACHED.into()) } else { result };
        }
        _ = tokio::time::sleep_until(end) => true,
        _ = parent_stop.changed() => false,
    };
    let _ = stop.send(true);
    // Owners keep their slot until shutdown/commit is acknowledged. A late
    // result remains in its journal but cannot start the next step on its own.
    let result = worker.await;
    if expired && !result.as_ref().is_err_and(|e| e.starts_with("task_busy:")) {
        Err(TIME_LIMIT_REACHED.into())
    } else {
        result
    }
}

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
        let acting = !self
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty();
        if acting {
            // How long an action ran, if the app stops before it reports back.
            let time = now();
            let _ = self
                .db(move |s| s.set_preference(store::HEARTBEAT, &json!(time)))
                .await;
        }
        // Each automation fails alone. The first error is still reported, after
        // everything else has had its turn.
        let mut outcome = missions::tick(self).await;
        outcome = outcome.and(discovery::tick(self).await);
        outcome = outcome.and(self.db(|s| s.fire_schedules(now())).await);
        match self.db(|s| s.ready_ids(now())).await {
            Ok(ids) => {
                for id in ids {
                    if let Err(error) = self.tick_run(&id).await {
                        let key = id.clone();
                        let retry = Some(now() + store::RECORD_RETRY_SECS);
                        let _ = self.db(move |s| s.touch_due(&key, retry)).await;
                        outcome = outcome.and(Err(error));
                    }
                }
            }
            Err(error) => outcome = outcome.and(Err(error)),
        }
        self.retain().await;
        outcome
    }

    async fn tick_run(self: &Arc<Self>, id: &str) -> Result<()> {
        let id = id.to_string();
        let mut run = self.db(move |s| s.get(&id)).await?;
        if run.schedule_id.is_some()
            && run.progress.occurrence_session.is_none()
            && run.scope.session_id.is_some()
        {
            let expected = adapters::task_binding(&run.scope)?;
            let operation = run.id.clone();
            let title = run.name.clone();
            match crate::workbench::commands::run_store(move |s| {
                if crate::workbench::tasks::owns_occurrence(&s, &operation, &expected.session_id)? {
                    return Ok(expected);
                }
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
                    run.scope.connection_id = binding.connection_id;
                    // An older scheduler used this conditional's address. Preserve
                    // committed branch work, but retire a marker with no child work.
                    if !run
                        .progress
                        .receipts
                        .keys()
                        .chain(run.progress.decisions.keys())
                        .any(|address| address.starts_with("occurrenceSession/"))
                    {
                        run.progress.decisions.remove("occurrenceSession");
                    }
                    run.progress
                        .occurrence_session
                        .clone_from(&run.scope.session_id);
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
                    return Ok(());
                }
            }
        }
        if let Some(group) = run.progress.expired_group(now()) {
            let reason = format!(
                "Group {group} reached its elapsed deadline. Start a new run to give it more time."
            );
            run.state = "attention".into();
            run.due_at = None;
            run.reason = Some(reason.clone());
            self.save(&mut run, "deadline", &reason).await?;
            return Ok(());
        }
        if run.deadline_at <= now() {
            run.state = "attention".into();
            run.reason = Some("Task deadline reached".into());
            self.save(&mut run, "deadline", "Task deadline reached")
                .await?;
            self.stop_children(&run.id);
            return Ok(());
        }
        let previous_state = run.state.clone();
        let previous_due = run.due_at;
        let previous_groups = run.progress.group_deadlines.clone();
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
        changed |= previous_groups != run.progress.group_deadlines;
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
                run.due_at = run.progress.next_wake(run.due_at);
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
                // A run with a live sibling stays "running", so the reason
                // alone says the wait is already on record.
                let was_queued = run.reason.as_deref() == Some("Waiting for an execution slot");
                let mut queued_due = None;
                let mut blocked = false;
                let retries = if ready.iter().any(|a| !a.groups.is_empty())
                    && run.progress.actions as usize > run.progress.receipts.len()
                {
                    let id = run.id.clone();
                    self.db(move |s| s.retried_addresses(&id)).await?
                } else {
                    Vec::new()
                };
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
                    if !control && (busy || !adapters::available(&pending.step.action, &run.scope))
                    {
                        queued_due = run.progress.next_wake(Some(now() + 2));
                        run.due_at = queued_due;
                        if run.state != "running" {
                            run.state = "queued".into();
                        }
                        run.reason = Some("Waiting for an execution slot".into());
                        blocked = true;
                        continue;
                    }
                    if let Some((group, limit)) = pending.groups.iter().find(|(prefix, limit)| {
                        run.progress
                            .receipts
                            .keys()
                            .chain(retries.iter())
                            .filter(|addr| addr.starts_with(prefix))
                            .count()
                            >= *limit as usize
                    }) {
                        let reason = format!(
                            "Group {} reached its limit of {limit} actions",
                            group.trim_end_matches('/')
                        );
                        run.state = "attention".into();
                        run.reason = Some(reason.clone());
                        run.due_at = None;
                        self.save(&mut run, "budget", &reason).await?;
                        break;
                    }
                    if run.progress.actions >= run.chain.limits.max_actions
                        || serde_json::to_vec(&run.progress).map_err(err)?.len() > 8 * 1024 * 1024
                    {
                        run.state = "attention".into();
                        run.reason =
                            Some("Task action or retained context budget reached (8 MiB)".into());
                        self.save(&mut run, "budget", "Action budget reached")
                            .await?;
                        break;
                    }
                    let operation = store::id();
                    let timestamp = now();
                    let (status, wake) = match pending.step.action {
                        Action::Delay { seconds } => ("timer", Some(timestamp + seconds as i64)),
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
                    let cutoff = pending.cutoff(&run.progress, run.deadline_at, timestamp);
                    if cutoff <= timestamp {
                        run.due_at = Some(timestamp);
                        self.save(&mut run, "deadline", "Checking elapsed deadline")
                            .await?;
                        break;
                    }
                    let receipt = Receipt {
                        action_kind: Some(adapters::contract::Kind::of(&pending.step.action)),
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
                    run.state = if run.progress.receipts.values().any(|r| r.state == "running") {
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
                            let timeout = std::time::Duration::from_secs(
                                cutoff.saturating_sub(now()).max(0) as u64,
                            );
                            let worker = {
                                let dispatch_id = operation.clone();
                                let (owned, pending) = (owned.clone(), pending.clone());
                                move |stop| async move {
                                    tokio::spawn(execute(
                                        app,
                                        store,
                                        owned,
                                        pending,
                                        dispatch_id,
                                        stop,
                                    ))
                                    .await
                                    .unwrap_or_else(|e| {
                                        Err(format!("Action worker stopped unexpectedly: {e}"))
                                    })
                                }
                            };
                            let result = time_limited(timeout, receiver, worker).await;
                            coordinator
                                .active
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .remove(&key);
                            coordinator
                                .publish_completion(&owned.id, &pending.address, &operation, result)
                                .await;
                            coordinator.notify();
                        });
                    }
                }
                if run.state == "attention" {
                    return Ok(());
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
        Ok(())
    }
}

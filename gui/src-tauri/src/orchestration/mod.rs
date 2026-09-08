//! Durable, local task orchestration. Product runtimes retain their ownership.
pub(crate) mod adapters;
pub(crate) mod background;
pub mod commands;
pub mod definition;
pub mod missions;
pub mod state;
pub(crate) mod store;
pub mod triggers;

use adapters::{atomic_json, execute};
use definition::Action;
use serde_json::{json, Value};
use state::{Advance, Receipt};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};
use store::*;
use tauri::Emitter;
use tokio::sync::{watch, Notify};

static INSTANCE: OnceLock<Arc<Coordinator>> = OnceLock::new();
static INITIALIZE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static DB_WORKERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
pub struct Coordinator {
    pub(crate) store: Store,
    pub(crate) gate: tokio::sync::Mutex<()>,
    wake: Notify,
    active: Mutex<HashMap<String, (String, watch::Sender<bool>)>>,
    stopping: AtomicBool,
    pub(crate) background: AtomicBool,
    _owner: std::fs::File,
    app: Option<tauri::AppHandle>,
}
pub(crate) async fn manager(app: tauri::AppHandle) -> Result<Arc<Coordinator>> {
    if let Some(value) = INSTANCE.get() {
        return Ok(value.clone());
    }
    let _init = INITIALIZE.lock().await;
    if let Some(value) = INSTANCE.get() {
        return Ok(value.clone());
    }
    let (store, lock) = tokio::task::spawn_blocking(|| {
        use fs2::FileExt;
        let root = Store::default_root()?;
        std::fs::create_dir_all(&root).map_err(err)?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("owner.lock"))
            .map_err(err)?;
        lock.try_lock_exclusive().map_err(|_| {
            "Another Pipeline instance owns task execution. Open that instance to manage tasks."
                .to_string()
        })?;
        let store = Store::open(&root)?;
        store.recover()?;
        Ok::<_, String>((store, lock))
    })
    .await
    .map_err(err)??;
    let background = store
        .preference("background")?
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if background {
        background::install(&app)?;
    }
    let manager = Arc::new(Coordinator {
        store,
        gate: tokio::sync::Mutex::new(()),
        wake: Notify::new(),
        active: Mutex::new(HashMap::new()),
        stopping: AtomicBool::new(false),
        background: AtomicBool::new(background),
        _owner: lock,
        app: Some(app),
    });
    let _ = INSTANCE.set(manager.clone());
    let runner = manager.clone();
    tokio::spawn(async move {
        runner.run().await;
    });
    Ok(manager)
}
pub fn start_if_present(app: tauri::AppHandle) {
    if Store::default_root().is_ok_and(|r| r.join("tasks.sqlite3").exists()) {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = manager(app.clone()).await {
                let _ = app.emit("tasks:error", e);
            }
        });
    }
}
pub fn background_enabled() -> bool {
    INSTANCE
        .get()
        .is_some_and(|s| s.background.load(Ordering::Acquire))
}
pub async fn shutdown() {
    if let Some(m) = INSTANCE.get() {
        m.stopping.store(true, Ordering::Release);
        for (_, stop) in m.active.lock().unwrap_or_else(|e| e.into_inner()).values() {
            let _ = stop.send(true);
        }
        m.wake.notify_one();
        for _ in 0..100 {
            if m.active
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}
impl Coordinator {
    pub(crate) fn notify(&self) {
        self.wake.notify_one();
        if let Some(app) = &self.app {
            let _ = app.emit("tasks:changed", ());
        }
    }
    pub(crate) async fn db<T: Send + 'static>(
        &self,
        f: impl FnOnce(Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let _permit = DB_WORKERS.acquire().await.map_err(err)?;
        let s = self.store.clone();
        tokio::task::spawn_blocking(move || f(s))
            .await
            .map_err(err)?
    }
    async fn run(self: Arc<Self>) {
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
    async fn tick(self: &Arc<Self>) -> Result<()> {
        let _gate = self.gate.lock().await;
        missions::tick(self).await?;
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
                    let mut queued_due = None;
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
                            changed = true;
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
                    if changed {
                        self.save(&mut run, "queued", "Waiting for an execution slot")
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) async fn save(&self, run: &mut TaskRun, kind: &str, detail: &str) -> Result<()> {
        let mut copy = run.clone();
        let stored_kind = kind.to_string();
        let detail = detail.to_string();
        let (stored, mission_owned) = self
            .db(move |s| {
                s.save(&mut copy, &stored_kind, &detail)?;
                let owned = missions::storage::wake_owner(&s, &copy.id)?;
                Ok((copy, owned))
            })
            .await?;
        *run = stored;
        self.notify();
        let needs_input =
            kind == "stepStarted" && run.progress.receipts.values().any(|r| r.state == "input");
        if !mission_owned
            && (matches!(kind, "finished" | "attention" | "deadline" | "budget") || needs_input)
        {
            if let Some(app) = &self.app {
                let _ = app.emit(
                    "tasks:notice",
                    json!({"id":run.id,"name":run.name,"state":run.state,"reason":run.reason}),
                );
                if self.background.load(Ordering::Acquire) {
                    use tauri::Manager;
                    if let Some(window) = app.get_webview_window("main") {
                        if window.is_visible().is_ok_and(|v| !v) {
                            let _ = window.request_user_attention(Some(
                                tauri::UserAttentionType::Informational,
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
    async fn complete(&self, task: &str, address: &str, result: Result<Value>) -> Result<()> {
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
    pub(crate) fn stop_children(&self, id: &str) {
        let prefix = format!("{id}/");
        for (key, (_, sender)) in self.active.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            if key.starts_with(&prefix) {
                let _ = sender.send(true);
            }
        }
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
                        if value["state"] == "completed" {
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
#[cfg(test)]
mod tests;

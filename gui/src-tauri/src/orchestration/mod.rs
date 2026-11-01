//! Durable, local task orchestration. Product runtimes retain their ownership.
//!
//! `owner.lock` lets one Pipeline process run tasks; a second instance gets an
//! error instead of a shared store. The coordinator saves before it dispatches
//! and advances only after an action's response is journaled. An interrupted
//! action is marked uncertain and never replayed. Conversations and Reviews are
//! reached only through `adapters`, which pin profiles at preparation and resolve
//! Review credentials at dispatch. Leaf actions stay within the app-wide limits
//! of one Workspace turn and one Review run.
pub(crate) mod adapters;
pub(crate) mod background;
pub mod commands;
#[path = "coordinator/completion.rs"]
mod completion;
pub mod definition;
pub mod discovery;
mod durable;
pub mod missions;
#[path = "coordinator/recovery.rs"]
mod recovery;
#[path = "coordinator/retention.rs"]
mod retention;
#[path = "coordinator/scheduler.rs"]
mod scheduler;
pub mod state;
pub(crate) mod store;
pub mod triggers;

#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
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
        if let Err(error) = adapters::reconcile_abandoned_reviews(&store) {
            eprintln!("Abandoned Review retention needs attention: {error}");
        }
        if let Err(error) = adapters::reconcile_adopted_reviews(&store) {
            eprintln!("Completed Review retention needs attention: {error}");
        }
        if let Err(error) = crate::commands::orchestration::reconcile_failed_pins() {
            eprintln!("Review retention reconciliation needs attention: {error}");
        }
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
/// Task runs in progress, for warning before a restart.
pub fn running_count() -> usize {
    INSTANCE.get().map_or(0, |m| {
        m.active.lock().unwrap_or_else(|e| e.into_inner()).len()
    })
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
    pub(crate) fn stop_children(&self, id: &str) {
        let prefix = format!("{id}/");
        for (key, (_, sender)) in self.active.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            if key.starts_with(&prefix) {
                let _ = sender.send(true);
            }
        }
    }
}
#[cfg(test)]
pub(crate) mod tests;

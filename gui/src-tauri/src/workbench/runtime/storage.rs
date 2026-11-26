//! Bounded store workers and exclusive storage maintenance.

use super::turn_queue;
use crate::workbench::{
    research,
    store::{Store, WorkbenchError, WorkbenchResult},
};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};

pub(crate) const DATABASE_WORKER_LIMIT: usize = 4;
static DATABASE_WORKERS: Semaphore = Semaphore::const_new(DATABASE_WORKER_LIMIT);
pub(crate) static DATABASE_GATE: RwLock<()> = RwLock::const_new(());

#[path = "storage_phases.rs"]
mod phases;
pub(crate) use phases::run_store_phased;

pub(crate) async fn run_store<T, F>(operation: F) -> WorkbenchResult<T>
where
    T: Send + 'static,
    F: FnOnce(Store) -> WorkbenchResult<T> + Send + 'static,
{
    let gate = DATABASE_GATE.read().await;
    run_store_ungated(move |store| {
        let _gate = gate;
        operation(store)
    })
    .await
}

pub(crate) async fn run_store_owned<T, F>(store: Store, operation: F) -> WorkbenchResult<T>
where
    T: Send + 'static,
    F: FnOnce(Store) -> WorkbenchResult<T> + Send + 'static,
{
    let gate = DATABASE_GATE.read().await;
    let permit = DATABASE_WORKERS
        .acquire()
        .await
        .map_err(|_| WorkbenchError::worker("Database workers are unavailable", true))?;
    tokio::task::spawn_blocking(move || {
        let (_gate, _permit) = (gate, permit);
        operation(store)
    })
    .await
    .map_err(|e| WorkbenchError::worker(format!("Database worker ended: {e}"), true))?
}

async fn run_store_ungated<T, F>(operation: F) -> WorkbenchResult<T>
where
    T: Send + 'static,
    F: FnOnce(Store) -> WorkbenchResult<T> + Send + 'static,
{
    let permit = DATABASE_WORKERS
        .acquire()
        .await
        .map_err(|_| WorkbenchError::worker("Workbench database workers are unavailable", true))?;
    tokio::task::spawn_blocking(move || {
        // A cancelled await must not release admission while its worker runs.
        let _permit = permit;
        operation(Store::open_default()?)
    })
    .await
    .map_err(|error| {
        WorkbenchError::worker(format!("Workbench database worker stopped: {error}"), true)
    })?
}

pub(crate) async fn run_store_exclusive<T, F>(operation: F) -> WorkbenchResult<T>
where
    T: Send + 'static,
    F: FnOnce(Store) -> WorkbenchResult<T> + Send + 'static,
{
    let gate = DATABASE_GATE.write().await;
    run_store_ungated(move |store| {
        let _gate = gate;
        operation(store)
    })
    .await
}

pub(crate) fn storage_turn_permit(queue: Arc<Semaphore>) -> WorkbenchResult<OwnedSemaphorePermit> {
    queue.try_acquire_owned().map_err(|_| {
        WorkbenchError::conflict(
            "Stop or finish the Workspace turn before changing private storage",
        )
    })
}

pub(crate) async fn run_storage_maintenance<T, F>(operation: F) -> WorkbenchResult<T>
where
    T: Send + 'static,
    F: FnOnce(Store) -> WorkbenchResult<T> + Send + 'static,
{
    // The native permit also covers setup before a turn has a persisted identity.
    // The exclusive database gate prevents local jobs/captures starting mid-move.
    let _turn = storage_turn_permit(turn_queue().clone())?;
    run_store_exclusive(move |store| {
        if research::jobs::has_active() {
            return Err(WorkbenchError::conflict(
                "Stop or finish local jobs before changing private storage",
            ));
        }
        operation(store)
    })
    .await
}

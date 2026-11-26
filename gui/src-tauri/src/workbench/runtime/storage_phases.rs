//! Bounded file work between short database phases. The storage gate and file
//! admission travel with the actual workers, including after caller cancellation.
use super::{DATABASE_GATE, DATABASE_WORKERS};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use tokio::sync::{RwLock, RwLockReadGuard, Semaphore, SemaphorePermit};

static FILE_WORKERS: Semaphore = Semaphore::const_new(2);

struct Resources {
    database: &'static Semaphore,
    files: &'static Semaphore,
    gate: &'static RwLock<()>,
}

struct Protected<T> {
    store: Store,
    gate: RwLockReadGuard<'static, ()>,
    file: SemaphorePermit<'static>,
    value: T,
}

impl<T> Protected<T> {
    fn map<U>(self, work: impl FnOnce(T) -> WorkbenchResult<U>) -> WorkbenchResult<Protected<U>> {
        let Self {
            store,
            gate,
            file,
            value,
        } = self;
        let value = work(value)?;
        Ok(Protected {
            store,
            gate,
            file,
            value,
        })
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> WorkbenchResult<T> + Send + 'static,
) -> WorkbenchResult<T> {
    tokio::task::spawn_blocking(work).await.map_err(|error| {
        WorkbenchError::worker(format!("Project file worker stopped: {error}"), true)
    })?
}

async fn database<T: Send + 'static>(
    workers: &'static Semaphore,
    work: impl FnOnce() -> WorkbenchResult<T> + Send + 'static,
) -> WorkbenchResult<T> {
    let permit = workers
        .acquire()
        .await
        .map_err(|_| WorkbenchError::worker("Database workers are unavailable", true))?;
    blocking(move || {
        let _permit = permit;
        work()
    })
    .await
}

pub(crate) async fn run_store_phased<P, Q, T>(
    prepare: impl FnOnce(&Store) -> WorkbenchResult<P> + Send + 'static,
    work: impl FnOnce(P) -> WorkbenchResult<Q> + Send + 'static,
    finish: impl FnOnce(&Store, Q) -> WorkbenchResult<T> + Send + 'static,
) -> WorkbenchResult<T>
where
    P: Send + 'static,
    Q: Send + 'static,
    T: Send + 'static,
{
    run_with(
        Resources {
            database: &DATABASE_WORKERS,
            files: &FILE_WORKERS,
            gate: &DATABASE_GATE,
        },
        Store::open_default,
        prepare,
        work,
        finish,
    )
    .await
}

async fn run_with<P, Q, T>(
    resources: Resources,
    open: impl FnOnce() -> WorkbenchResult<Store> + Send + 'static,
    prepare: impl FnOnce(&Store) -> WorkbenchResult<P> + Send + 'static,
    work: impl FnOnce(P) -> WorkbenchResult<Q> + Send + 'static,
    finish: impl FnOnce(&Store, Q) -> WorkbenchResult<T> + Send + 'static,
) -> WorkbenchResult<T>
where
    P: Send + 'static,
    Q: Send + 'static,
    T: Send + 'static,
{
    // Queue before holding a storage gate or capturing paths/revisions.
    let file = resources
        .files
        .acquire()
        .await
        .map_err(|_| WorkbenchError::worker("Project file workers are unavailable", true))?;
    let gate = resources.gate.read().await;
    let prepared = database(resources.database, move || {
        let store = open()?;
        let value = prepare(&store)?;
        Ok(Protected {
            store,
            gate,
            file,
            value,
        })
    })
    .await?;
    let result = blocking(move || prepared.map(work)).await?;
    database(resources.database, move || {
        let Protected {
            store,
            gate,
            file,
            value,
        } = result;
        let _protection = (gate, file);
        finish(&store, value)
    })
    .await
}

#[cfg(test)]
#[path = "storage_phases_tests.rs"]
mod tests;

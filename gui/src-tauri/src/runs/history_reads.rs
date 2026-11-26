//! Share overlapping native History scans. Completed values are not cached:
//! later requests see other processes' writes, Trash/restore and external copies.
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, LazyLock,
};
use tokio::sync::{watch, Mutex};

type ReadResult<T> = Result<Arc<T>, String>;
type Waiting<T> = watch::Receiver<Option<ReadResult<T>>>;
type PendingRead<T> = Arc<Mutex<Option<(u64, Waiting<T>)>>>;

struct ReadFlight<T> {
    pending: PendingRead<T>,
    generation: AtomicU64,
}

impl<T: Send + Sync + 'static> ReadFlight<T> {
    fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(None)),
            generation: AtomicU64::new(0),
        }
    }

    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    async fn get(
        &self,
        read: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> ReadResult<T> {
        let mut read = Some(read);
        loop {
            let mut pending = self.pending.lock().await;
            let generation = self.generation.load(Ordering::Acquire);
            let (receiver, stale) = if let Some((started, receiver)) = pending.as_ref() {
                (receiver.clone(), *started != generation)
            } else {
                let (sender, receiver) = watch::channel(None);
                *pending = Some((generation, receiver.clone()));
                let slot = self.pending.clone();
                let read = read.take().expect("read admitted once");
                // The worker outlives cancelled callers and owns its scan slot.
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(read)
                        .await
                        .map_err(|error| format!("History read worker stopped: {error}"))
                        .and_then(|value| value)
                        .map(Arc::new);
                    let mut pending = slot.lock().await;
                    *pending = None;
                    sender.send_replace(Some(result));
                });
                (receiver, false)
            };
            drop(pending);
            let result = Self::wait(receiver).await;
            if !stale {
                return result;
            }
            // A write completed before this request. Wait out the older scan,
            // then start/join a fresh one without allowing concurrent scans.
        }
    }

    async fn wait(mut receiver: Waiting<T>) -> ReadResult<T> {
        loop {
            let result = receiver.borrow_and_update().clone();
            if let Some(result) = result {
                return result;
            }
            receiver
                .changed()
                .await
                .map_err(|_| "History read worker stopped without a result".to_string())?;
        }
    }
}

static LIST: LazyLock<ReadFlight<Vec<super::RunSummary>>> = LazyLock::new(ReadFlight::new);
static USAGE: LazyLock<ReadFlight<super::RunsDiskUsage>> = LazyLock::new(ReadFlight::new);

pub(super) fn changed() {
    LIST.invalidate();
    USAGE.invalidate();
}

pub(crate) async fn list() -> Result<Vec<super::RunSummary>, String> {
    LIST.get(super::history::list_with_recovery)
        .await
        .map(|runs| (*runs).clone())
}

pub(crate) async fn usage() -> Result<super::RunsDiskUsage, String> {
    USAGE
        .get(super::disk_usage)
        .await
        .map(|usage| (*usage).clone())
}

#[cfg(test)]
#[path = "tests/history_reads.rs"]
mod tests;

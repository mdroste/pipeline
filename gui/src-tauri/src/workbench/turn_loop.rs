//! What the API and Claude turn loops have in common and nothing more: a clock
//! that keeps model time apart from host time, and the sink for live events.
//! Neither carries runtime state, so each turn loop keeps its own lifecycle.
use crate::agent_runtime::events::NormalizedEvent;
use std::time::Duration;
use tauri::Emitter;

/// Host time allowed for one turn: two research runs at the longest profile
/// timeout (7,200 seconds), or one that first waits behind another job.
pub(crate) const HOST_TIME_LIMIT: Duration = Duration::from_secs(4 * 60 * 60);

/// A turn deadline that stands still during host work, which has its own
/// allowance for the turn.
pub(crate) struct ModelClock {
    deadline: tokio::time::Instant,
    host_left: Duration,
}

impl ModelClock {
    pub(crate) fn new(model_limit: Duration, host_limit: Duration) -> Self {
        Self {
            deadline: tokio::time::Instant::now() + model_limit,
            host_left: host_limit,
        }
    }

    /// Waits for model output or input; `None` once the model time is used up.
    pub(crate) async fn model<T>(&self, work: impl std::future::Future<Output = T>) -> Option<T> {
        tokio::time::timeout_at(self.deadline, work).await.ok()
    }

    /// Runs host work and moves the deadline by the time it took; `None`
    /// once the turn's host time is used up.
    pub(crate) async fn host<T>(
        &mut self,
        work: impl std::future::Future<Output = T>,
    ) -> Option<T> {
        let started = tokio::time::Instant::now();
        let result = tokio::time::timeout(self.host_left, work).await.ok();
        let elapsed = started.elapsed();
        self.deadline += elapsed;
        self.host_left = self.host_left.saturating_sub(elapsed);
        result
    }
}

/// Where a turn's live events go once they are saved.
pub(crate) trait TurnEvents: Send + Sync {
    fn publish(&self, event: &NormalizedEvent) -> Result<(), String>;
}

impl TurnEvents for tauri::AppHandle {
    fn publish(&self, event: &NormalizedEvent) -> Result<(), String> {
        self.emit("workbench:event", event)
            .map_err(|error| error.to_string())
    }
}

/// Keeps a test's events, and can stand in for a slow step of the turn.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct RecordedEvents {
    pub(crate) events: std::sync::Mutex<Vec<NormalizedEvent>>,
    /// How long publishing a started tool takes. It happens inside the tool
    /// step, so it plays the part of a long-running tool.
    pub(crate) tool_delay: Duration,
}

#[cfg(test)]
impl TurnEvents for RecordedEvents {
    fn publish(&self, event: &NormalizedEvent) -> Result<(), String> {
        if matches!(event, NormalizedEvent::ItemStarted { .. }) {
            std::thread::sleep(self.tool_delay);
        }
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

use super::*;

/// Foreground GUI/CLI runs execute behind a scheduler boundary just like
/// provider calls. Keeping the join handle owned prevents a dropped Tauri
/// invocation or CLI waiter from detaching a run that still holds the global
/// pipeline lock and child processes.
pub(super) struct PipelineTask {
    handle: Option<tokio::task::JoinHandle<std::result::Result<serde_json::Value, String>>>,
}

pub(super) struct PipelineRequest {
    pub(super) bus: crate::emit::EventBus,
    pub(super) paper_path: String,
    pub(super) input_interpretation: Option<String>,
    pub(super) diff: bool,
    pub(super) variables: std::collections::HashMap<String, String>,
    pub(super) extra_inputs: std::collections::HashMap<String, String>,
    pub(super) snapshot: Option<RunSnapshot>,
}

impl PipelineTask {
    pub(super) fn spawn_future(
        future: impl std::future::Future<Output = std::result::Result<serde_json::Value, String>>
            + Send
            + 'static,
    ) -> Self {
        Self {
            handle: Some(tokio::spawn(future)),
        }
    }

    pub(super) fn spawn(guard: PipelineGuard, request: PipelineRequest) -> Self {
        Self::spawn_future(async move {
            let _guard = guard;
            run_pipeline_inner_with_snapshot(
                &request.bus,
                &request.paper_path,
                request.input_interpretation.as_deref(),
                request.diff,
                request.variables,
                request.extra_inputs,
                request.snapshot,
            )
            .await
        })
    }

    pub(super) async fn join(mut self) -> Result<serde_json::Value, String> {
        // Retain ownership while awaiting so Drop can abort instead of detach
        // if the command/CLI waiter itself is cancelled.
        let joined = self
            .handle
            .as_mut()
            .expect("pipeline task handle missing")
            .await;
        let _ = self.handle.take();
        joined.map_err(|error| {
            if error.is_panic() {
                format!("Pipeline task panicked: {error}")
            } else {
                format!("Pipeline task was cancelled unexpectedly: {error}")
            }
        })?
    }
}

impl Drop for PipelineTask {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            CANCEL_FLAG.store(true, std::sync::atomic::Ordering::Release);
            signal_cancellation();
            kill_all_children();
            handle.abort();
        }
    }
}

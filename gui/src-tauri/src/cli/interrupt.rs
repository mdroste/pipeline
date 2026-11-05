//! CLI signal handling, owned cancellation waits, and interruption status.

use pipeline_gui_lib::emit::EventBus;
use pipeline_gui_lib::{commands, engines};

#[cfg(unix)]
async fn shutdown_signal() {
    use std::future::Future as _;
    use tokio::signal::unix::{signal, SignalKind};
    match signal(SignalKind::terminate()) {
        Ok(mut terminate) => {
            let mut ctrl_c = std::pin::pin!(tokio::signal::ctrl_c());
            let mut terminate = std::pin::pin!(terminate.recv());
            std::future::poll_fn(|cx| {
                if ctrl_c.as_mut().poll(cx).is_ready() || terminate.as_mut().poll(cx).is_ready() {
                    std::task::Poll::Ready(())
                } else {
                    std::task::Poll::Pending
                }
            })
            .await;
        }
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

pub(super) async fn run_with_interrupt(
    bus: EventBus,
    options: commands::HeadlessRunOptions,
    batch: Option<&commands::HeadlessBatchSnapshot>,
) -> Result<serde_json::Value, String> {
    use std::future::Future as _;
    let mut run = std::pin::pin!(commands::run_headless_batch_item(bus, options, batch));
    let mut signal = std::pin::pin!(shutdown_signal());
    let completed = std::future::poll_fn(|cx| {
        if let std::task::Poll::Ready(result) = run.as_mut().poll(cx) {
            return std::task::Poll::Ready(Some(result));
        }
        if signal.as_mut().poll(cx).is_ready() {
            return std::task::Poll::Ready(None);
        }
        std::task::Poll::Pending
    })
    .await;
    match completed {
        Some(result) => result,
        None => {
            eprintln!("Interrupt received; cancelling the active run…");
            let _ = commands::cancel_pipeline(None).await;
            match tokio::time::timeout(std::time::Duration::from_secs(15), run.as_mut()).await {
                Ok(_) => Err("Pipeline cancelled by interrupt".to_string()),
                Err(_) => Err(
                    "Pipeline cancellation timed out; child processes were terminated".to_string(),
                ),
            }
        }
    }
}

pub(super) fn is_interruption(error: &str) -> bool {
    commands::is_pipeline_cancellation_error(error)
        || matches!(
            error,
            "Pipeline cancelled by interrupt"
                | "Pipeline cancellation timed out; child processes were terminated"
                | "Engine installation cancelled by interrupt"
                | "Engine installation cancellation timed out; child processes were terminated"
                | "Install cancelled"
                | "Installation cancelled"
        )
}

pub(super) fn report_run_error(error: &str) -> i32 {
    eprintln!("Error: {error}");
    if is_interruption(error) {
        130
    } else {
        1
    }
}

pub(super) async fn install_engine_with_interrupt(bus: EventBus, id: &str) -> Result<(), String> {
    use std::future::Future as _;
    let mut install = std::pin::pin!(engines::install_engine(&bus, id));
    let mut signal = std::pin::pin!(shutdown_signal());
    let completed = std::future::poll_fn(|cx| {
        if let std::task::Poll::Ready(result) = install.as_mut().poll(cx) {
            return std::task::Poll::Ready(Some(result));
        }
        if signal.as_mut().poll(cx).is_ready() {
            return std::task::Poll::Ready(None);
        }
        std::task::Poll::Pending
    })
    .await;
    match completed {
        Some(result) => result,
        None => {
            eprintln!("Interrupt received; cancelling the engine installation…");
            engines::cancel_install();
            match tokio::time::timeout(std::time::Duration::from_secs(15), install.as_mut()).await {
                Ok(_) => Err("Engine installation cancelled by interrupt".to_string()),
                Err(_) => Err(
                    "Engine installation cancellation timed out; child processes were terminated"
                        .to_string(),
                ),
            }
        }
    }
}

//! Cancellation before dispatch is definitive. Once a native request is in
//! flight, its owner must await acknowledgement and interrupt the accepted turn.
use super::commands::RuntimeCommandError;
use std::future::Future;
use tokio::sync::watch;

#[derive(Clone, Default)]
pub(crate) struct Control(pub Option<watch::Receiver<bool>>);

impl Control {
    fn error() -> RuntimeCommandError {
        super::store::WorkbenchError {
            code: "turn_not_sent".into(),
            message: "Task stopped before dispatch; the message was not sent".into(),
            retryable: false,
            recovery: None,
        }
        .into()
    }
    pub fn check(&self) -> Result<(), RuntimeCommandError> {
        if self
            .0
            .as_ref()
            .is_some_and(|stop| *stop.borrow() || stop.has_changed().is_err())
        {
            Err(Self::error())
        } else {
            Ok(())
        }
    }

    pub async fn stopped(&self) {
        let Some(mut stop) = self.0.clone() else {
            return std::future::pending().await;
        };
        while !*stop.borrow_and_update() {
            if stop.changed().await.is_err() {
                break;
            }
        }
    }

    /// Only for setup work that cannot send a user prompt. Dropping a turn
    /// dispatch here would lose acknowledgement and make delivery ambiguous.
    pub async fn preflight<T>(
        &self,
        work: impl Future<Output = Result<T, RuntimeCommandError>>,
    ) -> Result<T, RuntimeCommandError> {
        self.check()?;
        let result = tokio::select! {
            biased;
            _ = self.stopped() => return Err(Self::error()),
            result = work => result,
        };
        self.check()?;
        result
    }

    pub async fn run<T>(&self, work: impl Future<Output = Result<T, String>>) -> Result<T, String> {
        tokio::select! {
            biased;
            _ = self.stopped() => Err("interrupted".into()),
            result = work => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn stop_during_preflight_prevents_dispatch_and_releases_setup() {
        for initially_stopped in [false, true] {
            let (stop, receiver) = watch::channel(initially_stopped);
            let control = Control(Some(receiver));
            let dispatched = Arc::new(AtomicBool::new(false));
            let sent = dispatched.clone();
            let (entered, ready) = tokio::sync::oneshot::channel();
            let queue = Arc::new(tokio::sync::Semaphore::new(1));
            let permit = queue.clone().acquire_owned().await.unwrap();
            let task = tokio::spawn(async move {
                let _setup_permit = permit;
                control
                    .preflight(async {
                        let _ = entered.send(());
                        std::future::pending::<Result<(), RuntimeCommandError>>().await
                    })
                    .await?;
                sent.store(true, Ordering::SeqCst);
                Ok::<_, RuntimeCommandError>(())
            });
            if !initially_stopped {
                ready.await.unwrap();
                stop.send(true).unwrap();
            }
            assert_eq!(
                serde_json::to_value(task.await.unwrap().unwrap_err()).unwrap()["code"],
                "turn_not_sent"
            );
            assert!(!dispatched.load(Ordering::SeqCst));
            assert_eq!(queue.available_permits(), 1);
        }
    }

    #[tokio::test]
    async fn stop_after_setup_prevents_polling_the_provider_worker() {
        let (stop, receiver) = watch::channel(false);
        let control = Control(Some(receiver));
        control.preflight(async { Ok(()) }).await.unwrap();
        stop.send(true).unwrap();
        let result = control
            .run(async { panic!("provider prompt must not be sent") })
            .await;
        assert_eq!(result, Err::<(), _>("interrupted".into()));
    }

    #[tokio::test]
    async fn expired_or_abandoned_task_never_dispatches() {
        let (stop, receiver) = watch::channel(false);
        let control = Control(Some(receiver));
        let timeout = tokio::spawn(async move {
            tokio::task::yield_now().await;
            stop.send(true).unwrap();
        });
        assert_eq!(
            serde_json::to_value(
                control
                    .preflight(std::future::pending::<Result<(), RuntimeCommandError>>())
                    .await
                    .unwrap_err()
            )
            .unwrap()["code"],
            "turn_not_sent"
        );
        timeout.await.unwrap();
        let (stop, receiver) = watch::channel(false);
        drop(stop);
        assert!(Control(Some(receiver)).check().is_err());
        assert_eq!(
            Control::default()
                .preflight(async { Ok(42) })
                .await
                .unwrap(),
            42
        );
    }
}

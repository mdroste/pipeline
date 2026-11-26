//! Common local-turn completion order: capture, cancel host work, persist/publish,
//! clear provider ownership, release admission, announce. Providers retain their
//! partial-text, error item, usage payload, and automatic-title policies.
use super::{announce_active_turn, finish_local_turn, run_store};
use crate::agent_runtime::events::NormalizedEvent;
use crate::workbench::{conversation_tools, research, turn_loop::persist_and_publish};
use tauri::Emitter;
pub(crate) struct LocalCompletion<'a> {
    pub epoch: u64,
    pub thread: &'a str,
    pub turn: &'a str,
    pub binding: &'a str,
}
impl LocalCompletion<'_> {
    pub async fn capture(&self, app: &tauri::AppHandle) {
        let capture = conversation_tools::finish(self.thread, self.turn).await;
        conversation_tools::report_capture(app, self.epoch, capture);
        let thread = self.thread.to_owned();
        let turn = self.turn.to_owned();
        let _ = run_store(move |store| {
            research::cancel_executions_for_provider_turn(&store, &thread, &turn)
        })
        .await;
    }
    pub async fn publish(
        self,
        app: &tauri::AppHandle,
        event: NormalizedEvent,
        clear_provider: impl FnOnce(),
    ) {
        if persist_and_publish(self.binding, app, event.clone())
            .await
            .is_err()
        {
            let _ = app.emit("workbench:event", event);
        }
        clear_provider();
        finish_local_turn(self.epoch, self.thread, self.turn);
        announce_active_turn(app, None);
    }
}
pub(crate) fn outcome(result: Result<(), String>) -> (&'static str, Option<String>) {
    match result {
        Ok(()) => ("completed", None),
        Err(message) if message == "interrupted" => ("interrupted", None),
        Err(message) => ("failed", Some(message)),
    }
}
#[cfg(test)]
mod tests {
    use super::outcome;
    #[test]
    fn only_the_owned_interruption_is_cancellation() {
        assert_eq!(outcome(Ok(())), ("completed", None));
        assert_eq!(outcome(Err("interrupted".into())), ("interrupted", None));
        assert_eq!(
            outcome(Err("subscription interrupted".into())),
            ("failed", Some("subscription interrupted".into()))
        );
    }
}

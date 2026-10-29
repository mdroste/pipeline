//! Bounded native tool observations, kept distinct from assistant messages.
//! Completion is reported only when Claude emits the matching tool result.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Activity {
    calls: HashMap<String, Value>,
}
impl Activity {
    pub(super) async fn observe(
        &mut self,
        frame: &Value,
        turn: &Turn,
        app: &tauri::AppHandle,
    ) -> Result<(), String> {
        if !frame["parent_tool_use_id"].is_null() {
            return Ok(());
        }
        for part in frame["message"]["content"].as_array().into_iter().flatten() {
            let (native_id, completed) = match (frame["type"].as_str(), part["type"].as_str()) {
                (Some("assistant"), Some("tool_use")) => (part["id"].as_str(), false),
                (Some("user"), Some("tool_result")) => (part["tool_use_id"].as_str(), true),
                _ => continue,
            };
            let Some(native_id) = native_id.filter(|id| !id.is_empty() && id.len() <= 512) else {
                continue;
            };
            let item_id = format!("{}-tool-{:x}", turn.id, Sha256::digest(native_id));
            let item = if completed {
                let Some(mut item) = self.calls.remove(native_id) else {
                    continue;
                };
                item["status"] = json!(if part["is_error"] == true {
                    "failed"
                } else {
                    "completed"
                });
                item["output"] = json!(bounded(&part["content"]));
                item
            } else {
                if self.calls.len() >= 128 || self.calls.contains_key(native_id) {
                    continue;
                }
                let item = json!({"id":item_id,"type":"toolCall","runtime":"claude","tool":part["name"],"input":bounded(&part["input"]),"status":"inProgress"});
                self.calls.insert(native_id.into(), item.clone());
                item
            };
            let event = if completed {
                NormalizedEvent::ItemCompleted {
                    epoch: turn.epoch,
                    thread_id: turn.thread.clone(),
                    turn_id: turn.id.clone(),
                    item_id,
                    item_kind: "toolCall".into(),
                    item,
                }
            } else {
                NormalizedEvent::ItemStarted {
                    epoch: turn.epoch,
                    thread_id: turn.thread.clone(),
                    turn_id: turn.id.clone(),
                    item_id,
                    item_kind: "toolCall".into(),
                    item,
                }
            };
            turn.emit(app, event).await?;
        }
        Ok(())
    }
}
fn bounded(value: &Value) -> String {
    let text = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    crate::workbench::search::prefix(&text, 32 * 1024).to_string()
}

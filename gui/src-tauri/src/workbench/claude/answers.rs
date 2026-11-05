//! Claude's success result identifies the final answer. Until it arrives, the
//! latest assistant item remains open; a following assistant completes it as
//! commentary. Each item is completed once, matching the projector's replay key.
use super::*;

#[derive(Default)]
pub(super) struct Answers {
    pub index: usize,
    pending: Option<String>,
    pub final_received: bool,
}

impl Answers {
    pub fn stream_index(&self) -> usize {
        self.index + usize::from(self.pending.is_some())
    }
    pub fn observe(&mut self, value: &Value, turn: &Turn) -> Vec<NormalizedEvent> {
        let mut events = Vec::new();
        match value["type"].as_str() {
            Some("assistant") if value["parent_tool_use_id"].is_null() => {
                let text = value["message"]["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|part| part["type"] == "text")
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                if let Some(previous) = self.pending.take() {
                    events.push(self.event(turn, &previous, true, false));
                    self.index += 1;
                }
                if !text.is_empty() {
                    events.push(self.event(turn, &text, false, false));
                    self.pending = Some(text);
                }
            }
            Some("result") => {
                let success = value["is_error"] == false && value["subtype"] == "success";
                let text = if success {
                    value["result"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .or_else(|| self.pending.take())
                } else {
                    self.pending.take()
                };
                if let Some(text) = text {
                    events.push(self.event(turn, &text, true, success));
                    self.final_received = success;
                }
                self.pending = None;
            }
            _ => {}
        }
        events
    }

    fn event(
        &self,
        turn: &Turn,
        text: &str,
        complete: bool,
        final_answer: bool,
    ) -> NormalizedEvent {
        let item_id = format!("{}-assistant-{}", turn.id, self.index);
        let item = json!({"id":item_id,"type":"agentMessage","text":text,"runtime":"claude","model":turn.model,
            "phase": if final_answer { "final_answer" } else { "commentary" }});
        if complete {
            NormalizedEvent::ItemCompleted {
                epoch: turn.epoch,
                thread_id: turn.thread.clone(),
                turn_id: turn.id.clone(),
                item_id,
                item_kind: "agentMessage".into(),
                item,
            }
        } else {
            NormalizedEvent::ItemStarted {
                epoch: turn.epoch,
                thread_id: turn.thread.clone(),
                turn_id: turn.id.clone(),
                item_id,
                item_kind: "agentMessage".into(),
                item,
            }
        }
    }
}

//! Bounded SSE assembly. Only complete, terminal provider replies enter the
//! tool loop. Text deltas are presentation only; signatures and tool arguments
//! are retained in native messages and never emitted as assistant text.
use super::wire;
use crate::connections::ApiProvider as Provider;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) struct Stream {
    provider: Provider,
    pending: Vec<u8>,
    data: String,
    value: Value,
    calls: BTreeMap<usize, Value>,
    inputs: BTreeMap<usize, String>,
    done: bool,
    total: usize,
}

impl Stream {
    pub fn new(provider: Provider) -> Self {
        Self {
            provider,
            pending: vec![],
            data: String::new(),
            calls: BTreeMap::new(),
            inputs: BTreeMap::new(),
            done: false,
            total: 0,
            value: match provider {
                Provider::Anthropic => json!({"content":[],"usage":{}}),
                Provider::Google => json!({"candidates":[{"content":{"role":"model","parts":[]}}]}),
                _ => json!({"choices":[{"message":{"role":"assistant","content":""}}]}),
            },
        }
    }

    pub fn push(&mut self, bytes: &[u8], emit: &mut dyn FnMut(&str)) -> Result<(), String> {
        self.total += bytes.len();
        if self.total > wire::MAX_BODY {
            return Err("Model response exceeded 8 MB".into());
        }
        self.pending.extend_from_slice(bytes);
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line = self.pending.drain(..=end).collect::<Vec<_>>();
            let line = std::str::from_utf8(&line)
                .map_err(|_| "Invalid UTF-8 in model stream")?
                .trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    let data = std::mem::take(&mut self.data);
                    self.event(data.trim_end_matches('\n'), emit)?;
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data.push_str(data.strip_prefix(' ').unwrap_or(data));
                self.data.push('\n');
            }
        }
        Ok(())
    }

    fn event(&mut self, data: &str, emit: &mut dyn FnMut(&str)) -> Result<(), String> {
        if self.done {
            return Err("Unexpected data after model stream completion".into());
        }
        if data == "[DONE]" && matches!(self.provider, Provider::OpenAi | Provider::Local) {
            self.done = true;
            return Ok(());
        }
        let event: Value =
            serde_json::from_str(data).map_err(|_| "Invalid JSON in model stream")?;
        if event.get("error").is_some() || event["type"] == "error" {
            return Err(
                "Model server reported a streaming error; the request was not retried".into(),
            );
        }
        match self.provider {
            Provider::OpenAi | Provider::Local => {
                if event["usage"].is_object() {
                    self.value["usage"] = event["usage"].clone();
                }
                for choice in event["choices"].as_array().into_iter().flatten() {
                    if choice["index"].as_u64().unwrap_or(0) != 0 {
                        continue;
                    }
                    let delta = &choice["delta"];
                    for field in ["content", "refusal"] {
                        if let Some(text) = delta[field].as_str() {
                            append(&mut self.value["choices"][0]["message"][field], text);
                            emit(text);
                        }
                    }
                    // Some OpenAI-compatible reasoning servers require these
                    // opaque fields on subsequent tool exchanges.
                    for field in ["reasoning_content", "reasoning"] {
                        if let Some(text) = delta[field].as_str() {
                            append(&mut self.value["choices"][0]["message"][field], text);
                        }
                    }
                    for call in delta["tool_calls"].as_array().into_iter().flatten() {
                        let index = index(call)?;
                        let target = self.calls.entry(index).or_insert_with(|| json!({"type":"function","id":"","function":{"name":"","arguments":""}}));
                        if let Some(id) = call["id"].as_str() {
                            append(&mut target["id"], id);
                        }
                        if let Some(kind) = call["type"].as_str() {
                            if kind != "function" {
                                return Err("Unsupported streamed tool type".into());
                            }
                        }
                        for field in ["name", "arguments"] {
                            if let Some(text) = call["function"][field].as_str() {
                                append(&mut target["function"][field], text);
                            }
                        }
                    }
                    if !choice["finish_reason"].is_null() {
                        self.value["choices"][0]["finish_reason"] = choice["finish_reason"].clone();
                    }
                }
            }
            Provider::Anthropic => match event["type"].as_str() {
                Some("message_start") => {
                    self.value["usage"] = event["message"]["usage"].clone();
                }
                Some("content_block_start") => {
                    let index = index(&event)?;
                    if self
                        .calls
                        .insert(index, event["content_block"].clone())
                        .is_some()
                    {
                        return Err("Duplicate streamed content block".into());
                    }
                    if event["content_block"]["type"] == "text" {
                        if let Some(text) = event["content_block"]["text"].as_str() {
                            emit(text);
                        }
                    }
                }
                Some("content_block_delta") => {
                    let index = index(&event)?;
                    let target = self
                        .calls
                        .get_mut(&index)
                        .ok_or("Missing streamed content block")?;
                    let delta = &event["delta"];
                    match delta["type"].as_str() {
                        Some("text_delta") => {
                            let text = delta["text"].as_str().ok_or("Missing text delta")?;
                            append(&mut target["text"], text);
                            emit(text);
                        }
                        Some("input_json_delta") => {
                            self.inputs.entry(index).or_default().push_str(
                                delta["partial_json"]
                                    .as_str()
                                    .ok_or("Missing argument delta")?,
                            );
                        }
                        Some("thinking_delta") => {
                            append(
                                &mut target["thinking"],
                                delta["thinking"].as_str().ok_or("Missing thinking delta")?,
                            );
                        }
                        Some("signature_delta") => {
                            append(
                                &mut target["signature"],
                                delta["signature"]
                                    .as_str()
                                    .ok_or("Missing signature delta")?,
                            );
                        }
                        _ => (),
                    }
                }
                Some("message_delta") => {
                    self.value["stop_reason"] = event["delta"]["stop_reason"].clone();
                    if let Some(usage) = event["usage"].as_object() {
                        if !self.value["usage"].is_object() {
                            self.value["usage"] = json!({});
                        }
                        self.value["usage"]
                            .as_object_mut()
                            .unwrap()
                            .extend(usage.clone());
                    }
                }
                Some("message_stop") => self.done = true,
                _ => (),
            },
            Provider::Google => {
                if event["usageMetadata"].is_object() {
                    self.value["usageMetadata"] = event["usageMetadata"].clone();
                }
                for candidate in event["candidates"].as_array().into_iter().flatten() {
                    if candidate["index"].as_u64().unwrap_or(0) != 0 {
                        continue;
                    }
                    for part in candidate["content"]["parts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                    {
                        if part["thought"] != true {
                            if let Some(text) = part["text"].as_str() {
                                emit(text);
                            }
                        }
                        self.value["candidates"][0]["content"]["parts"]
                            .as_array_mut()
                            .unwrap()
                            .push(part.clone());
                    }
                    if !candidate["finishReason"].is_null() {
                        self.value["candidates"][0]["finishReason"] =
                            candidate["finishReason"].clone();
                    }
                }
            }
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<Value, String> {
        if !self.pending.iter().all(u8::is_ascii_whitespace)
            || !self.data.is_empty()
            || (self.provider != Provider::Google && !self.done)
        {
            return Err(
                "The model stream ended before completion; the request was not retried".into(),
            );
        }
        match self.provider {
            Provider::Anthropic => {
                // A tool without arguments streams an empty delta and keeps the
                // block's own `input: {}`. A reply cut off at the length limit
                // can end mid-argument, and its calls never run.
                let truncated = self.value["stop_reason"] == "max_tokens";
                for (index, input) in self.inputs {
                    if truncated || input.trim().is_empty() {
                        continue;
                    }
                    self.calls.get_mut(&index).ok_or("Missing tool block")?["input"] =
                        serde_json::from_str(&input)
                            .map_err(|_| "Invalid streamed tool arguments")?;
                }
                self.value["content"] = json!(self.calls.into_values().collect::<Vec<_>>());
            }
            Provider::OpenAi | Provider::Local if !self.calls.is_empty() => {
                self.value["choices"][0]["message"]["tool_calls"] =
                    json!(self.calls.into_values().collect::<Vec<_>>())
            }
            _ => (),
        }
        Ok(self.value)
    }
}
fn index(value: &Value) -> Result<usize, String> {
    value["index"]
        .as_u64()
        .filter(|n| *n < 128)
        .map(|n| n as usize)
        .ok_or_else(|| "Invalid streamed content index".into())
}
fn append(value: &mut Value, text: &str) {
    match value {
        Value::String(joined) => joined.push_str(text),
        _ => *value = Value::String(text.to_owned()),
    }
}

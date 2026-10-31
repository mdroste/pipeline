//! API request encoding, bounded exchange, and terminal reply decoding.
//! Callers own tools, retries, cancellation, and persistence.
//! Native assistant parts are retained within a turn (including Gemini thought
//! signatures); tool arguments are validated before any host action.
use crate::connections::{ApiConnection as Connection, ApiProvider as Provider};
use serde_json::{json, Value};
use std::time::Duration;

pub(crate) const MAX_BODY: usize = 8 * 1024 * 1024;
pub(crate) struct Reply {
    pub message: Value,
    pub text: String,
    pub calls: Vec<Call>,
    pub usage: Value,
}
pub(crate) struct Call {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub response_id: Option<String>,
}

pub(crate) fn initial(provider: Provider, instructions: &str, prompt: &str) -> Vec<Value> {
    match provider {
        Provider::Google => vec![json!({"role":"user","parts":[{"text":prompt}]})],
        Provider::Anthropic => vec![json!({"role":"user","content":prompt})],
        _ => vec![
            json!({"role":"system","content":instructions}),
            json!({"role":"user","content":prompt}),
        ],
    }
}

pub(crate) fn body(
    connection: &Connection,
    instructions: &str,
    messages: &[Value],
    specs: &[Value],
) -> Value {
    let mut body = match connection.provider {
        Provider::Anthropic => {
            json!({"model":connection.model,"max_tokens":8192,"system":instructions,"messages":messages})
        }
        Provider::Google => {
            json!({"systemInstruction":{"parts":[{"text":instructions}]},"contents":messages,"generationConfig":{"maxOutputTokens":8192}})
        }
        Provider::OpenAi => {
            json!({"model":connection.model,"messages":messages,"max_completion_tokens":8192})
        }
        Provider::Local => json!({"model":connection.model,"messages":messages,"max_tokens":8192}),
    };
    if let Some(effort) = &connection.effort {
        match connection.provider {
            Provider::Anthropic => {
                body["output_config"] = json!({"effort":effort});
                if let Some(thinking) = &connection.thinking {
                    body["thinking"] = thinking.clone();
                }
            }
            Provider::Google => {
                if let Some(thinking) = &connection.thinking {
                    body["generationConfig"]["thinkingConfig"] = thinking.clone();
                    body["generationConfig"]["maxOutputTokens"] = json!(16384);
                }
            }
            Provider::OpenAi => {
                if let Some(effort) = crate::connections::thinking::openai_effort(effort) {
                    body["reasoning_effort"] = json!(effort);
                }
            }
            Provider::Local => body["reasoning_effort"] = json!(effort),
        }
    }
    if !specs.is_empty() {
        let declarations: Vec<_> = specs
            .iter()
            .map(|spec| {
                super::function_tool(
                    connection.provider,
                    spec["name"].as_str().unwrap_or_default(),
                    spec["description"].as_str().unwrap_or_default(),
                    &spec["inputSchema"],
                )
            })
            .collect();
        body["tools"] = if connection.provider == Provider::Google {
            json!([{"functionDeclarations": declarations}])
        } else {
            json!(declarations)
        };
    }
    body
}

pub(crate) async fn request(
    client: &reqwest::Client,
    connection: &Connection,
    body: &Value,
) -> Result<Value, String> {
    exchange(client, connection, body, None).await
}

pub(crate) async fn exchange(
    client: &reqwest::Client,
    connection: &Connection,
    body: &Value,
    mut emit: Option<&mut (dyn FnMut(&str) + Send)>,
) -> Result<Value, String> {
    let mut body = body.clone();
    let mut endpoint = connection.endpoint.clone();
    if emit.is_some() {
        if connection.provider == Provider::Google {
            endpoint = endpoint.replace(":generateContent", ":streamGenerateContent");
            endpoint.push_str("?alt=sse");
        } else {
            body["stream"] = json!(true);
            if connection.provider == Provider::OpenAi {
                body["stream_options"] = json!({"include_usage":true});
            }
        }
    }
    let bytes = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BODY {
        return Err("Conversation exceeds the 8 MB request limit. Start a new conversation or use less context.".into());
    }
    let mut request = client
        .post(&endpoint)
        .header("content-type", "application/json")
        .body(bytes);
    request = super::authorize(connection.provider, request, &connection.key);
    let mut response = request.send().await.map_err(|_| {
        "Could not reach the model server. The request was not retried.".to_string()
    })?;
    let status = response.status();
    let streamed = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("text/event-stream"))
        });
    let mut stream = super::streaming::Stream::new(connection.provider);
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        "The model connection ended while receiving its reply. The request was not retried."
    })? {
        if streamed && status.is_success() {
            stream.push(&chunk, &mut |text| {
                if let Some(emit) = emit.as_mut() {
                    emit(text);
                }
            })?;
            continue;
        }
        if bytes.len() + chunk.len() > MAX_BODY {
            return Err("Model response exceeded 8 MB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    // Provider error bodies can echo credentials or research text; do not journal them.
    if !status.is_success() {
        return Err(format!("Model server returned HTTP {}. Check the connection, model, and account limits. The request was not retried.",status.as_u16()));
    }
    if streamed {
        return stream.finish();
    }
    serde_json::from_slice(&bytes).map_err(|_| "Model server returned invalid JSON".into())
}

#[cfg(test)]
pub(crate) fn client() -> Result<reqwest::Client, String> {
    build_client(true)
}

pub(crate) fn client_for(connection: &Connection) -> Result<reqwest::Client, String> {
    // Plain HTTP is admitted only for loopback local servers. Never route
    // those prompts or credentials through an inherited enterprise proxy.
    build_client(
        connection.provider == Provider::Local && connection.endpoint.starts_with("http://"),
    )
}

fn build_client(no_proxy: bool) -> Result<reqwest::Client, String> {
    super::http::client(no_proxy, Some(Duration::from_secs(600)))
}

pub(crate) fn parse(provider: Provider, value: Value) -> Result<Reply, String> {
    let (message, parts, usage) = match provider {
        Provider::Anthropic => {
            if !matches!(
                value["stop_reason"].as_str(),
                Some("end_turn" | "tool_use" | "stop_sequence")
            ) {
                return Err("The API reply was incomplete or blocked".into());
            }
            (
                json!({"role":"assistant","content":value["content"]}),
                value["content"].clone(),
                value["usage"].clone(),
            )
        }
        Provider::Google => {
            let candidate = &value["candidates"][0];
            if candidate["finishReason"] != "STOP" {
                return Err("The API reply was incomplete or blocked".into());
            }
            (
                candidate["content"].clone(),
                candidate["content"]["parts"].clone(),
                value["usageMetadata"].clone(),
            )
        }
        _ => {
            let choice = &value["choices"][0];
            if !matches!(
                choice["finish_reason"].as_str(),
                Some("stop" | "tool_calls")
            ) {
                return Err("The API reply was incomplete or blocked".into());
            }
            (
                choice["message"].clone(),
                Value::Null,
                value["usage"].clone(),
            )
        }
    };
    let mut text = String::new();
    let mut calls = Vec::new();
    if matches!(provider, Provider::OpenAi | Provider::Local) {
        if message["role"] != "assistant" {
            return Err("Invalid assistant reply".into());
        }
        text = message["content"]
            .as_str()
            .filter(|text| !text.is_empty())
            .or_else(|| message["refusal"].as_str())
            .unwrap_or_default()
            .into();
        for call in message["tool_calls"].as_array().into_iter().flatten() {
            if call["type"] != "function" {
                return Err("Unsupported tool call type".into());
            }
            calls.push(Call {
                response_id: None,
                id: call["id"].as_str().unwrap_or_default().into(),
                name: call["function"]["name"].as_str().unwrap_or_default().into(),
                arguments: serde_json::from_str(
                    call["function"]["arguments"]
                        .as_str()
                        .ok_or("Missing tool arguments")?,
                )
                .map_err(|_| "Invalid tool arguments")?,
            });
        }
    } else {
        for (index, part) in parts
            .as_array()
            .ok_or("Missing reply content")?
            .iter()
            .enumerate()
        {
            if part["thought"] != true {
                if let Some(t) = part["text"].as_str() {
                    text.push_str(t);
                }
            }
            if provider == Provider::Anthropic && part["type"] == "tool_use" {
                calls.push(Call {
                    response_id: None,
                    id: part["id"].as_str().unwrap_or_default().into(),
                    name: part["name"].as_str().unwrap_or_default().into(),
                    arguments: part["input"].clone(),
                });
            } else if let Some(call) = part.get("functionCall") {
                calls.push(Call {
                    response_id: call["id"].as_str().map(str::to_owned),
                    id: call["id"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("call-{index}")),
                    name: call["name"].as_str().unwrap_or_default().into(),
                    arguments: call["args"].clone(),
                });
            }
        }
    }
    if calls.len() > 128
        || calls.iter().any(|c| {
            c.id.is_empty()
                || c.id.len() > 512
                || c.name.is_empty()
                || c.name.len() > 200
                || !c.arguments.is_object()
                || c.arguments.to_string().len() > 256 * 1024
        })
    {
        return Err("Invalid or oversized tool calls".into());
    }
    let unique: std::collections::HashSet<_> = calls.iter().map(|c| &c.id).collect();
    if unique.len() != calls.len() {
        return Err("Duplicate tool call identifiers".into());
    }
    if text.trim().is_empty() && calls.is_empty() {
        return Err("Model returned no assistant response".into());
    }
    Ok(Reply {
        message,
        text,
        calls,
        usage,
    })
}

pub(crate) fn tool_results(provider: Provider, results: Vec<(&Call, Value)>) -> Vec<Value> {
    match provider {
        Provider::Anthropic => vec![
            json!({"role":"user","content":results.into_iter().map(|(call,result)| json!({"type":"tool_result","tool_use_id":call.id,"is_error":result["success"]!=true,"content":content(&result,provider)})).collect::<Vec<_>>()}),
        ],
        Provider::Google => vec![
            json!({"role":"user","parts":results.into_iter().flat_map(|(call,result)| {
            let mut function=json!({"name":call.name,"response":{"success":result["success"],"content":text_content(&result)}});
            if let Some(id)=&call.response_id { function["id"]=json!(id); }
            let mut parts=vec![json!({"functionResponse":function})];
            parts.extend(content(&result,provider).into_iter().filter(|p|p.get("inlineData").is_some())); parts
        }).collect::<Vec<_>>()}),
        ],
        _ => {
            let mut messages = Vec::new();
            let mut images = Vec::new();
            for (call, result) in results {
                messages.push(json!({"role":"tool","tool_call_id":call.id,"content":json!({"success":result["success"],"content":text_content(&result)}).to_string()}));
                images.extend(
                    content(&result, provider)
                        .into_iter()
                        .filter(|p| p["type"] == "image_url"),
                );
            }
            if !images.is_empty() {
                messages.push(json!({"role":"user","content":images}));
            }
            messages
        }
    }
}
fn text_content(result: &Value) -> String {
    result["contentItems"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
fn content(result: &Value, provider: Provider) -> Vec<Value> {
    result["contentItems"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| {
            if let Some(text) = part["text"].as_str() {
                return Some(if provider == Provider::Google {
                    json!({"text":text})
                } else {
                    json!({"type":"text","text":text})
                });
            }
            let url = part["imageUrl"].as_str()?;
            let (mime, data) = url.strip_prefix("data:")?.split_once(";base64,")?;
            Some(match provider {
                Provider::Anthropic => {
                    json!({"type":"image","source":{"type":"base64","media_type":mime,"data":data}})
                }
                Provider::Google => json!({"inlineData":{"mimeType":mime,"data":data}}),
                _ => json!({"type":"image_url","image_url":{"url":url}}),
            })
        })
        .collect()
}

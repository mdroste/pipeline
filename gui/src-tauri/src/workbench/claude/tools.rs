//! MCP bridge to existing Workspace services; callers cannot choose their scope.
use super::*;

pub(super) fn catalog(effective: &super::super::research::EffectiveHarness) -> Vec<Value> {
    effective.dynamic_tools.iter().map(|v| json!({"name":v["name"],"description":v["description"],"inputSchema":v["inputSchema"]})).collect()
}

pub(super) async fn message(
    request: &Value,
    specs: &[Value],
    turn: Option<&Turn>,
) -> Result<Value, String> {
    if request["server_name"] != "pipeline" {
        return Err("Unknown Claude MCP server".into());
    }
    let message = &request["message"];
    let id = &message["id"];
    let result = match message["method"].as_str() {
        Some("initialize") => {
            json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"pipeline","version":env!("CARGO_PKG_VERSION")}})
        }
        Some("notifications/initialized" | "ping") => json!({}),
        Some("tools/list") => json!({"tools":specs}),
        Some("tools/call") => {
            let turn = turn.ok_or("A tool call arrived before a conversation turn was accepted")?;
            let name = message["params"]["name"]
                .as_str()
                .ok_or("Tool name missing")?;
            if !specs.iter().any(|spec| spec["name"] == name) {
                return Err("Tool is not enabled for this conversation".into());
            }
            let input = message["params"]["arguments"].clone();
            if !input.is_object() || input.to_string().len() > 256 * 1024 {
                return Err("Tool arguments must be an object through 256 KiB".into());
            }
            let name = name.to_owned();
            let thread = turn.thread.clone();
            let turn_id = turn.id.clone();
            // MCP request IDs are untrusted and not suitable database IDs.
            use sha2::{Digest, Sha256};
            let call_id = format!("claude-{:x}", Sha256::digest(format!("{turn_id}:{}", id)));
            let params = json!({"threadId":thread,"turnId":turn_id,"callId":call_id,"tool":name,"arguments":input});
            let response = if super::super::conversation_tools::owns(&name) {
                super::super::conversation_tools::handle(params).await
            } else {
                run_store(move |store| super::super::research::handle_dynamic_tool_call(&store, &params)).await
            };
            super::super::research::jobs::launch_pending();
            let mut response = match response {
                Ok(value) => value,
                Err(error) => {
                    json!({"success":false,"contentItems":[{"type":"inputText","text":error.message}]})
                }
            };
            if message["params"]["name"] == "workbench_research_run" && response["success"] == true
            {
                let execution = response["contentItems"][0]["text"]
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .and_then(|v| v["id"].as_str().map(str::to_owned));
                if let Some(id) = execution {
                    while super::super::research::jobs::execution_active(&id) {
                        tokio::time::sleep(std::time::Duration::from_millis(75)).await;
                    }
                    response = run_store(move |store| {
                        super::super::research::jobs::complete_tool_result(&store, &id)
                    })
                    .await
                    .map_err(|e| e.message)?;
                }
            }
            let content = response["contentItems"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| match item["type"].as_str() {
                    Some("inputText") => Some(json!({"type":"text","text":item["text"]})),
                    Some("inputImage") => item["imageUrl"]
                        .as_str()
                        .and_then(|url| url.strip_prefix("data:"))
                        .and_then(|data| data.split_once(";base64,"))
                        .map(|(mime, data)| json!({"type":"image","mimeType":mime,"data":data})),
                    _ => None,
                })
                .collect::<Vec<_>>();
            json!({"content":content,"isError":response["success"] != true})
        }
        _ => {
            return Ok(
                json!({"mcp_response":{"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Unsupported method"}}}),
            )
        }
    };
    Ok(json!({"mcp_response":{"jsonrpc":"2.0","id":id,"result":result}}))
}

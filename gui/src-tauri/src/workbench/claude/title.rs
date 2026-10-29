//! Automatic titles from one tool-free Claude Code process. It shares no
//! process, history, tools, or turn slot with the conversation it names.
use super::*;
use std::time::Duration;

/// Cheapest first, as documented `--model` aliases; then the account default.
const TITLE_MODELS: [Option<&str>; 3] = [Some("haiku"), Some("sonnet"), None];
const MAX_TITLE_REPLY_BYTES: usize = 16 * 1024;

fn arguments(model: Option<&str>) -> Vec<String> {
    let mut args = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--tools",
        "",
        "--disable-slash-commands",
        "--no-chrome",
        "--strict-mcp-config",
        "--permission-mode",
        "dontAsk",
        "--system-prompt",
        super::super::titles::TITLE_INSTRUCTIONS,
    ]
    .map(str::to_owned)
    .to_vec();
    if let Some(model) = model {
        args.push(format!("--model={model}"));
    }
    args
}

/// Returns the model's raw reply; the caller sanitizes it.
pub(crate) async fn generate(prompt: String, limit: Duration) -> Result<String, String> {
    // Dropping a timed-out process kills it.
    tokio::time::timeout(limit, async {
        for model in TITLE_MODELS {
            let mut process = transport::Process::start(&arguments(model), None).await?;
            let result = reply(&mut process, model, &prompt).await;
            process.stop().await;
            if let Some(text) = result? {
                return Ok(text);
            }
        }
        Err("Claude Code offered no model for the title".to_string())
    })
    .await
    .map_err(|_| "Claude did not return a title in time".to_string())?
}

/// `Ok(None)` when this account does not offer `model`.
async fn reply(
    process: &mut transport::Process,
    model: Option<&str>,
    prompt: &str,
) -> Result<Option<String>, String> {
    let initialization = process.initialize(&[]).await?;
    if model.is_some() && protocol::validate_model(&initialization, model, None).is_err() {
        return Ok(None);
    }
    protocol::validate_model(&initialization, None, None)?;
    process.write(&json!({"type":"user","message":{"role":"user","content":prompt},"parent_tool_use_id":null,"session_id":""})).await?;
    let mut text = String::new();
    for _ in 0..256 {
        let frame = process.read().await?;
        match frame["type"].as_str() {
            Some("control_request") => process.control(&frame, &[], None).await?,
            Some("rate_limit_event") => usage::record(&frame),
            Some("assistant") if frame["parent_tool_use_id"].is_null() => {
                for part in frame["message"]["content"].as_array().into_iter().flatten() {
                    if let Some(part) = part["text"].as_str().filter(|_| part["type"] == "text") {
                        text.push_str(part);
                    }
                }
                if text.len() > MAX_TITLE_REPLY_BYTES {
                    return Err("Claude's title reply exceeded its limit".into());
                }
            }
            Some("result") => {
                if frame["is_error"] != false || frame["subtype"] != "success" {
                    return Err(frame["result"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| "Claude could not write a title".into()));
                }
                if text.trim().is_empty() {
                    text = frame["result"].as_str().unwrap_or_default().to_owned();
                }
                return Ok(Some(text));
            }
            _ => {}
        }
    }
    Err("Claude's title reply exceeded its message limit".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_process_has_no_tools_history_or_project_context() {
        let args = arguments(Some("haiku"));
        let tools = args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(args[tools + 1], "");
        for flag in [
            "--no-session-persistence",
            "--disable-slash-commands",
            "--no-chrome",
            "--strict-mcp-config",
        ] {
            assert!(args.iter().any(|a| a == flag), "{flag}");
        }
        assert!(!args.iter().any(|a| a == "--mcp-config"));
        assert_eq!(args.last().unwrap(), "--model=haiku");
        assert!(!arguments(None).iter().any(|a| a.starts_with("--model")));
    }
}

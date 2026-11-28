//! One tool-free Claude Code process for a request outside any conversation:
//! automatic titles, and drafts of the researcher's style notes. It shares no
//! process, history, tools, or turn slot with any conversation.
use super::*;
use std::time::Duration;

/// Cheapest first, as documented `--model` aliases; then the account default.
const TITLE_MODELS: [Option<&str>; 3] = [Some("haiku"), Some("sonnet"), None];
const MAX_REPLY_BYTES: usize = 16 * 1024;

fn arguments(system_prompt: &str, model: Option<&str>) -> Vec<String> {
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
        system_prompt,
    ]
    .map(str::to_owned)
    .to_vec();
    if let Some(model) = model {
        args.push(format!("--model={model}"));
    }
    args
}

/// Returns the model's raw reply; the caller sanitizes it.
pub(crate) async fn generate(
    auth_mode: crate::agent_runtime::claude::AuthMode,
    prompt: String,
    limit: Duration,
) -> Result<String, String> {
    side_call(
        auth_mode,
        super::super::titles::TITLE_INSTRUCTIONS,
        &TITLE_MODELS,
        &prompt,
        limit,
    )
    .await
}

/// Tries each of `models` in turn, `None` being the account default, and
/// returns the first one's raw reply.
pub(crate) async fn side_call(
    auth_mode: crate::agent_runtime::claude::AuthMode,
    system_prompt: &str,
    models: &[Option<&str>],
    prompt: &str,
    limit: Duration,
) -> Result<String, String> {
    // Dropping a timed-out process kills it.
    tokio::time::timeout(limit, async {
        for model in models {
            let mut process =
                transport::Process::start(auth_mode, &arguments(system_prompt, *model), None, None)
                    .await?;
            let result = reply(&mut process, *model, prompt).await;
            process.stop().await;
            if let Some(text) = result? {
                return Ok(text);
            }
        }
        Err("Claude Code offered none of the models to try".to_string())
    })
    .await
    .map_err(|_| "Claude did not reply in time".to_string())?
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
            Some("rate_limit_event") => usage::record(&frame, process.usage_account.as_deref()),
            Some("assistant") if frame["parent_tool_use_id"].is_null() => {
                for part in frame["message"]["content"].as_array().into_iter().flatten() {
                    if let Some(part) = part["text"].as_str().filter(|_| part["type"] == "text") {
                        text.push_str(part);
                    }
                }
                if text.len() > MAX_REPLY_BYTES {
                    return Err("Claude's reply exceeded its limit".into());
                }
            }
            Some("result") => {
                if frame["is_error"] != false || frame["subtype"] != "success" {
                    return Err(frame["result"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| "Claude could not reply".into()));
                }
                if text.trim().is_empty() {
                    text = frame["result"].as_str().unwrap_or_default().to_owned();
                }
                return Ok(Some(text));
            }
            _ => {}
        }
    }
    Err("Claude's reply exceeded its message limit".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_process_has_no_tools_history_or_project_context() {
        let args = arguments("Name it.", Some("haiku"));
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
        let prompt = args.iter().position(|a| a == "--system-prompt").unwrap();
        assert_eq!(args[prompt + 1], "Name it.");
        assert_eq!(args.last().unwrap(), "--model=haiku");
        assert!(!arguments("Name it.", None)
            .iter()
            .any(|a| a.starts_with("--model")));
    }
}

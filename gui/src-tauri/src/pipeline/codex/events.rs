//! Legacy CLI progress, tool and final-message decoding.

pub(super) fn read_last_message_file(path: &std::path::Path) -> Result<Option<String>, String> {
    // A failed turn never writes this file, so its absence is not a problem.
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Could not inspect Codex final-message file: {error}"
            ))
        }
    };
    if metadata.len() > super::super::claude::MAX_STDOUT_BYTES as u64 {
        return Err(format!(
            "Codex final message exceeded the {} MB safety limit",
            super::super::claude::MAX_STDOUT_BYTES / 1_000_000
        ));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("Could not read Codex final-message file: {error}"))?;
    let trimmed = text.trim();
    Ok((!trimmed.is_empty()).then(|| trimmed.to_string()))
}

pub(super) fn update_last_agent_message(
    event: &serde_json::Value,
    last_agent_text: &mut String,
) -> Result<bool, String> {
    let Some(item) = event.get("item") else {
        return Ok(false);
    };
    if item.get("type").and_then(|kind| kind.as_str()) != Some("agent_message") {
        return Ok(false);
    }
    let Some(text) = item.get("text").and_then(|text| text.as_str()) else {
        return Ok(false);
    };
    if text.len() > super::super::claude::MAX_STDOUT_BYTES {
        return Err(format!(
            "Codex agent message exceeded the {} MB safety limit",
            super::super::claude::MAX_STDOUT_BYTES / 1_000_000
        ));
    }
    last_agent_text.clear();
    last_agent_text.push_str(text);
    Ok(true)
}

pub(super) fn codex_completed_tool_kind(
    event: &serde_json::Value,
) -> Option<crate::models::ToolCallKind> {
    use crate::models::ToolCallKind;

    if event.get("type").and_then(|kind| kind.as_str()) != Some("item.completed") {
        return None;
    }
    let item = event.get("item")?;
    let item_type = item.get("type").and_then(|kind| kind.as_str())?;
    match item_type {
        "file_change" | "file_read" | "file_write" => Some(ToolCallKind::TextFile),
        "command_execution" | "computer_call" | "computer_action" => {
            Some(ToolCallKind::ShellOrOther)
        }
        "web_search" | "web_fetch" => Some(ToolCallKind::Web),
        "image_generation" | "image_view" | "view_image" => Some(ToolCallKind::Image),
        "mcp_tool_call" | "function_call" | "tool_call" => {
            let name = item
                .get("tool")
                .or_else(|| item.get("name"))
                .or_else(|| {
                    item.get("function")
                        .and_then(|function| function.get("name"))
                })
                .and_then(|name| name.as_str())
                .unwrap_or("");
            Some(super::super::logging::classify_tool_name(name))
        }
        // Preserve future tool-like item kinds in an explicit unknown bucket.
        other if other.contains("tool") || other.ends_with("_call") => Some(ToolCallKind::Unknown),
        _ => None,
    }
}

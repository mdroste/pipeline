//! Bounded Claude stream-json contract and application-owned history.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static EPOCH: AtomicU64 = AtomicU64::new(1 << 40);
pub(super) fn next_epoch() -> u64 {
    EPOCH.fetch_add(1, Ordering::AcqRel)
}
pub(super) fn identity() -> WorkbenchResult<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|e| WorkbenchError::invalid(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub(super) struct Prompt {
    pub text: String,
    pub user_text: String,
}

pub(super) fn prompt(store: &Store, session: &str, text: &str) -> WorkbenchResult<Prompt> {
    if text.trim().is_empty() || text.len() > 1_000_000 || text.contains('\0') {
        return Err(WorkbenchError::invalid(
            "Claude messages must contain text and fit within 1 MB",
        ));
    }
    let user_text = text.to_owned();
    let text = format!(
        "{}\n\nCurrent user message:\n{text}",
        super::super::history::recent_context(store, session)?
    );
    Ok(Prompt { text, user_text })
}

/// `shell_root` offers the sandboxed Bash tool, reading that working root.
pub(super) fn arguments(
    effective: &super::super::research::EffectiveHarness,
    model: Option<&str>,
    effort: Option<&str>,
    shell_root: Option<&std::path::Path>,
) -> Vec<String> {
    let native_tools = match (shell_root.is_some(), effective.web_search) {
        (true, true) => "Bash,WebSearch,WebFetch",
        (true, false) => "Bash",
        (false, true) => "WebSearch,WebFetch",
        (false, false) => "",
    };
    let mut args = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--no-session-persistence",
        "--tools",
        native_tools,
        "--disable-slash-commands",
        "--no-chrome",
        "--strict-mcp-config",
        "--permission-mode",
        "dontAsk",
        "--allowedTools",
        if effective.web_search {
            "mcp__pipeline__*,WebSearch,WebFetch"
        } else {
            "mcp__pipeline__*"
        },
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    args.extend([
        "--mcp-config".into(),
        json!({"mcpServers":{"pipeline":{"type":"sdk","name":"pipeline"}}}).to_string(),
    ]);
    if let Some(base) = &effective.preset.base_instructions {
        args.extend(["--system-prompt".into(), base.clone()]);
    }
    let shell = match shell_root {
        Some(root) => {
            super::shell::instructions(root, &effective.capabilities, effective.command_network)
        }
        None => "Shell commands are available only through authorized research profiles.".into(),
    };
    let project = if effective.capabilities.private_folder {
        "Change files in the working folder with Bash."
    } else {
        "Project tools are exposed through the pipeline MCP server. File edits require Edit access and use project_edit."
    };
    args.extend(["--append-system-prompt".into(), format!("{}\n\n{project} {shell} Native filesystem and third-party tools are disabled; browser access, when enabled, uses Pipeline tools. Conversation history is supplied by Pipeline on each turn. Never claim to have run a tool that is unavailable.", effective.developer_instructions)]);
    if let Some(model) = model.filter(|s| !s.is_empty()) {
        args.push(format!("--model={model}"));
    }
    if let Some(effort) = effort.filter(|s| !s.is_empty()) {
        args.push(format!("--effort={effort}"));
    }
    args
}

pub(super) fn validate_model(
    initialization: &Value,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<(), String> {
    let models = initialization["models"]
        .as_array()
        .ok_or("Claude Code did not return its model catalog. Update Claude Code.")?;
    if let Some(model) = model.filter(|s| !s.is_empty()) {
        let selected = models
            .iter()
            .find(|m| m["value"] == model || m["resolvedModel"] == model)
            .ok_or("The selected Claude model is unavailable; choose an offered model.")?;
        if let Some(effort) = effort.filter(|s| !s.is_empty()) {
            if !selected["supportedEffortLevels"]
                .as_array()
                .is_some_and(|levels| levels.iter().any(|v| v == effort))
            {
                return Err(
                    "The selected thinking level is unavailable for this Claude model.".into(),
                );
            }
        }
    } else if effort.is_some_and(|s| !s.is_empty()) {
        return Err("Choose a Claude model before setting its thinking level.".into());
    }
    Ok(())
}

pub(super) fn control_response(id: &Value, response: Value) -> Value {
    json!({"type":"control_response","response":{"subtype":"success","request_id":id,"response":response}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_missing_catalog_and_unsupported_model_effort() {
        let init = json!({"models":[{"value":"sonnet","supportedEffortLevels":["low","high"]}]});
        assert!(validate_model(&init, Some("sonnet"), Some("high")).is_ok());
        assert!(validate_model(&init, Some("opus"), None).is_err());
        assert!(validate_model(&init, Some("sonnet"), Some("max")).is_err());
        assert!(validate_model(&json!({}), None, None).is_err());
        assert!(validate_model(&init, None, Some("high")).is_err());
    }
}

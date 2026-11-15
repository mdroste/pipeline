use super::args::{append_codex_output_schema, codex_web_search_override};
use super::*;

fn pair(args: &[String], key: &str, value: &str) -> bool {
    args.windows(2)
        .any(|window| window[0] == key && window[1] == value)
}

#[test]
fn resumed_calls_pin_the_sandbox_through_config_overrides() {
    let (read_only, writes) = codex_exec_args(
        &["Read"],
        None,
        &LlmOverrides::default(),
        CodexSessionMode::Resume("abc"),
        None,
        "/tmp/final.txt",
    )
    .unwrap();
    assert!(!writes);
    assert_eq!(&read_only[..2], ["exec", "resume"]);
    assert!(pair(&read_only, "-c", "sandbox_mode=\"read-only\""));
    assert!(pair(&read_only, "-c", "approval_policy=\"never\""));
    assert!(!read_only.iter().any(|arg| arg == "--sandbox"));
    assert_eq!(read_only[read_only.len() - 2..], ["abc", "-"]);

    let overrides = LlmOverrides {
        write_dir: Some("/runs/r1/artifacts"),
        ..LlmOverrides::default()
    };
    let (writable, writes) = codex_exec_args(
        &["Read", "Write"],
        None,
        &overrides,
        CodexSessionMode::Resume("abc"),
        None,
        "/tmp/final.txt",
    )
    .unwrap();
    assert!(writes);
    assert!(pair(&writable, "-c", "sandbox_mode=\"workspace-write\""));
    assert!(pair(
        &writable,
        "-c",
        "sandbox_workspace_write.network_access=false"
    ));

    let (fresh, _) = codex_exec_args(
        &["Read"],
        None,
        &LlmOverrides::default(),
        CodexSessionMode::New,
        None,
        "/tmp/final.txt",
    )
    .unwrap();
    assert!(pair(&fresh, "--sandbox", "read-only"));
    assert!(!fresh.iter().any(|arg| arg.starts_with("sandbox_mode=")));
}

#[test]
fn write_mode_keeps_windows_artifact_directory_as_workspace() {
    let cwd = codex_effective_cwd(
        true,
        Some(r"C:\Users\Mike\Documents\Paper"),
        Some(r"C:\Users\Mike\.pipeline\runs\r1\artifacts"),
        Some(r"C:\Users\Mike\AppData\Local\Temp\pipeline_prompt"),
    )
    .unwrap();
    assert_eq!(
        cwd.as_deref(),
        Some("C:/Users/Mike/.pipeline/runs/r1/artifacts")
    );
}

#[test]
fn long_prompt_root_is_workspace_when_no_other_cwd_exists() {
    let cwd = codex_effective_cwd(false, None, None, Some("/private/tmp/pipeline_prompt")).unwrap();
    assert_eq!(cwd.as_deref(), Some("/private/tmp/pipeline_prompt"));
}

#[test]
fn invalid_write_workspace_fails_closed() {
    assert!(codex_effective_cwd(true, None, Some("relative/artifacts"), None).is_err());
}

#[test]
fn resume_errors_are_distinguished_from_provider_failures() {
    assert!(is_session_resume_error("failed to load rollout for thread"));
    assert!(is_session_resume_error("session not found"));
    assert!(!is_session_resume_error("rate limited"));
}

#[test]
fn reads_only_the_terminal_message_file() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "  ## Final report\n\nClean.  \n").unwrap();
    assert_eq!(
        read_last_message_file(file.path()).unwrap().as_deref(),
        Some("## Final report\n\nClean.")
    );
}

#[test]
fn a_missing_terminal_message_file_is_not_an_error() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
        read_last_message_file(&directory.path().join("final.txt")),
        Ok(None)
    );
}

#[test]
fn jsonl_fallback_replaces_commentary_with_the_terminal_message() {
    let events = [
        serde_json::json!({
            "type": "item.completed",
            "item": {
                "type": "agent_message",
                "text": "I’ll read the instructions and use the requested skill."
            }
        }),
        serde_json::json!({
            "type": "item.completed",
            "item": {
                "type": "agent_message",
                "text": "## Final report\n\nClean."
            }
        }),
    ];
    let mut fallback = String::new();
    for event in events {
        update_last_agent_message(&event, &mut fallback).unwrap();
    }
    assert_eq!(fallback, "## Final report\n\nClean.");
    assert!(!fallback.contains("instructions"));
}

#[test]
fn completed_codex_items_keep_stable_tool_categories_and_unknowns() {
    use crate::models::ToolCallKind;

    let cases = [
        ("file_change", None, ToolCallKind::TextFile),
        ("command_execution", None, ToolCallKind::ShellOrOther),
        ("web_search", None, ToolCallKind::Web),
        ("image_generation", None, ToolCallKind::Image),
        (
            "mcp_tool_call",
            Some("ReadDocumentAsset"),
            ToolCallKind::Image,
        ),
        ("future_tool_call", None, ToolCallKind::Unknown),
    ];
    for (item_type, name, expected) in cases {
        let mut item = serde_json::json!({ "type": item_type });
        if let Some(name) = name {
            item["tool"] = serde_json::Value::String(name.to_string());
        }
        let event = serde_json::json!({
            "type": "item.completed",
            "item": item
        });
        assert_eq!(codex_completed_tool_kind(&event), Some(expected));
    }

    let message = serde_json::json!({
        "type": "item.completed",
        "item": { "type": "agent_message", "text": "done" }
    });
    assert_eq!(codex_completed_tool_kind(&message), None);
}

#[test]
fn codex_web_search_is_explicitly_scoped_per_turn() {
    assert_eq!(
        codex_web_search_override(&["Read", "WebSearch"]),
        "web_search=\"live\""
    );
    assert_eq!(
        codex_web_search_override(&["Read"]),
        "web_search=\"disabled\""
    );
    assert_eq!(codex_web_search_override(&[]), "web_search=\"disabled\"");
}

#[test]
fn codex_cli_receives_native_output_schema_file() {
    let mut args = vec!["exec".to_string()];
    append_codex_output_schema(&mut args, Some("/tmp/output-schema.json"));
    assert_eq!(
        args,
        vec!["exec", "--output-schema", "/tmp/output-schema.json"]
    );
}

#[test]
fn validates_a_scoped_workspace_write_fork() {
    let result = serde_json::json!({
        "cwd": "/tmp/pipeline/run/steps/referee",
        "model": "gpt-5.6-sol",
        "runtimeWorkspaceRoots": ["/tmp/pipeline/run/steps/referee"],
        "sandbox": {
            "type": "workspaceWrite",
            "excludeSlashTmp": true,
            "excludeTmpdirEnvVar": true,
            "networkAccess": false,
            "writableRoots": []
        },
        "thread": { "id": "child-thread" }
    });
    let spec = CodexForkSpec {
        cwd: "/tmp/pipeline/run/steps/referee",
        sandbox: "workspace-write",
        model: Some("gpt-5.6-sol"),
    };

    assert_eq!(validate_codex_fork(&result, &spec).unwrap(), "child-thread");
}

#[test]
fn rejects_a_fork_that_inherits_a_broader_workspace() {
    let result = serde_json::json!({
        "cwd": "/tmp/pipeline/run/steps/referee",
        "model": "gpt-5.6-sol",
        "runtimeWorkspaceRoots": ["/tmp/pipeline/run"],
        "sandbox": {
            "type": "workspaceWrite",
            "excludeSlashTmp": true,
            "excludeTmpdirEnvVar": true,
            "networkAccess": false,
            "writableRoots": []
        },
        "thread": { "id": "child-thread" }
    });
    let spec = CodexForkSpec {
        cwd: "/tmp/pipeline/run/steps/referee",
        sandbox: "workspace-write",
        model: Some("gpt-5.6-sol"),
    };

    assert!(validate_codex_fork(&result, &spec)
        .unwrap_err()
        .contains("unexpected runtime workspace roots"));
}

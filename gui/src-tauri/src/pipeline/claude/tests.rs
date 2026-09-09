use super::*;

#[test]
fn read_only_calls_deny_every_write_tool() {
    // A read-only call (no producer write root) runs with acceptEdits and a
    // cwd that may be the user's real source folder. Edit/Write/NotebookEdit
    // must be denied outright so an injected instruction cannot write there.
    let denies = cli_disallowed_tools(&["Read"], &["/Users/x/Paper".to_string()], false);
    assert!(denies.contains(&"Edit".to_string()));
    assert!(denies.contains(&"Write".to_string()));
    assert!(denies.contains(&"NotebookEdit".to_string()));
    assert!(denies.contains(&"WebSearch".to_string()));
    assert!(denies.contains(&"Bash".to_string()));
    assert!(denies.contains(&"WebFetch".to_string()));
    assert!(denies.contains(&"mcp__*".to_string()));
    assert!(denies.contains(&"Task".to_string()));
}

#[test]
fn write_enabled_calls_scope_edits_to_read_roots_only() {
    // With a write root, writes are allowed in the cwd but must not leak
    // into selected read roots; Edit is not denied wholesale.
    let denies = cli_disallowed_tools(
        &["Read"],
        &["/Users/x/src".to_string(), "/Users/x/data".to_string()],
        true,
    );
    assert!(denies
        .iter()
        .any(|d| d.starts_with("Edit(") && d.contains("/Users/x/src")));
    assert!(denies
        .iter()
        .any(|d| d.starts_with("Edit(") && d.contains("/Users/x/data")));
    assert!(!denies.iter().any(|d| d == "Edit"));
    assert!(!denies.iter().any(|d| d == "Write"));
}

#[test]
fn allowed_websearch_is_not_denied() {
    let denies = cli_disallowed_tools(&["Read", "WebSearch"], &[], false);
    assert!(!denies.contains(&"WebSearch".to_string()));
    // Read-only still denies writes even when no read roots are present.
    assert!(denies.contains(&"Write".to_string()));
}

#[test]
fn ambient_read_and_discovery_tools_are_denied() {
    let denies = cli_disallowed_tools(&["Read"], &[], false);
    for tool in ["Glob", "Grep", "Agent", "Skill", "AskUserQuestion"] {
        assert!(
            denies.contains(&tool.to_string()),
            "missing deny for {tool}"
        );
    }
}

#[test]
fn webview_request_previews_are_utf8_safe_and_bounded() {
    let short = event_text_preview("short request");
    assert_eq!(short.text, "short request");
    assert!(!short.truncated);

    let large = "é".repeat(MAX_EVENT_TEXT_PREVIEW_BYTES);
    let preview = event_text_preview(&large);
    assert!(preview.truncated);
    assert!(preview.text.len() <= MAX_EVENT_TEXT_PREVIEW_BYTES);
    assert!(preview.text.is_char_boundary(preview.text.len()));
    assert!(large.starts_with(&preview.text));
}

#[test]
fn request_provider_labels_do_not_repeat_the_transport() {
    assert_eq!(request_provider_label("claude", "cli"), "Claude Code");
    assert_eq!(request_provider_label("codex", "cli"), "Codex");
    assert_eq!(request_provider_label("claude", "api"), "Anthropic");
    assert_eq!(request_provider_label("codex", "api"), "OpenAI");
    assert_eq!(request_provider_label("antigravity", "api"), "Google");
}

#[test]
fn request_effort_reports_what_each_transport_sends() {
    let settings = crate::settings::Settings {
        claude_effort: "high".to_string(),
        codex_effort: "medium".to_string(),
        antigravity_effort: "low".to_string(),
        ..Default::default()
    };
    let overrides = LlmOverrides::default();

    assert_eq!(
        request_effort("claude", "api", "claude-sonnet-4-6", &settings, &overrides),
        "Provider default"
    );
    let claude_high = LlmOverrides {
        effort: Some("high"),
        ..Default::default()
    };
    assert_eq!(
        request_effort("claude", "api", "claude-haiku-4-5", &settings, &claude_high),
        "Not sent (unsupported by model)"
    );
    let codex_medium = LlmOverrides {
        effort: Some("medium"),
        ..Default::default()
    };
    assert_eq!(
        request_effort("codex", "cli", "gpt-5.6", &settings, &codex_medium),
        "medium"
    );
    assert_eq!(
        request_effort(
            "antigravity",
            "api",
            "gemini-3.6-flash",
            &settings,
            &overrides
        ),
        "Not configurable"
    );
}

#[test]
#[cfg(unix)]
fn configured_child_owns_a_dedicated_process_group() {
    let mut command = std::process::Command::new("sleep");
    super::configure_silent_command(&mut command);
    let mut child = command.arg("5").spawn().unwrap();
    let pid = child.id();
    let process_group = unsafe { libc::getpgid(pid as i32) };
    assert_eq!(process_group, pid as i32);
    crate::commands::kill_process(pid);
    let _ = child.wait();
}

#[test]
fn unwraps_result_envelope_and_sums_input_tokens() {
    let raw = r#"{"type":"result","subtype":"success","result":"hello world","num_turns":4,
        "usage":{"input_tokens":100,"output_tokens":20,"cache_read_input_tokens":5,
        "cache_creation_input_tokens":3,"server_tool_use":{"web_search_requests":2,
        "code_execution_requests":1,"future_tool_requests":3}}}"#;
    let (text, usage) = parse_claude_result(raw).unwrap();
    assert_eq!(text, "hello world");
    assert_eq!(
        usage,
        Some(crate::pipeline::logging::CallUsage {
            input_tokens: 108,
            output_tokens: 20,
            cached_input_tokens: 5,
            cache_write_input_tokens: 3,
            model_round_trips: 4,
            tool_calls: crate::models::ToolCallCounts {
                web: 2,
                shell_or_other: 1,
                unknown: 3,
                ..Default::default()
            },
            ..Default::default()
        })
    );
}

#[test]
fn session_capability_errors_are_distinguished_from_model_failures() {
    assert!(is_cli_session_capability_error(
        "error: unknown option '--fork-session'"
    ));
    assert!(is_cli_session_capability_error(
        "failed to resume: session not found"
    ));
    // The exact shape the CLI emits when --resume cannot see the warmed
    // session (observed killing all parallel steps of a run).
    assert!(is_cli_session_capability_error(
        "Claude call failed (exit 1): No conversation found with session ID: \
         9b2ab2be-13d2-435c-8ea4-4710684fee5c"
    ));
    assert!(!is_cli_session_capability_error(
        "service overloaded; try again"
    ));
}

#[test]
fn fork_failures_fall_back_except_cancellation_timeout_and_usage_exhaustion() {
    assert!(fork_failure_uses_fallback(
        "Claude call failed (exit 1): No conversation found with session ID: abc"
    ));
    assert!(fork_failure_uses_fallback(
        "Claude call failed (exit 1): service overloaded"
    ));
    assert!(!fork_failure_uses_fallback("Pipeline cancelled"));
    assert!(!fork_failure_uses_fallback(
        "Pass 'technical/claude' cancelled"
    ));
    assert!(!fork_failure_uses_fallback(
        "claude call timed out after 900s"
    ));
    assert!(!fork_failure_uses_fallback(
        "Claude call failed: You've hit your limit · resets 3am"
    ));
    assert!(fork_failure_uses_fallback(
        "Claude call failed: subscription cancelled"
    ));
}

#[test]
fn envelope_without_usage_yields_result_and_no_tokens() {
    let (text, usage) = parse_claude_result(r#"{"type":"result","result":"ok"}"#).unwrap();
    assert_eq!(text, "ok");
    assert_eq!(usage, None);
}

#[test]
fn structured_result_envelope_returns_canonical_json_text() {
    let (text, usage) =
        parse_claude_result(r#"{"type":"result","result":"","structured_output":{"ok":true}}"#)
            .unwrap();
    assert_eq!(text, r#"{"ok":true}"#);
    assert_eq!(usage, None);
}

#[test]
fn claude_cli_receives_native_json_schema_argument() {
    let schema = serde_json::json!({
        "type": "object",
        "required": ["ok"],
        "properties": {"ok": {"type": "boolean"}}
    });
    let mut args = Vec::new();
    append_claude_output_schema(&mut args, Some(&schema)).unwrap();
    assert_eq!(args[0], "--json-schema");
    let sent: serde_json::Value = serde_json::from_str(&args[1]).unwrap();
    assert_eq!(sent["required"], serde_json::json!(["ok"]));
}

#[test]
fn error_envelope_preserves_subscription_limit_detail() {
    let error = parse_claude_result(
        r#"{"type":"result","subtype":"error_during_execution","is_error":true,
            "result":"You've hit your limit · resets 3am (America/Los_Angeles)"}"#,
    )
    .unwrap_err();
    assert!(error.contains("You've hit your limit"));
    assert!(super::super::provider_error::is_usage_limit_error(&error));
}

#[test]
fn envelope_preserves_reported_turns_without_token_usage() {
    let (_, usage) =
        parse_claude_result(r#"{"type":"result","result":"ok","num_turns":3}"#).unwrap();
    assert_eq!(
        usage,
        Some(crate::pipeline::logging::CallUsage {
            model_round_trips: 3,
            ..Default::default()
        })
    );
}

#[test]
fn plain_text_is_rejected_as_a_result_envelope() {
    assert!(parse_claude_result("  just some plain text  ").is_err());
}

#[test]
fn model_json_output_is_rejected_without_the_cli_envelope() {
    let model = r#"{"metadata":{"title":"X"},"result":"not an envelope"}"#;
    assert!(parse_claude_result(model).is_err());
}

#[test]
fn error_result_envelopes_are_rejected() {
    assert!(parse_claude_result(
        r#"{"type":"result","subtype":"error_max_turns","result":"partial"}"#
    )
    .is_err());
    assert!(
        parse_claude_result(r#"{"type":"result","is_error":true,"result":"partial"}"#).is_err()
    );
}

#[test]
fn claude_auth_failures_get_one_actionable_console_hint() {
    for error in [
        "Authentication required",
        "Not logged in. Please run /login.",
        "HTTP 401 Unauthorized",
    ] {
        assert_eq!(
            claude_failure_hint(error, &[]).as_deref(),
            Some(CLAUDE_LOGIN_HINT)
        );
    }
    assert_eq!(
        claude_failure_hint("", &["authentication failed".to_string()]).as_deref(),
        Some(CLAUDE_LOGIN_HINT)
    );
}

#[test]
fn ordinary_auth_substrings_do_not_trigger_the_login_hint() {
    let hint = claude_failure_hint("The authoring tool rejected the file", &[]).unwrap();
    assert_eq!(hint, "The authoring tool rejected the file");
}

#[test]
fn folder_orientation_uses_folder_as_cwd_without_broadening_roots() {
    let plan = plan_cli_workspace(
        Some("/Users/Mike/Documents/Paper Folder"),
        &["/Users/Mike/Documents/Paper Folder"],
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        plan.cwd.as_deref(),
        Some("/Users/Mike/Documents/Paper Folder")
    );
    assert!(plan.read_dirs.is_empty());
}

#[test]
fn windows_write_workspace_keeps_all_read_roots_and_artifacts_as_cwd() {
    let plan = plan_cli_workspace(
        Some(r"C:\Users\Mike\Documents\Paper"),
        &[
            r"C:\Users\Mike\Documents\Paper",
            r"C:\Users\Mike\AppData\Local\Temp\pipeline_run",
            r"C:\Users\Mike\AppData\Local\Temp\pipeline_run\named",
            r"D:\Shared Inputs\Rubric",
        ],
        Some(r"C:\Users\Mike\AppData\Local\Temp\pipeline_prompt"),
        Some(r"C:\Users\Mike\.pipeline\runs\r1\artifacts"),
    )
    .unwrap();

    assert_eq!(
        plan.cwd.as_deref(),
        Some("C:/Users/Mike/.pipeline/runs/r1/artifacts")
    );
    assert_eq!(
        plan.read_dirs,
        vec![
            "C:/Users/Mike/AppData/Local/Temp/pipeline_prompt",
            "C:/Users/Mike/AppData/Local/Temp/pipeline_run",
            "C:/Users/Mike/Documents/Paper",
            "D:/Shared Inputs/Rubric",
        ]
    );
}

#[test]
fn invalid_write_workspace_fails_closed() {
    let error = plan_cli_workspace(Some("/safe/source"), &[], None, Some("relative/artifacts"))
        .unwrap_err();
    assert!(error.contains("working directory must be absolute"));
}

#[test]
fn long_prompt_gets_its_own_read_root() {
    let prompt = "x".repeat(MAX_DIRECT_PROMPT_LENGTH + 1);
    let prepared = prepare_cli_prompt(&prompt).unwrap();
    let path = prepared.path.as_deref().unwrap();
    let root = prepared.read_root.as_deref().unwrap();
    assert_eq!(cli_parent_dir(path).as_deref(), Some(root));
    assert_eq!(std::fs::read_to_string(path).unwrap(), prompt);

    let isolated = plan_cli_workspace(None, &[], Some(root), None).unwrap();
    assert_eq!(isolated.cwd.as_deref(), Some(root));
    assert!(isolated.read_dirs.is_empty());

    let alongside_source =
        plan_cli_workspace(Some("/Users/Mike/Paper"), &[], Some(root), None).unwrap();
    assert_eq!(alongside_source.read_dirs, vec![root.to_string()]);
    assert!(!alongside_source
        .read_dirs
        .contains(&std::env::temp_dir().to_string_lossy().replace('\\', "/")));
}

#[test]
fn cli_paths_normalize_mac_windows_and_permission_rules() {
    assert_eq!(
        normalize_cli_root("/Users/Mike/Documents/../Paper").as_deref(),
        Some("/Users/Mike/Paper")
    );
    assert_eq!(
        normalize_cli_root(r"C:\Users\Mike\Documents\..\Paper\").as_deref(),
        Some("C:/Users/Mike/Paper")
    );
    assert_eq!(
        absolute_rule_path("/Users/Mike/Paper"),
        "//Users/Mike/Paper"
    );
    assert_eq!(
        absolute_rule_path("C:/Users/Mike/Paper"),
        "//C:/Users/Mike/Paper"
    );
}

#[test]
fn live_artifact_monitor_rejects_oversized_files() {
    let dir = tempfile::tempdir().unwrap();
    let file = std::fs::File::create(dir.path().join("too-large.bin")).unwrap();
    file.set_len(MAX_LIVE_ARTIFACT_FILE_BYTES + 1).unwrap();
    assert!(check_live_artifact_quota(dir.path())
        .unwrap_err()
        .contains("quota exceeded"));
}

#[cfg(unix)]
#[test]
fn live_artifact_monitor_rejects_special_files() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt as _;

    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("fifo");
    let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
    assert!(check_live_artifact_quota(dir.path())
        .unwrap_err()
        .contains("non-regular"));
}

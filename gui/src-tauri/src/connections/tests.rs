use super::*;

#[test]
fn review_modes_and_conversation_choices_resolve_independently() {
    let mut settings = Settings {
        openai_api_key: "test-openai".into(),
        anthropic_api_key: "test-anthropic".into(),
        ..Default::default()
    };
    for (family, api, subscription) in [
        ("codex", ConnectionId::OpenAiApi, ConnectionId::Chatgpt),
        (
            "claude",
            ConnectionId::AnthropicApi,
            ConnectionId::ClaudeCode,
        ),
    ] {
        assert_eq!(
            ConnectionId::for_review(family, &settings).unwrap(),
            subscription
        );
        settings.codex_access_mode = "api".into();
        settings.claude_access_mode = "api".into();
        assert_eq!(ConnectionId::for_review(family, &settings).unwrap(), api);
        assert_eq!(
            ConnectionId::from_runtime(api.api().unwrap().runtime()).unwrap(),
            api
        );
        settings.codex_access_mode = "subscription".into();
        settings.claude_access_mode = "subscription".into();
    }
    assert_eq!(
        ConnectionId::for_review("local", &settings).unwrap().api(),
        Some(ApiProvider::Local)
    );
    assert_eq!(
        ConnectionId::for_review("antigravity", &settings).unwrap(),
        ConnectionId::GeminiApi
    );
    assert!(ConnectionId::from_runtime("unknown").is_err());
    assert!(ConnectionId::for_review("unknown", &settings).is_err());
}

#[test]
fn legacy_missing_modes_keep_their_meaning() {
    let mut settings = Settings::default();
    settings.codex_access_mode.clear();
    assert_eq!(
        ConnectionId::for_review("codex", &settings).unwrap(),
        ConnectionId::Chatgpt
    );
    settings.openai_api_key = "test".into();
    assert_eq!(
        ConnectionId::for_review("codex", &settings).unwrap(),
        ConnectionId::OpenAiApi
    );
    for value in [
        "codex",
        "claude",
        "claude-code-api",
        "openai-api",
        "anthropic-api",
        "google-api",
        "local",
    ] {
        let connection = ConnectionId::from_runtime(value).unwrap();
        assert_eq!(serde_json::to_value(connection).unwrap(), value);
    }
}

#[test]
fn shared_registry_matches_native_selection() {
    let descriptions = descriptions();
    assert_eq!(descriptions.len(), 7);
    let mut ids = std::collections::HashSet::new();
    for description in descriptions {
        assert!(ids.insert(serde_json::to_string(&description.id).unwrap()));
        assert_eq!(description.family, description.id.catalog_provider());
        assert_eq!(description.access == "api", description.id.api().is_some());
        assert!(!description.label.is_empty());
    }
}

#[test]
fn shared_connection_and_protocol_modules_do_not_import_product_runtimes() {
    fn inspect(path: &std::path::Path) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                inspect(&path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && path.file_name().unwrap() != "tests.rs"
            {
                let source = std::fs::read_to_string(&path).unwrap();
                for forbidden in [
                    "crate::pipeline",
                    "crate::workbench",
                    "crate::commands",
                    "crate::run_scope",
                ] {
                    assert!(
                        !source.contains(forbidden),
                        "{} imports {forbidden}",
                        path.display()
                    );
                }
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for module in ["connections", "agent_runtime/api", "model_catalog"] {
        inspect(&root.join(module));
    }
}

#[test]
fn claude_code_api_is_explicit_native_and_separate_from_saved_subscription_chats() {
    let mut settings = Settings {
        anthropic_api_key: "saved-direct-api-key".into(),
        ..Default::default()
    };
    assert_eq!(
        ConnectionId::for_review("claude", &settings).unwrap(),
        ConnectionId::ClaudeCode
    );
    settings.claude_access_mode = "code_api".into();
    let selected = ConnectionId::for_review("claude", &settings).unwrap();
    assert_eq!(selected, ConnectionId::ClaudeCodeApi);
    assert_eq!(selected.transport(&settings), Transport::ClaudeCode);
    assert_eq!(selected.api(), None);
    assert_eq!(
        ConnectionId::from_runtime("claude").unwrap(),
        ConnectionId::ClaudeCode
    );
    assert_eq!(
        ConnectionId::from_runtime("claude-code-api").unwrap(),
        selected
    );
    assert!(settings.validate().is_ok());
}

#[test]
fn subscription_exhaustion_never_chooses_an_api_billed_backup() {
    for target in [
        ConnectionId::ClaudeCodeApi,
        ConnectionId::AnthropicApi,
        ConnectionId::OpenAiApi,
        ConnectionId::GeminiApi,
        ConnectionId::Local,
    ] {
        assert!(!ConnectionId::ClaudeCode.permits_usage_fallback(target));
    }
    for target in [ConnectionId::Chatgpt, ConnectionId::ClaudeCode] {
        assert!(ConnectionId::ClaudeCode.permits_usage_fallback(target));
    }
    assert!(ConnectionId::ClaudeCodeApi.permits_usage_fallback(ConnectionId::AnthropicApi));
}

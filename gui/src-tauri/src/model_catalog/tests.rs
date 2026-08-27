use super::*;

#[test]
fn bundled_policy_is_valid_and_versioned() {
    let policy = bundled_policy();
    assert_eq!(policy.schema_version, 1);
    assert!(!policy.updated_at.is_empty());
}

#[test]
fn pricing_comes_from_policy() {
    assert_eq!(price_for_model("claude-sonnet-4-6"), Some((3.0, 15.0)));
    assert_eq!(price_for_model("gpt-4.1-mini"), Some((0.4, 1.6)));
    assert_eq!(price_for_model("gpt-5.6-terra"), Some((2.5, 15.0)));
}

#[test]
fn openai_chat_filter_excludes_specialty_and_responses_only_models() {
    for model in ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-4.1", "o3", "o4-mini"] {
        assert!(is_openai_chat_model(model), "{model}");
    }
    for model in [
        "text-embedding-3-large",
        "gpt-image-2",
        "gpt-realtime-2",
        "gpt-5.3-codex",
        "gpt-4o-search-preview",
        "omni-moderation-latest",
        "ft:gpt-4.1:org:custom",
    ] {
        assert!(!is_openai_chat_model(model), "{model}");
    }
}

#[test]
fn cache_reader_is_bounded() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp.path(), b"123456789").unwrap();
    assert_eq!(read_file_limited(temp.path(), 9).unwrap(), b"123456789");
    assert!(read_file_limited(temp.path(), 8).is_none());
}

#[test]
fn legacy_or_different_policy_caches_are_rejected() {
    let settings = Settings {
        openai_api_key: "account-one-secret".to_string(),
        ..Default::default()
    };
    let legacy = CacheEnvelope {
        saved_at: now_epoch(),
        ..Default::default()
    };
    assert!(!cache_matches_context(&legacy, "codex", "api", &settings));

    let current = CacheEnvelope {
        schema_version: CACHE_SCHEMA_VERSION,
        policy_fingerprint: bundled_policy_fingerprint(),
        credential_fingerprint: catalog_credential_fingerprint("codex", "api", &settings),
        saved_at: now_epoch(),
        ..Default::default()
    };
    assert!(cache_matches_context(&current, "codex", "api", &settings));

    let other_account = Settings {
        openai_api_key: "account-two-secret".to_string(),
        ..Default::default()
    };
    assert!(!cache_matches_context(
        &current,
        "codex",
        "api",
        &other_account
    ));
    assert!(!serde_json::to_string(&current)
        .unwrap()
        .contains("account-one-secret"));
}

#[test]
fn discovery_output_is_bounded_but_fully_drained() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (mut writer, reader) = tokio::io::duplex(8);
            let writing = tokio::spawn(async move {
                writer.write_all(b"123456789").await.unwrap();
            });
            let (bytes, overflowed) = read_discovery_output(reader, 4).await.unwrap();
            writing.await.unwrap();
            assert_eq!(bytes, b"1234");
            assert!(overflowed);
        });
}

fn test_catalog(source: &str) -> ModelCatalog {
    ModelCatalog {
        provider: "test".into(),
        source: source.into(),
        ..Default::default()
    }
}

fn test_discovery_key(provider: &str) -> DiscoveryKey {
    DiscoveryKey {
        provider: provider.into(),
        transport: "api".into(),
        endpoint_fingerprint: "endpoint-fingerprint".into(),
        credential_fingerprint: "credential-fingerprint".into(),
        catalog_revision: "catalog-revision".into(),
    }
}

#[test]
fn identical_discovery_keys_share_one_operation() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let calls = Arc::new(AtomicUsize::new(0));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let release = Arc::new(tokio::sync::Notify::new());
        let first_calls = Arc::clone(&calls);
        let first_release = Arc::clone(&release);
        let first = tokio::spawn(single_flight_discovery(
            test_discovery_key("same-key"),
            move || async move {
                first_calls.fetch_add(1, Ordering::SeqCst);
                let _ = started_tx.send(());
                first_release.notified().await;
                Ok(test_catalog("shared"))
            },
        ));
        started_rx.await.unwrap();
        let second_calls = Arc::clone(&calls);
        let second = tokio::spawn(single_flight_discovery(
            test_discovery_key("same-key"),
            move || async move {
                second_calls.fetch_add(1, Ordering::SeqCst);
                Ok(test_catalog("duplicate"))
            },
        ));
        tokio::time::sleep(Duration::from_millis(10)).await;
        release.notify_one();
        let first = first.await.unwrap().unwrap();
        let second = second.await.unwrap().unwrap();
        assert_eq!(first.source, "shared");
        assert_eq!(second.source, "shared");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn different_discovery_keys_run_in_parallel() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let gate = Arc::new(tokio::sync::Barrier::new(2));
        let first_gate = Arc::clone(&gate);
        let first = tokio::spawn(single_flight_discovery(
            test_discovery_key("provider-a"),
            move || async move {
                first_gate.wait().await;
                Ok(test_catalog("a"))
            },
        ));
        let second_gate = Arc::clone(&gate);
        let second = tokio::spawn(single_flight_discovery(
            test_discovery_key("provider-b"),
            move || async move {
                second_gate.wait().await;
                Ok(test_catalog("b"))
            },
        ));
        let first = first.await.unwrap().unwrap();
        let second = second.await.unwrap().unwrap();
        assert_eq!(first.source, "a");
        assert_eq!(second.source, "b");
    });
}

#[test]
fn discovery_key_is_context_bound_without_retaining_secrets() {
    let first = Settings {
        local_base_url: "HTTP://LOCALHOST:11434/v1/".into(),
        local_api_key: "one-very-secret-key".into(),
        ..Default::default()
    };
    let equivalent = Settings {
        local_base_url: "http://localhost:11434/v1".into(),
        local_api_key: first.local_api_key.clone(),
        ..Default::default()
    };
    let other_endpoint = Settings {
        local_base_url: "http://localhost:22445/v1".into(),
        local_api_key: first.local_api_key.clone(),
        ..Default::default()
    };
    let other_credential = Settings {
        local_base_url: first.local_base_url.clone(),
        local_api_key: "another-very-secret-key".into(),
        ..Default::default()
    };
    let first_key = discovery_key("local", "api", &first);
    assert_eq!(first_key, discovery_key("local", "api", &equivalent));
    assert_ne!(first_key, discovery_key("local", "api", &other_endpoint));
    assert_ne!(first_key, discovery_key("local", "api", &other_credential));
    let mut newer_catalog = first_key.clone();
    newer_catalog.catalog_revision.push_str(":next");
    assert_ne!(first_key, newer_catalog);
    let debug = format!("{first_key:?}");
    assert!(!debug.contains(&first.local_api_key));
    assert!(!debug.contains(&first.local_base_url));
}

#[test]
fn antigravity_models_parse_json_document_shapes() {
    // Array wrappers and both snake/camel field spellings must resolve.
    let mut catalog = base_catalog("antigravity", "cli", "installed_cli");
    populate_antigravity_models(
        &mut catalog,
        r#"{"models":[
            {"id":"gemini-3.5-flash","displayName":"Gemini 3.5 Flash","isDefault":true},
            {"modelId":"gemini-3.1-pro","name":"Gemini 3.1 Pro"},
            {"model":"claude-sonnet"}
        ]}"#,
    )
    .unwrap();
    assert_eq!(catalog.models.len(), 3);
    assert_eq!(catalog.models[0].id, "gemini-3.5-flash");
    assert_eq!(catalog.models[0].display_name, "Gemini 3.5 Flash");
    assert!(catalog.models[0].is_default);
    assert_eq!(catalog.default_model.as_deref(), Some("gemini-3.5-flash"));
    assert_eq!(catalog.models[1].id, "gemini-3.1-pro");
    assert_eq!(catalog.models[2].id, "claude-sonnet");
}

#[test]
fn antigravity_models_parse_text_listing_with_defaults_and_spaced_ids() {
    // The 1.1.12 subcommand is text-only: banner lines are skipped, marker
    // and "(default)" decorations are stripped, ids may contain spaces, and
    // tab-separated records carry a display name.
    let mut catalog = base_catalog("antigravity", "cli", "installed_cli");
    populate_antigravity_models(
        &mut catalog,
        "Fetching available models...\nAvailable models:\n* Gemini 3.5 Flash (default)\n- Gemini 3.1 Pro\ngemini-3.5-flash-lite\tGemini 3.5 Flash-Lite\n",
    )
    .unwrap();
    assert_eq!(catalog.models.len(), 3);
    assert_eq!(catalog.models[0].id, "Gemini 3.5 Flash");
    assert!(catalog.models[0].is_default);
    assert_eq!(catalog.default_model.as_deref(), Some("Gemini 3.5 Flash"));
    assert_eq!(catalog.models[1].id, "Gemini 3.1 Pro");
    assert_eq!(catalog.models[2].id, "gemini-3.5-flash-lite");
    assert_eq!(catalog.models[2].display_name, "Gemini 3.5 Flash-Lite");
}

#[test]
fn antigravity_models_empty_output_is_an_error_not_an_empty_catalog() {
    // The live list is authoritative for pinned-ID validation, so a parse
    // that yields nothing must fail discovery instead of emptying the picker.
    let mut catalog = base_catalog("antigravity", "cli", "installed_cli");
    assert!(populate_antigravity_models(&mut catalog, "Fetching available models...\n").is_err());
    assert!(populate_antigravity_models(&mut catalog, "").is_err());
}

#[test]
fn retired_gemini_provider_id_is_rejected_by_both_transports() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let settings = Settings::default();
    let api = runtime.block_on(api_catalog("gemini", &settings));
    assert!(api.unwrap_err().contains("Unknown model provider"));
    let cli = runtime.block_on(cli_catalog("gemini"));
    assert!(cli.unwrap_err().contains("Unknown CLI provider"));
}

#[test]
fn antigravity_api_credentials_bind_to_the_google_key() {
    let account_one = Settings {
        google_api_key: "key-one".into(),
        ..Default::default()
    };
    let account_two = Settings {
        google_api_key: "key-two".into(),
        ..Default::default()
    };
    assert_ne!(
        catalog_credential_fingerprint("antigravity", "api", &account_one),
        catalog_credential_fingerprint("antigravity", "api", &account_two)
    );
    // CLI accounts have no stable fingerprint input, so the key is not bound.
    assert_eq!(
        catalog_credential_fingerprint("antigravity", "cli", &account_one),
        catalog_credential_fingerprint("antigravity", "cli", &account_two)
    );
}

#[test]
fn discovery_errors_name_the_program() {
    let error = serde_json::json!({ "code": -32000, "message": "Connection timed out" });
    assert_eq!(
        provider_discovery_error("codex", &error),
        format!("codex discovery returned {error}")
    );
}

#[test]
fn claude_sdk_models_are_account_aware_and_skip_default_sentinel() {
    let initialization = serde_json::json!({
        "models": [
            {
                "value": "default",
                "resolvedModel": "claude-opus-4-8[1m]",
                "displayName": "Default (recommended)",
                "description": "Account default"
            },
            {
                "value": "opus[1m]",
                "resolvedModel": "claude-opus-4-8[1m]",
                "displayName": "Opus",
                "description": "Everyday complex tasks",
                "supportedEffortLevels": ["low", "medium", "high", "xhigh", "max"]
            },
            {
                "value": "claude-fable-5[1m]",
                "resolvedModel": "claude-fable-5",
                "displayName": "Fable",
                "description": "Hardest and longest-running tasks",
                "supportedEffortLevels": ["low", "medium", "high", "xhigh", "max"]
            }
        ]
    });
    let mut catalog = base_catalog("claude", "cli", "installed_cli");
    populate_claude_models(&mut catalog, &initialization).unwrap();

    assert_eq!(
        catalog.default_model.as_deref(),
        Some("claude-opus-4-8[1m]")
    );
    assert_eq!(catalog.models.len(), 2);
    assert_eq!(catalog.models[0].id, "opus[1m]");
    assert!(catalog.models[0].is_default);
    assert_eq!(catalog.models[1].id, "claude-fable-5[1m]");
    assert_eq!(catalog.models[1].display_name, "Fable");
    assert_eq!(
        catalog.models[1].supported_efforts,
        ["low", "medium", "high", "xhigh", "max"]
    );
}

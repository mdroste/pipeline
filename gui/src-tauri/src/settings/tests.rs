use super::*;

#[test]
fn revision_reconciliation_is_opt_in_for_new_and_existing_settings() {
    assert!(!Settings::default().auto_revision_reconciliation);
    let legacy: Settings = serde_json::from_str("{}").unwrap();
    assert!(!legacy.auto_revision_reconciliation);
}

#[test]
fn usage_limit_fallback_is_opt_in_and_transport_specific() {
    let defaults = Settings::default();
    assert_eq!(defaults.usage_limit_fallback_agent(), None);

    let mut configured = Settings {
        usage_limit_fallback_agent: "codex".to_string(),
        ..Default::default()
    };
    configured.usage_limit_fallback_model_overrides.insert(
        "codex:cli".to_string(),
        ModelSelection::Pinned {
            model: "fallback-model".to_string(),
        },
    );
    assert_eq!(configured.usage_limit_fallback_agent(), Some("codex"));
    assert_eq!(
        configured.usage_limit_fallback_model_selection("codex"),
        Some(ModelSelection::Pinned {
            model: "fallback-model".to_string(),
        })
    );
    assert!(configured.validate().is_ok());

    configured.usage_limit_fallback_agent = "unknown".to_string();
    assert!(configured.validate().is_err());
}

#[test]
fn worker_concurrency_defaults_to_sixteen_and_allows_up_to_twenty() {
    let defaults = Settings::default();
    assert_eq!(defaults.max_workers, 16);

    let legacy: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(legacy.max_workers, 16);

    let at_limit = Settings {
        max_workers: 20,
        ..Default::default()
    };
    assert!(at_limit.validate().is_ok());

    let above_limit = Settings {
        max_workers: 21,
        ..Default::default()
    };
    assert_eq!(
        above_limit.validate().unwrap_err(),
        "Maximum workers must be between 1 and 20"
    );
}

#[test]
fn retired_fast_paddle_selection_migrates_to_full_parser() {
    let legacy: Settings = serde_json::from_str(r#"{"pdf_extractor":"paddleocr-vl"}"#).unwrap();
    assert_eq!(legacy.pdf_extractor, "paddleocr-vl-full");
    assert!(legacy.validate().is_ok());
}

#[test]
fn automatic_pdf_extractor_prefers_an_installed_full_parser() {
    assert_eq!(Settings::default().pdf_extractor, "auto");
    let without_saved_preference: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(without_saved_preference.pdf_extractor, "auto");

    assert_eq!(
        resolve_pdf_extractor_for_paddle_availability("auto", true),
        "paddleocr-vl-full"
    );
    assert_eq!(
        resolve_pdf_extractor_for_paddle_availability("auto", false),
        "llm"
    );
    assert_eq!(
        resolve_pdf_extractor_for_paddle_availability("llm", true),
        "llm"
    );
    assert_eq!(
        resolve_pdf_extractor_for_paddle_availability("pdftotext", true),
        "pdftotext"
    );
}

#[test]
fn paddle_tuning_defaults_are_automatic_and_bounded() {
    let defaults = Settings::default();
    assert_eq!(defaults.paddle_page_concurrency, 0);
    assert_eq!(defaults.paddle_mtmd_batch_tokens, 0);
    assert_eq!(defaults.paddle_flash_attention, "auto");
    assert_eq!(defaults.paddle_max_output_tokens, 4096);
    assert_eq!(defaults.paddle_page_retries, 1);
    assert!(defaults.paddle_full_layout_detection);
    assert_eq!(defaults.paddle_full_layout_threshold, 0.5);
    assert!(defaults.paddle_full_layout_nms);
    assert_eq!(defaults.paddle_full_layout_merge_bboxes_mode, "large");
    assert!(defaults.paddle_full_merge_layout_blocks);
    assert!(defaults.paddle_full_ocr_image_blocks);
    assert!(defaults.paddle_full_format_block_content);
    assert!(defaults.paddle_full_merge_tables);
    assert!(defaults.paddle_full_relevel_titles);
    assert!(defaults.paddle_full_show_formula_numbers);
    assert_eq!(defaults.pdf_extraction_timeout_secs, 1800);
    assert!(defaults.reuse_pdf_extraction_cache);

    // Unknown retired fields remain harmless because settings deserialization
    // is intentionally permissive for older files.
    let legacy: Settings = serde_json::from_str(r#"{"paddle_render_dpi":160}"#).unwrap();
    assert_eq!(legacy.paddle_page_concurrency, 0);
    assert_eq!(legacy.paddle_mtmd_batch_tokens, 0);
    assert_eq!(legacy.paddle_flash_attention, "auto");
    assert_eq!(legacy.paddle_max_output_tokens, 4096);
    assert_eq!(legacy.paddle_page_retries, 1);
    assert!(legacy.paddle_full_layout_detection);
    assert_eq!(legacy.paddle_full_layout_threshold, 0.5);
    assert!(legacy.paddle_full_layout_nms);
    assert_eq!(legacy.paddle_full_layout_merge_bboxes_mode, "large");
    assert!(legacy.paddle_full_merge_layout_blocks);
    assert!(legacy.paddle_full_ocr_image_blocks);
    assert!(legacy.paddle_full_format_block_content);
    assert!(legacy.paddle_full_merge_tables);
    assert!(legacy.paddle_full_relevel_titles);
    assert!(legacy.paddle_full_show_formula_numbers);
    assert_eq!(legacy.pdf_extraction_timeout_secs, 1800);
    assert!(legacy.reuse_pdf_extraction_cache);
}

#[test]
fn paddle_automatic_settings_resolve_to_supported_values() {
    let defaults = Settings::default();
    assert!(matches!(resolved_paddle_page_concurrency(&defaults), 1 | 2));
    assert!(matches!(
        resolved_paddle_mtmd_batch_tokens(&defaults),
        1024 | 2048
    ));

    let explicit = Settings {
        paddle_page_concurrency: 1,
        paddle_mtmd_batch_tokens: 512,
        ..Default::default()
    };
    assert_eq!(resolved_paddle_page_concurrency(&explicit), 1);
    assert_eq!(resolved_paddle_mtmd_batch_tokens(&explicit), 512);
}

#[test]
fn test_encrypt_decrypt_roundtrip() {
    let mut key = [0u8; KEY_SIZE];
    getrandom::fill(&mut key).unwrap();

    let original = "sk-ant-api03-test-key-12345";
    let encrypted = encrypt_string(original, &key).unwrap();

    assert!(encrypted.starts_with(ENC_PREFIX));
    assert_ne!(encrypted, original);

    let decrypted = decrypt_string(&encrypted, &key).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_empty_string_passthrough() {
    let key = [0u8; KEY_SIZE];
    assert_eq!(encrypt_string("", &key).unwrap(), "");
    assert_eq!(decrypt_string("", &key).unwrap(), "");
}

#[test]
fn quarantine_moves_corrupt_file_aside() {
    // Regression: a corrupt settings.json used to be silently replaced by
    // defaults, and the next save() (e.g. via switch_profile) overwrote the
    // user's settings — including encrypted API keys — permanently.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, "{not json").unwrap();

    let backup = quarantine_corrupt_file(&path).expect("quarantine should succeed");

    assert!(!path.exists());
    assert_eq!(backup, dir.path().join("settings.json.corrupt"));
    assert_eq!(fs::read_to_string(&backup).unwrap(), "{not json");
}

#[test]
fn test_plaintext_passthrough() {
    let key = [0u8; KEY_SIZE];
    // Legacy plaintext value (no enc: prefix) should pass through decrypt unchanged
    assert_eq!(
        decrypt_string("sk-plain-key", &key).unwrap(),
        "sk-plain-key"
    );
}

#[test]
fn test_wrong_key_fails() {
    let mut key1 = [0u8; KEY_SIZE];
    let mut key2 = [0u8; KEY_SIZE];
    getrandom::fill(&mut key1).unwrap();
    getrandom::fill(&mut key2).unwrap();

    let encrypted = encrypt_string("secret", &key1).unwrap();
    assert!(decrypt_string(&encrypted, &key2).is_err());
}

#[test]
fn unreadable_ciphertext_is_preserved_during_unrelated_save() {
    let key = Ok([7u8; KEY_SIZE]);
    let raw = "enc:not-valid-base64";
    assert_eq!(prepare_secret_for_save("", raw, &key, "test").unwrap(), raw);

    let missing_key = Err("key unavailable".to_string());
    assert_eq!(
        prepare_secret_for_save("", raw, &missing_key, "test").unwrap(),
        raw
    );
    assert!(prepare_secret_for_save("new-key", raw, &missing_key, "test").is_err());
}

#[test]
fn raw_profile_mutation_does_not_reencrypt_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let mut settings = Settings {
        active_profile: "quick-review".to_string(),
        anthropic_api_key: "enc:opaque-ciphertext".to_string(),
        ..Default::default()
    };
    save_raw_unlocked(&path, &settings).unwrap();

    settings = load_raw_settings_required(&path).unwrap();
    assert_eq!(settings.active_profile, "quick-review");
    assert_eq!(settings.anthropic_api_key, "enc:opaque-ciphertext");
}

#[test]
fn settings_validation_rejects_unsafe_or_out_of_range_values() {
    assert!(Settings::default().validate().is_ok());

    let mut invalid = Settings {
        max_workers: 0,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        codex_cli_model_selection: ModelSelection::Pinned {
            model: "--dangerous-flag".into(),
        },
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        local_base_url: "file:///tmp/model".into(),
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        claude_model: "--dangerous-flag".into(),
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        active_profile: "profile.with.dots".into(),
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        paddle_page_concurrency: 5,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        paddle_mtmd_batch_tokens: 1536,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        paddle_flash_attention: "sometimes".into(),
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        paddle_max_output_tokens: 3072,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        paddle_page_retries: 4,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        paddle_full_layout_threshold: 1.1,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        paddle_full_layout_merge_bboxes_mode: "overlap".into(),
        ..Default::default()
    };
    assert!(invalid.validate().is_err());

    invalid = Settings {
        pdf_extraction_timeout_secs: 60,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn plaintext_local_api_urls_are_loopback_only() {
    for url in [
        "http://localhost:11434/v1",
        "http://localhost.:11434/v1",
        "http://127.0.0.1:1234/v1",
        "http://127.42.0.9:1234/v1",
        "http://[::1]:1234/v1",
        "http://[::ffff:127.0.0.1]:1234/v1",
    ] {
        assert!(validate_local_base_url(url).is_ok(), "{url}");
    }
    for url in [
        "http://models.example.com/v1",
        "http://192.168.1.10:1234/v1",
        "http://10.0.0.2:1234/v1",
        "http://[fd00::1]:1234/v1",
    ] {
        assert!(validate_local_base_url(url).is_err(), "{url}");
    }
    assert!(validate_local_base_url("https://models.example.com/v1").is_ok());
}

#[test]
fn settings_reader_rejects_oversized_files() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    temp.as_file()
        .set_len(MAX_SETTINGS_BYTES as u64 + 1)
        .unwrap();
    assert!(read_settings_file(temp.path()).is_err());
}

// ── sanitize_cli_arg ──────────────────────────────────────────

#[test]
fn sanitize_normal_values() {
    assert_eq!(sanitize_cli_arg("sonnet"), "sonnet");
    assert_eq!(sanitize_cli_arg("o4-mini"), "o4-mini");
    assert_eq!(sanitize_cli_arg("gemini-2.5-pro"), "gemini-2.5-pro");
    assert_eq!(sanitize_cli_arg("high"), "high");
}

#[test]
fn sanitize_trims_whitespace() {
    assert_eq!(sanitize_cli_arg("  sonnet  "), "sonnet");
}

#[test]
fn sanitize_rejects_flag_like() {
    assert_eq!(sanitize_cli_arg("--dangerouslySkipPermissions"), "");
    assert_eq!(sanitize_cli_arg("-p"), "");
}

#[test]
fn sanitize_rejects_control_chars() {
    assert_eq!(sanitize_cli_arg("sonnet\n--bad"), "");
    assert_eq!(sanitize_cli_arg("sonnet\0"), "");
}

#[test]
fn sanitize_rejects_empty() {
    assert_eq!(sanitize_cli_arg(""), "");
    assert_eq!(sanitize_cli_arg("   "), "");
}

#[test]
fn legacy_models_migrate_to_roles_or_pins() {
    assert_eq!(
        ModelSelection::from_legacy("sonnet"),
        ModelSelection::Role {
            role: "sonnet".into()
        }
    );
    assert_eq!(
        ModelSelection::from_legacy("gpt-5.6-sol"),
        ModelSelection::Pinned {
            model: "gpt-5.6-sol".into()
        }
    );
    assert_eq!(ModelSelection::from_legacy(""), ModelSelection::Automatic);
    assert_eq!(
        ModelSelection::from_legacy("auto"),
        ModelSelection::Automatic
    );
}

#[test]
fn access_mode_controls_transport_and_provider_defaults_are_automatic() {
    let mut settings = Settings {
        codex_cli_model_selection: ModelSelection::Role {
            role: "balanced".into(),
        },
        codex_api_model_selection: ModelSelection::Pinned {
            model: "gpt-api-only".into(),
        },
        ..Default::default()
    };
    assert_eq!(settings.model_transport("codex"), "cli");
    assert_eq!(settings.model_selection("codex").label(), "Automatic");
    settings.openai_api_key = "secret".into();
    assert_eq!(settings.model_transport("codex"), "cli");
    settings.codex_access_mode = "api".into();
    assert_eq!(settings.model_transport("codex"), "api");
    assert_eq!(settings.model_selection("codex").label(), "Automatic");
}

#[test]
fn legacy_key_presence_migrates_to_explicit_api_mode() {
    let legacy: Settings = serde_json::from_str(r#"{"openai_api_key":"secret"}"#).unwrap();
    assert!(legacy.codex_access_mode.is_empty());
    assert_eq!(legacy.model_transport("codex"), "api");

    let normalized = legacy.normalized();
    assert_eq!(normalized.codex_access_mode, "api");
    assert_eq!(normalized.model_transport("codex"), "api");
}

#[test]
fn role_defaults_preserve_legacy_fallbacks_and_transport_specific_policies() {
    let legacy: Settings = serde_json::from_str(
        r#"{
            "preferred_provider":"antigravity",
            "default_sequential_model_overrides": {
                "antigravity:cli": {"mode":"pinned","model":"legacy-merge-model"}
            },
            "default_sequential_effort_overrides": {"antigravity:cli":"high"}
        }"#,
    )
    .unwrap();
    assert_eq!(legacy.parallel_agents(), vec!["antigravity"]);
    assert_eq!(legacy.sequential_agent(), "antigravity");
    assert_eq!(legacy.merge_agent(), "antigravity");
    assert_eq!(
        legacy.merge_model_selection("antigravity"),
        Some(ModelSelection::Pinned {
            model: "legacy-merge-model".into()
        })
    );
    assert_eq!(legacy.merge_effort("antigravity"), "high");
    assert_eq!(legacy.orientation_agent(), "antigravity");

    let mut settings = Settings {
        default_parallel_agents: vec!["claude".into(), "codex".into()],
        default_merge_agent: "codex".into(),
        default_sequential_agent: "antigravity".into(),
        default_orientation_agent: "claude".into(),
        ..Default::default()
    };
    settings.default_parallel_model_overrides.insert(
        "codex:cli".into(),
        ModelSelection::Pinned {
            model: "gpt-test".into(),
        },
    );
    settings.default_merge_model_overrides.insert(
        "codex:cli".into(),
        ModelSelection::Pinned {
            model: "gpt-merge".into(),
        },
    );
    settings
        .default_merge_effort_overrides
        .insert("codex:cli".into(), "medium".into());
    settings
        .default_orientation_effort_overrides
        .insert("claude:cli".into(), "high".into());
    assert!(settings.validate().is_ok());
    assert_eq!(
        settings.parallel_model_selection("codex"),
        Some(ModelSelection::Pinned {
            model: "gpt-test".into()
        })
    );
    assert_eq!(settings.merge_agent(), "codex");
    assert_eq!(
        settings.merge_model_selection("codex"),
        Some(ModelSelection::Pinned {
            model: "gpt-merge".into()
        })
    );
    assert_eq!(settings.merge_effort("codex"), "medium");
    assert_eq!(settings.orientation_effort("claude"), "high");

    settings.default_parallel_agents.push("claude".into());
    assert!(settings.validate().unwrap_err().contains("duplicates"));
}

#[test]
fn normalization_drops_retired_provider_ids_from_saved_settings() {
    // Settings written before the Gemini CLI removal can still name the
    // retired "gemini" provider in surviving fields. Loading must heal them
    // (the UI can neither display nor delete unknown providers) so the
    // run-launch validate() call keeps passing.
    let persisted = r#"{
        "preferred_provider": "gemini",
        "default_parallel_agents": ["claude", "gemini"],
        "default_merge_agent": "gemini",
        "default_sequential_agent": "gemini",
        "default_orientation_agent": "claude",
        "default_parallel_model_overrides": {
            "gemini:cli": {"mode": "pinned", "model": "gemini-2.5-pro"},
            "codex:cli": {"mode": "automatic"}
        },
        "default_merge_model_overrides": {
            "gemini:cli": {"mode": "pinned", "model": "gemini-2.5-pro"}
        },
        "default_merge_effort_overrides": {"gemini": "medium"},
        "default_sequential_effort_overrides": {"gemini": "high", "claude:cli": "low"}
    }"#;
    let settings = serde_json::from_str::<Settings>(persisted)
        .unwrap()
        .normalized();
    assert!(settings.validate().is_ok());
    assert_eq!(settings.preferred_provider, "claude");
    assert_eq!(settings.default_parallel_agents, vec!["claude"]);
    assert_eq!(settings.sequential_agent(), "claude");
    assert_eq!(settings.merge_agent(), "claude");
    assert_eq!(settings.orientation_agent(), "claude");
    assert!(!settings
        .default_parallel_model_overrides
        .contains_key("gemini:cli"));
    assert!(settings
        .default_parallel_model_overrides
        .contains_key("codex:cli"));
    assert!(settings.default_merge_model_overrides.is_empty());
    assert!(settings.default_merge_effort_overrides.is_empty());
    assert!(!settings
        .default_sequential_effort_overrides
        .contains_key("gemini"));
    assert_eq!(settings.sequential_effort("claude"), "low");
}

#[test]
fn load_paths_heal_retired_providers_and_extractors() {
    // Regression: settings persisted by pre-Antigravity builds could carry
    // preferred_provider "gemini" and pdf_extractor "marker". The load paths
    // used at run start must heal both instead of failing validate(), which
    // previously blocked every run, profile switch, and settings save until
    // the user hand-edited settings.json.
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        temp.path(),
        r#"{
            "preferred_provider": "gemini",
            "default_sequential_agent": "gemini",
            "pdf_extractor": "marker"
        }"#,
    )
    .unwrap();
    let settings = load_raw_settings_required(temp.path()).unwrap();
    assert!(settings.validate().is_ok());
    assert_eq!(settings.preferred_provider, "claude");
    assert_eq!(settings.sequential_agent(), "claude");
    assert_eq!(settings.pdf_extractor, "auto");
}

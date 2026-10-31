use super::crypto::{decrypt_string, encrypt_string, ENC_PREFIX, KEY_SIZE};
use super::extraction::resolve_pdf_extractor_for_paddle_availability;
use super::persistence::{
    load_raw_settings_required, prepare_secret_for_save, read_settings_file, save_raw_unlocked,
    MAX_SETTINGS_BYTES,
};
use super::quarantine::quarantine_corrupt_file;
use super::recovery::discard_undecryptable_secret_at;
use super::*;
use std::fs;

#[test]
fn validation_preserves_first_failure_order() {
    let mut settings = Settings {
        codex_backend: "unknown".into(),
        preferred_provider: "unknown".into(),
        max_workers: 0,
        ..Default::default()
    };
    assert_eq!(
        settings.validate().unwrap_err(),
        "Unsupported Workflow Codex backend"
    );
    settings.codex_backend = "app_server".into();
    assert_eq!(
        settings.validate().unwrap_err(),
        "Invalid preferred provider 'unknown'"
    );
    settings.preferred_provider = "claude".into();
    assert_eq!(
        settings.validate().unwrap_err(),
        "Maximum workers must be between 1 and 20"
    );
}

#[test]
fn frozen_settings_remain_scoped_across_spawned_work() {
    let first = crate::run_scope::ActiveScope::begin("settings-frozen-first");
    let second = crate::run_scope::ActiveScope::begin("settings-frozen-second");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(crate::run_scope::scoped(first.scope().clone(), async {
        let first_guard = freeze_for_run(Settings {
            max_workers: 1,
            ..Default::default()
        });
        assert_eq!(load().max_workers, 1);
        assert_eq!(
            crate::run_scope::spawn(async { load().max_workers })
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            crate::run_scope::spawn_blocking(|| load().max_workers)
                .await
                .unwrap(),
            1
        );
        crate::run_scope::scoped(second.scope().clone(), async {
            let second_guard = freeze_for_run(Settings {
                max_workers: 2,
                ..Default::default()
            });
            assert_eq!(load().max_workers, 2);
            assert_eq!(first.scope().frozen_settings().unwrap().max_workers, 1);
            drop(second_guard);
            assert!(second.scope().frozen_settings().is_none());
        })
        .await;
        assert_eq!(load().max_workers, 1);
        drop(first_guard);
        assert!(first.scope().frozen_settings().is_none());
    }));
}

#[test]
fn stale_frozen_guard_does_not_release_replacement() {
    let run = crate::run_scope::ActiveScope::begin("settings-frozen-replacement");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(crate::run_scope::scoped(run.scope().clone(), async {
        let old_guard = freeze_for_run(Settings {
            max_workers: 1,
            ..Default::default()
        });
        let current_guard = freeze_for_run(Settings {
            max_workers: 2,
            ..Default::default()
        });
        drop(old_guard);
        assert_eq!(load().max_workers, 2);
        drop(current_guard);
        assert!(run.scope().frozen_settings().is_none());
    }));
}

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
    assert_eq!(defaults.paddle_full_layout_threshold, 0.3);
    assert_eq!(defaults.paddle_full_layout_threshold_version, 1);
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
    assert_eq!(legacy.paddle_full_layout_threshold, 0.3);
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

    let backup = quarantine_corrupt_file(&path, &fs::read_to_string(&path).unwrap())
        .expect("quarantine should succeed");

    assert!(!path.exists());
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("settings.corrupt-"));
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
fn antigravity_is_api_only() {
    // Google subscription dispatch (the agy CLI) is disabled: Google's
    // Antigravity terms do not permit third-party software to use an
    // Antigravity sign-in.
    let settings = Settings::default();
    assert_eq!(settings.antigravity_access_mode, "api");
    assert_eq!(settings.model_transport("antigravity"), "api");

    // A stored subscription choice from an older build still loads, is
    // dispatched over the API, and normalizes to an explicit "api".
    let legacy: Settings =
        serde_json::from_str(r#"{"antigravity_access_mode":"subscription","antigravity_cli_model_selection":{"mode":"pinned","model":"legacy-model"},"antigravity_effort":"high"}"#).unwrap();
    assert_eq!(legacy.model_transport("antigravity"), "api");
    assert!(legacy.validate().is_err());
    let normalized = legacy.normalized();
    assert_eq!(normalized.antigravity_access_mode, "api");
    let roundtrip = serde_json::to_value(&normalized).unwrap();
    assert_eq!(
        roundtrip["antigravity_cli_model_selection"]["model"],
        "legacy-model"
    );
    assert_eq!(roundtrip["antigravity_effort"], "high");
    assert!(normalized.validate().is_ok());
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
                "antigravity:api": {"mode":"pinned","model":"legacy-merge-model"}
            },
            "default_sequential_effort_overrides": {"antigravity:api":"high"}
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

#[test]
fn codex_app_server_is_default_and_rejects_unknown_backends() {
    let mut settings = Settings::default();
    assert_eq!(settings.codex_backend, "app_server");
    let mut serialized = serde_json::to_value(&settings).unwrap();
    serialized.as_object_mut().unwrap().remove("codex_backend");
    assert_eq!(
        serde_json::from_value::<Settings>(serialized)
            .unwrap()
            .codex_backend,
        "app_server"
    );
    settings.codex_backend = "legacy_cli".into();
    assert!(settings.validate().is_ok());
    settings.codex_backend = "unknown".into();
    assert!(settings.validate().is_err());
}

#[test]
fn old_codex_defaults_migrate_once_and_advanced_legacy_choice_survives_reload() {
    // A path, not an open handle: Windows cannot replace a file that is open.
    let directory = tempfile::tempdir().unwrap();
    let temp = directory.path().join("settings.json");
    for backend in [None, Some("legacy_cli"), Some("app_server")] {
        for mode in ["api", "subscription"] {
            let mut raw = serde_json::json!({
                "codex_access_mode": mode,
                "openai_api_key": "test-preserved-key",
                "default_parallel_model_overrides": {"codex:cli": {"mode": "pinned", "model": "test-model"}}
            });
            if let Some(backend) = backend {
                raw["codex_backend"] = backend.into();
            }
            std::fs::write(&temp, serde_json::to_vec(&raw).unwrap()).unwrap();
            let mut settings = load_raw_settings_required(&temp).unwrap();
            assert_eq!(settings.codex_backend, "app_server");
            assert_eq!(settings.codex_backend_preference_version, 1);
            assert_eq!(settings.codex_access_mode, mode);
            assert_eq!(settings.openai_api_key, "test-preserved-key");
            assert_eq!(
                settings.default_parallel_model_overrides["codex:cli"],
                ModelSelection::Pinned {
                    model: "test-model".into()
                }
            );

            settings.codex_backend = "legacy_cli".into();
            save_raw_unlocked(&temp, &settings).unwrap();
            let reloaded = load_raw_settings_required(&temp).unwrap().normalized();
            assert_eq!(reloaded.codex_backend, "legacy_cli");
            assert_eq!(reloaded.codex_backend_preference_version, 1);
            assert_eq!(reloaded.codex_access_mode, mode);
        }
    }
}

#[test]
fn the_former_paddle_layout_threshold_default_moves_once() {
    let directory = tempfile::tempdir().unwrap();
    let temp = directory.path().join("settings.json");
    for (saved, expected) in [
        (Some(0.5), 0.3),
        (Some(0.3), 0.3),
        (Some(0.7), 0.7),
        (None, 0.3),
    ] {
        let mut raw = serde_json::json!({});
        if let Some(saved) = saved {
            raw["paddle_full_layout_threshold"] = serde_json::json!(saved);
        }
        std::fs::write(&temp, serde_json::to_vec(&raw).unwrap()).unwrap();
        let mut settings = load_raw_settings_required(&temp).unwrap();
        assert_eq!(settings.paddle_full_layout_threshold, expected as f32);
        assert_eq!(settings.paddle_full_layout_threshold_version, 1);

        // A choice made after the move stays, even at the former default.
        settings.paddle_full_layout_threshold = 0.5;
        save_raw_unlocked(&temp, &settings).unwrap();
        let reloaded = load_raw_settings_required(&temp).unwrap().normalized();
        assert_eq!(reloaded.paddle_full_layout_threshold, 0.5);
        assert_eq!(reloaded.paddle_full_layout_threshold_version, 1);
    }
}

#[test]
fn quarantine_never_overwrites_a_backup_or_a_concurrent_repair() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, "first corrupt").unwrap();
    let first = quarantine_corrupt_file(&path, "first corrupt").unwrap();
    fs::write(&path, "second corrupt").unwrap();
    let second = quarantine_corrupt_file(&path, "second corrupt").unwrap();
    assert_ne!(first, second);
    assert_eq!(fs::read_to_string(first).unwrap(), "first corrupt");
    assert_eq!(fs::read_to_string(second).unwrap(), "second corrupt");
    fs::write(&path, "{}").unwrap();
    assert!(quarantine_corrupt_file(&path, "second corrupt").is_none());
    assert_eq!(fs::read_to_string(path).unwrap(), "{}");
}

#[test]
fn imported_bundle_settings_are_normalized_before_validation() {
    let exported = Settings {
        antigravity_access_mode: "subscription".to_string(),
        preferred_provider: "gemini".to_string(),
        ..Default::default()
    };
    assert!(exported.validate().is_err());
    let normalized = exported.normalized_for_import();
    assert_eq!(normalized.validate(), Ok(()));
    assert_eq!(normalized.antigravity_access_mode, "api");
    assert_ne!(normalized.preferred_provider, "gemini");
}

#[test]
fn imported_settings_move_the_former_paddle_layout_threshold_default_once() {
    let earlier_build = Settings {
        paddle_full_layout_threshold: 0.5,
        paddle_full_layout_threshold_version: 0,
        ..Default::default()
    };
    assert_eq!(
        earlier_build
            .normalized_for_import()
            .paddle_full_layout_threshold,
        0.3
    );

    let chosen = Settings {
        paddle_full_layout_threshold: 0.5,
        ..Default::default()
    };
    assert_eq!(
        chosen.normalized_for_import().paddle_full_layout_threshold,
        0.5
    );
}

#[test]
fn an_undecryptable_stored_key_can_be_discarded_but_a_readable_one_cannot() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("settings.json");
    let key_a = [7u8; KEY_SIZE];
    let key_b = [9u8; KEY_SIZE];
    let stored = Settings {
        google_api_key: encrypt_string("google-secret", &key_a).unwrap(),
        ..Default::default()
    };
    fs::write(&path, serde_json::to_string_pretty(&stored).unwrap()).unwrap();

    // The keyfile changed: the ciphertext is unreadable and may be discarded.
    assert!(decrypt_string(&stored.google_api_key, &key_b).is_err());
    assert_eq!(
        discard_undecryptable_secret_at(&path, &key_b, "google"),
        Ok(())
    );
    let reloaded: Settings = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert!(reloaded.google_api_key.is_empty());

    // A readable key is never discarded through this path.
    fs::write(&path, serde_json::to_string_pretty(&stored).unwrap()).unwrap();
    assert!(discard_undecryptable_secret_at(&path, &key_a, "google").is_err());
    assert!(discard_undecryptable_secret_at(&path, &key_b, "gemini").is_err());
    let untouched: Settings = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(untouched.google_api_key, stored.google_api_key);
}

#[test]
fn fresh_stages_follow_the_preferred_provider() {
    let settings = Settings {
        preferred_provider: "codex".into(),
        ..Default::default()
    };
    assert_eq!(settings.parallel_agents(), ["codex"]);
    assert_eq!(settings.sequential_agent(), "codex");
    assert_eq!(settings.merge_agent(), "codex");
    assert_eq!(settings.orientation_agent(), "codex");
}

#[test]
fn saved_copies_of_the_preferred_provider_are_released_once() {
    let mut settings: Settings = serde_json::from_value(serde_json::json!({
        "preferred_provider": "claude",
        "default_parallel_agents": ["claude"],
        "default_merge_agent": "claude",
        "default_sequential_agent": "claude",
        "default_orientation_agent": "claude",
    }))
    .unwrap();
    settings.normalize_access_modes();
    assert_eq!(settings.model_defaults_version, 1);
    assert!(settings.default_parallel_agents.is_empty());
    assert!(settings.default_merge_agent.is_empty());
    assert!(settings.default_sequential_agent.is_empty());
    assert!(settings.default_orientation_agent.is_empty());

    // A later choice that happens to match stays a choice.
    settings.default_sequential_agent = "claude".into();
    settings.normalize_access_modes();
    assert_eq!(settings.default_sequential_agent, "claude");
}

#[test]
fn releasing_stages_never_changes_what_they_resolve_to() {
    let pinned = |model: &str| ModelSelection::Pinned {
        model: model.into(),
    };
    let mut settings = Settings {
        model_defaults_version: 0,
        preferred_provider: "claude".into(),
        default_parallel_agents: vec!["claude".into(), "codex".into()],
        default_merge_agent: "claude".into(),
        default_sequential_agent: "claude".into(),
        default_sequential_model_overrides: [("claude:cli".to_string(), pinned("opus"))].into(),
        default_orientation_agent: "codex".into(),
        ..Default::default()
    };
    let merge_model = settings.merge_model_selection("claude");
    settings.normalize_access_modes();

    // Two reviewers, a different provider, and a stage's own model all stay.
    assert_eq!(settings.parallel_agents(), ["claude", "codex"]);
    assert_eq!(settings.orientation_agent(), "codex");
    assert_eq!(settings.default_sequential_agent, "claude");
    // Clearing Merge here would hand it Sequential's pinned model.
    assert_eq!(settings.default_merge_agent, "claude");
    assert_eq!(settings.merge_model_selection("claude"), merge_model);
}

#[test]
fn the_default_model_applies_only_to_its_own_provider() {
    let settings = Settings {
        preferred_provider: "claude".into(),
        default_model: "opus".into(),
        default_effort: "high".into(),
        default_merge_agent: "claude".into(),
        default_orientation_effort_overrides: [("claude:cli".to_string(), "low".to_string())]
            .into(),
        ..Default::default()
    };
    assert_eq!(settings.model_selection("claude").label(), "opus");
    assert_eq!(settings.model_selection("codex").label(), "Automatic");
    assert_eq!(settings.model_effort("claude"), "high");
    assert_eq!(settings.model_effort("codex"), "");
    assert_eq!(settings.sequential_effort("claude"), "high");
    assert_eq!(settings.merge_effort("claude"), "high");
    assert_eq!(settings.merge_effort("codex"), "");
    // A stage's own thinking level still wins.
    assert_eq!(settings.orientation_effort("claude"), "low");
}

#[test]
fn inherited_merge_prefers_sequential_thinking_over_the_default() {
    let settings = Settings {
        preferred_provider: "claude".into(),
        default_effort: "high".into(),
        default_sequential_effort_overrides: [("claude:cli".to_string(), "low".to_string())].into(),
        ..Default::default()
    };
    assert_eq!(settings.merge_effort("claude"), "low");
}

#[test]
fn the_default_model_stays_out_of_legacy_provider_fields() {
    let mut settings = Settings {
        preferred_provider: "claude".into(),
        default_model: "opus".into(),
        ..Default::default()
    };
    settings.sync_legacy_model_fields();
    assert_eq!(settings.claude_model, "");
}

#[test]
fn unsafe_default_models_are_rejected() {
    for default_model in [" opus", "opus\n--flag"] {
        let settings = Settings {
            default_model: default_model.into(),
            ..Default::default()
        };
        assert!(settings.validate().is_err(), "{default_model:?}");
    }
}

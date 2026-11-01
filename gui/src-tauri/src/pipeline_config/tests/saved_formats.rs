//! Saved settings and profiles survive restores, older formats, and changes
//! in validation.

use super::*;
use crate::settings::{ModelSelection, Settings};
use serde_json::{json, Value};

/// Every setting a backup restores. A new `Settings` field fails the test
/// below until it is added here or to `DEVICE_SETTINGS`.
const PORTABLE_SETTINGS: &[&str] = &[
    "preferred_provider",
    "default_model",
    "default_effort",
    "default_model_chosen",
    "model_defaults_version",
    "default_parallel_agents",
    "default_parallel_model_overrides",
    "default_parallel_effort_overrides",
    "default_merge_agent",
    "default_merge_model_overrides",
    "default_merge_effort_overrides",
    "default_sequential_agent",
    "default_sequential_model_overrides",
    "default_sequential_effort_overrides",
    "default_orientation_agent",
    "default_orientation_model_overrides",
    "default_orientation_effort_overrides",
    "usage_limit_fallback_agent",
    "usage_limit_fallback_model_overrides",
    "usage_limit_fallback_effort_overrides",
    "max_workers",
    "active_profile",
    "claude_model",
    "claude_cli_model_selection",
    "claude_api_model_selection",
    "claude_effort",
    "codex_model",
    "codex_cli_model_selection",
    "codex_api_model_selection",
    "codex_effort",
    "antigravity_cli_model_selection",
    "antigravity_api_model_selection",
    "antigravity_effort",
    "pdf_extractor",
    "paddle_page_concurrency",
    "paddle_mtmd_batch_tokens",
    "paddle_flash_attention",
    "paddle_max_output_tokens",
    "paddle_page_retries",
    "paddle_full_layout_detection",
    "paddle_full_layout_threshold",
    "paddle_full_layout_threshold_version",
    "paddle_full_layout_nms",
    "paddle_full_layout_merge_bboxes_mode",
    "paddle_full_merge_layout_blocks",
    "paddle_full_ocr_image_blocks",
    "paddle_full_format_block_content",
    "paddle_full_merge_tables",
    "paddle_full_relevel_titles",
    "paddle_full_show_formula_numbers",
    "pdf_extraction_timeout_secs",
    "reuse_pdf_extraction_cache",
    "verbose_logging",
    "step_timeout_secs",
    "max_retries",
    "max_concurrent_reviews",
    "review_max_cost_usd",
    "review_max_calls",
    "review_max_minutes",
    "auto_revision_reconciliation",
    "max_saved_runs",
    "max_saved_run_bytes",
    "local_base_url",
    "local_model",
];

fn keys_of(settings: &Settings) -> std::collections::BTreeSet<String> {
    serde_json::to_value(settings)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

fn with_chosen_default() -> Settings {
    Settings {
        default_model: "local-default".into(),
        default_effort: "high".into(),
        default_model_chosen: true,
        ..Default::default()
    }
}

#[test]
fn every_setting_is_classified_for_restore() {
    let found = keys_of(&with_chosen_default());
    let classified: std::collections::BTreeSet<String> = PORTABLE_SETTINGS
        .iter()
        .chain(DEVICE_SETTINGS)
        .map(|key| key.to_string())
        .collect();
    assert_eq!(found, classified);
}

/// A settings object with every field moved off its default.
fn changed_settings() -> Value {
    let pinned = serde_json::to_value(ModelSelection::Pinned {
        model: "backup-model".into(),
    })
    .unwrap();
    let mut settings = serde_json::to_value(with_chosen_default()).unwrap();
    for (key, value) in settings.as_object_mut().unwrap() {
        *value = if key == "default_model_chosen" {
            json!(true)
        } else if key.ends_with("model_overrides") {
            json!({"claude:cli": pinned})
        } else if key.ends_with("effort_overrides") {
            json!({"claude:cli": "high"})
        } else if key.ends_with("model_selection") {
            pinned.clone()
        } else {
            match value {
                Value::String(text) => json!(format!("{text}-backup")),
                Value::Bool(flag) => json!(!*flag),
                Value::Array(_) => json!(["codex"]),
                Value::Number(number) if number.is_f64() => {
                    json!(number.as_f64().unwrap() + 0.25)
                }
                Value::Number(number) => json!(number.as_u64().unwrap() + 3),
                other => panic!("no changed value for {key}: {other}"),
            }
        };
    }
    settings
}

#[test]
fn a_backup_restores_every_setting_except_this_computers_own() {
    let backup = changed_settings();
    let imported: Settings = serde_json::from_value(backup.clone()).unwrap();
    let local = Settings {
        anthropic_api_key: "local-key".into(),
        codex_access_mode: "subscription".into(),
        ..Default::default()
    };

    let restored = restored_settings(&local, &imported, &backup).unwrap();

    let restored = serde_json::to_value(restored).unwrap();
    let local = serde_json::to_value(local).unwrap();
    for key in PORTABLE_SETTINGS {
        assert_eq!(restored[*key], backup[*key], "{key}");
    }
    for key in DEVICE_SETTINGS {
        assert_eq!(restored[*key], local[*key], "{key}");
    }
}

#[test]
fn an_older_backup_keeps_settings_it_never_recorded() {
    let local = Settings {
        review_max_cost_usd: 5.0,
        max_concurrent_reviews: 4,
        ..with_chosen_default()
    };
    let backup = json!({"max_workers": 3, "preferred_provider": "codex"});
    let imported: Settings = serde_json::from_value(backup.clone()).unwrap();

    let restored = restored_settings(&local, &imported, &backup).unwrap();

    assert_eq!(restored.max_workers, 3);
    assert_eq!(restored.preferred_provider, "codex");
    assert_eq!(restored.review_max_cost_usd, 5.0);
    assert_eq!(restored.max_concurrent_reviews, 4);
    assert_eq!(restored.default_model, "local-default");
    assert!(restored.default_model_chosen);
}

#[test]
fn a_current_backup_with_automatic_defaults_clears_the_chosen_ones() {
    let imported = Settings::default();
    let backup = serde_json::to_value(&imported).unwrap();
    assert!(backup.get("default_model").is_none());

    let restored = restored_settings(&with_chosen_default(), &imported, &backup).unwrap();

    assert_eq!(restored.default_model, "");
    assert!(!restored.default_model_chosen);
}

fn write_profile(dir: &Path, id: &str, profile: &Value) -> PathBuf {
    let path = dir.join(format!("{id}.json"));
    fs::write(&path, serde_json::to_string_pretty(profile).unwrap()).unwrap();
    path
}

/// A copy of Paper Review made before its contract changed.
fn stale_contract_copy(name: &str) -> Value {
    let mut profile = serde_json::to_value(auto_review_profile()).unwrap();
    profile["name"] = json!(name);
    profile["orientation_schema"]["x-pipeline-contract"] = json!("auto-review-orientation-v0");
    profile
}

#[test]
fn a_profile_failing_validation_is_listed_with_its_reason_and_stays_in_backups() {
    let dir = tempfile::tempdir().unwrap();
    write_profile(dir.path(), "old-copy", &stale_contract_copy("My old copy"));
    write_profile(
        dir.path(),
        "good",
        &serde_json::to_value(auto_review_profile()).unwrap(),
    );

    let listed = list_profiles_at(dir.path()).unwrap();

    assert_eq!(listed.len(), 2, "{listed:?}");
    let old = listed.iter().find(|item| item.id == "old-copy").unwrap();
    assert_eq!(old.name, "My old copy");
    assert!(old.step_count > 0);
    let reason = old.invalid.as_deref().unwrap();
    assert!(reason.contains("is invalid"), "{reason}");
    assert!(listed
        .iter()
        .find(|item| item.id == "good")
        .unwrap()
        .invalid
        .is_none());

    let backup = bundle_profiles_at(dir.path()).unwrap();
    let kept = backup.iter().find(|item| item.id == "old-copy").unwrap();
    assert_eq!(kept.invalid.as_deref(), Some(reason));
    assert!(!kept.steps.is_empty());

    // The serialized backup restores whole, and the profile still can't run.
    let parsed: Vec<ProfileExport> =
        serde_json::from_str(&serde_json::to_string(&backup).unwrap()).unwrap();
    let validated = validate_bundle_profiles(&parsed, "good").unwrap();
    assert_eq!(validated.len(), 2);
    let restored = dir.path().join("restored.json");
    fs::write(
        &restored,
        serde_json::to_string(&kept.to_profile_data()).unwrap(),
    )
    .unwrap();
    assert!(load_profile_at(&restored, "restored").is_err());
}

#[test]
fn a_damaged_profile_file_is_listed_and_a_stray_file_name_is_not() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("damaged.json"), "{ not json").unwrap();
    fs::write(dir.path().join("not a profile.json"), "{}").unwrap();

    let listed = list_profiles_at(dir.path()).unwrap();

    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].name, "damaged");
    assert!(listed[0].invalid.is_some());
    assert!(bundle_profiles_at(dir.path()).unwrap().is_empty());
}

fn selects_primary(step: &StepConfig) -> bool {
    step.context
        .include
        .iter()
        .any(|selector| matches!(selector, ArtifactSelector::Primary { .. }))
}

fn reads_report_of(step: &StepConfig, producer: &str) -> bool {
    step.context.include.iter().any(|selector| {
        matches!(selector, ArtifactSelector::Step { step, parts, .. }
            if step == producer && parts.contains(&StepArtifactPart::Report))
    })
}

fn assert_wired(steps: &[StepConfig]) {
    assert_eq!(steps.len(), 3);
    assert!(selects_primary(&steps[0]));
    assert!(selects_primary(&steps[1]));
    assert!(reads_report_of(&steps[2], "first"));
    assert!(reads_report_of(&steps[2], "second"));
}

const LEGACY_FIELDS: &str = r#""referees": [
        {"id": "first", "label": "First", "prompt": "Read the paper.", "enabled": true, "web_search": true},
        {"id": "second", "label": "Second", "prompt": "Read it again.", "enabled": true}
    ],
    "post_steps": [
        {"id": "final", "label": "Final", "prompt": "Combine the reports.", "enabled": true}
    ]"#;

#[test]
fn a_legacy_profile_import_reads_the_paper_and_the_reports() {
    for file in [
        format!(r#"{{"type": "profile", "name": "Old", {LEGACY_FIELDS}}}"#),
        format!("{{{LEGACY_FIELDS}}}"),
    ] {
        match import_envelope(&file).unwrap() {
            ExportEnvelope::Profile { steps, .. } => {
                assert_wired(&steps);
                assert_eq!(steps[0].tools, vec!["WebSearch"]);
                validate_profile_steps(&steps).unwrap();
            }
            _ => panic!("expected a profile"),
        }
    }
}

#[test]
fn a_legacy_profile_on_disk_reads_the_paper_and_the_reports() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.json");
    fs::write(&path, format!(r#"{{"name": "Old", {LEGACY_FIELDS}}}"#)).unwrap();
    assert_wired(&load_profile_at(&path, "old").unwrap().steps);
}

fn early_steps() -> Value {
    json!([
        {"id": "first", "label": "First", "prompt": "Read.", "enabled": true, "phase": "parallel", "tools": ["Read"]},
        {"id": "second", "label": "Second", "prompt": "Read.", "enabled": true, "phase": "parallel"},
        {"id": "final", "label": "Final", "prompt": "Combine.", "enabled": true, "phase": "sequential", "tools": ["Read", "Write"]},
    ])
}

#[test]
fn an_early_export_without_contexts_gets_the_flow_it_implied() {
    let file =
        json!({"type": "profile", "schema_version": 5, "name": "Early", "steps": early_steps()});
    match import_envelope(&file.to_string()).unwrap() {
        ExportEnvelope::Profile { steps, .. } => {
            assert_wired(&steps);
            assert!(selects_primary(&steps[2]), "it declared Read");
            assert!(steps[2].tools.is_empty());
        }
        _ => panic!("expected a profile"),
    }
}

#[test]
fn an_early_profile_on_disk_and_in_a_backup_gets_the_flow_it_implied() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_profile(
        dir.path(),
        "early",
        &json!({"name": "Early", "steps": early_steps()}),
    );
    assert_wired(&load_profile_at(&path, "early").unwrap().steps);

    let file = json!({
        "type": "bundle",
        "active_profile": "early",
        "settings": {},
        "profiles": [{"id": "early", "name": "Early", "steps": early_steps()}],
    });
    match import_envelope(&file.to_string()).unwrap() {
        ExportEnvelope::Bundle { profiles, .. } => assert_wired(&profiles[0].steps),
        _ => panic!("expected a bundle"),
    }
}

#[test]
fn saved_contexts_stay_as_written_including_an_isolated_step() {
    let mut steps = early_steps();
    for step in steps.as_array_mut().unwrap() {
        step["context"] = json!({"include": []});
    }
    let file = json!({"type": "profile", "schema_version": 11, "name": "Now", "steps": steps});
    match import_envelope(&file.to_string()).unwrap() {
        ExportEnvelope::Profile { steps, .. } => {
            assert!(steps.iter().all(|step| step.context.include.is_empty()));
        }
        _ => panic!("expected a profile"),
    }
}

#[test]
fn a_file_with_some_contexts_missing_is_refused() {
    let mut steps = early_steps();
    steps[0]["context"] = json!({"include": []});
    let file = json!({"type": "profile", "name": "Mixed", "steps": steps});
    let error = import_envelope(&file.to_string()).unwrap_err();
    assert!(error.contains("say what they read"), "{error}");
}

#[test]
fn an_older_parallel_step_is_wired_and_a_sequential_one_is_refused() {
    let referee = json!({"type": "referee", "data": {
        "id": "one", "label": "One", "prompt": "Read.", "enabled": true
    }});
    match import_envelope(&referee.to_string()).unwrap() {
        ExportEnvelope::Step { data } => assert!(selects_primary(&data)),
        _ => panic!("expected a step"),
    }

    let post_step = json!({"type": "post_step", "data": {
        "id": "last", "label": "Last", "prompt": "Combine.", "enabled": true
    }});
    let early = json!({"type": "step", "data": early_steps()[2]});
    for file in [post_step, early] {
        let error = import_envelope(&file.to_string()).unwrap_err();
        assert!(error.contains("older file"), "{error}");
    }
}

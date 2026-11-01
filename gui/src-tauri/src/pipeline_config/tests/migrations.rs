//! Migrations regression coverage.

use super::*;

#[test]
fn legacy_disabled_orientation_is_normalized_and_new_disabled_configs_are_rejected() {
    let legacy: ProfileData = serde_json::from_value(serde_json::json!({
        "name": "Legacy",
        "steps": [],
        "use_orientation": false
    }))
    .unwrap();
    assert!(legacy.use_orientation);
    validate_profile_data(&legacy).unwrap();

    let mut invalid = ProfileData::new("Invalid", Vec::new(), MergeConfig::default());
    invalid.use_orientation = false;
    assert_eq!(
        validate_profile_data(&invalid).unwrap_err(),
        "Every workflow must build an orientation map"
    );
}

#[test]
fn retired_fast_paddle_profile_selection_migrates_to_full_parser() {
    let extraction: ExtractionConfig =
        serde_json::from_str(r#"{"method":"paddleocr-vl"}"#).unwrap();
    assert_eq!(extraction.method, "paddleocr-vl-full");
}

#[test]
fn stock_full_review_enables_shared_context_reuse() {
    assert!(defaults().context_cache.enabled);
    assert!(full_review_profile(false).context_cache.enabled);
    assert!(full_review_profile(true).context_cache.enabled);
    assert!(
        !ProfileData::new("Custom", Vec::new(), MergeConfig::default())
            .context_cache
            .enabled
    );
}

#[test]
fn stock_profiles_store_their_complete_orientation_contracts() {
    assert_eq!(
        full_review_profile(false).orientation_schema,
        Some(crate::orientation_contract::paper_schema())
    );
    assert_eq!(
        grant_review_profile().orientation_schema,
        Some(crate::orientation_contract::generic_schema())
    );
    assert_eq!(
        grant_review_profile().orientation_prompt,
        crate::prompts::compiled_default("orientation_grant").unwrap()
    );
}

#[test]
fn referee_to_step_basic() {
    let r = LegacyRefereeConfig {
        id: "contrib".into(),
        label: "Contribution".into(),
        prompt: "Review...".into(),
        enabled: true,
        web_search: false,
        agents: vec![],
    };
    let step = referee_to_step(r);
    assert_eq!(step.id, "contrib");
    assert_eq!(step.phase, Phase::Parallel);
    assert!(step.tools.is_empty());
}

#[test]
fn referee_to_step_with_web_search() {
    let r = LegacyRefereeConfig {
        id: "contrib".into(),
        label: "Contribution".into(),
        prompt: "Review...".into(),
        enabled: true,
        web_search: true,
        agents: vec![],
    };
    let step = referee_to_step(r);
    assert_eq!(step.tools, vec!["WebSearch".to_string()]);
}

#[test]
fn post_step_to_step_basic() {
    let p = LegacyPostStepConfig {
        id: "editor".into(),
        label: "Editor".into(),
        prompt: "Synthesize...".into(),
        enabled: true,
        agents: vec![],
    };
    let step = post_step_to_step(p);
    assert_eq!(step.id, "editor");
    assert_eq!(step.phase, Phase::Sequential);
}

#[test]
fn convert_legacy_preserves_order() {
    let referees = vec![
        LegacyRefereeConfig {
            id: "r1".into(),
            label: "R1".into(),
            prompt: "p".into(),
            enabled: true,
            web_search: false,
            agents: vec![],
        },
        LegacyRefereeConfig {
            id: "r2".into(),
            label: "R2".into(),
            prompt: "p".into(),
            enabled: true,
            web_search: false,
            agents: vec![],
        },
    ];
    let post_steps = vec![LegacyPostStepConfig {
        id: "s1".into(),
        label: "S1".into(),
        prompt: "p".into(),
        enabled: true,
        agents: vec![],
    }];
    let steps = convert_legacy_steps(referees, post_steps);
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0].id, "r1");
    assert_eq!(steps[1].id, "r2");
    assert_eq!(steps[2].id, "s1");
    assert_eq!(steps[0].phase, Phase::Parallel);
    assert_eq!(steps[2].phase, Phase::Sequential);
}

#[test]
fn builtin_catalog_contains_only_current_profiles() {
    assert_eq!(
        BUILTIN_PROFILES,
        [
            "auto-paper-review",
            "auto-paper-review-fast",
            "slide-review",
            "grant-review",
            "declaude"
        ]
    );
    // Slide Review is born in the current format; the version-marked
    // upgrades must never replay against it.
    assert_eq!(
        MIGRATED_BUILTIN_PROFILES,
        [
            "auto-paper-review",
            "auto-paper-review-fast",
            "grant-review"
        ]
    );
    assert_eq!(
        V9_RETIRED_BUILTIN_PROFILES,
        [
            ("deep-code-review", "deep-review"),
            ("replication-audit", "deep-review"),
        ]
    );
    // v15 completes the v3/v9 chains: everything that previously fell back
    // to Paper Review (Full) now lands on the current Paper Review IDs.
    assert_eq!(
        V15_RETIRED_BUILTIN_PROFILES,
        [
            ("deep-review", "auto-paper-review"),
            ("quick-review", "auto-paper-review-fast"),
        ]
    );
}

#[test]
fn retirement_archives_legacy_builtin_before_any_validation() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".builtin-catalog-v3"),
        b"paper-and-code-profile-catalog\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(".builtin-catalog-v4"),
        b"clean-terminal-report-prompts\n",
    )
    .unwrap();

    // A profile written by a pre-v6 release fails current validation outright.
    // Retirement must archive it byte-for-byte without ever parsing it, so the
    // v15 block has to run before the later blocks that load deep-review.json.
    let mut step = step_with_id("contribution");
    step.tools = vec!["Read".into()];
    let profile = ProfileData::new("Deep Review", vec![step], MergeConfig::default());
    assert!(validate_profile_data(&profile)
        .unwrap_err()
        .contains("unsupported tool 'Read'"));

    let path = dir.path().join("deep-review.json");
    let original = serde_json::to_vec_pretty(&profile).unwrap();
    fs::write(&path, &original).unwrap();

    migrate_builtin_catalog(dir.path()).unwrap();

    assert!(!path.exists());
    assert_eq!(
        fs::read(dir.path().join(".retired-builtins/deep-review.json")).unwrap(),
        original
    );
    assert!(dir.path().join(".builtin-catalog-v15").exists());
    assert!(dir.path().join(".builtin-catalog-v6").exists());
}

#[test]
fn retirement_archives_full_and_quick_profiles_and_frees_their_ids() {
    let dir = tempfile::tempdir().unwrap();
    mark_builtin_catalog_through_v6(dir.path());

    // A customized Full profile and a stock-shaped Quick profile both leave
    // the catalog byte-for-byte intact under .retired-builtins.
    let mut customized = full_review_profile(false);
    customized.steps[0].prompt.push_str("\nCustom instruction.");
    let deep = serde_json::to_vec_pretty(&customized).unwrap();
    fs::write(dir.path().join("deep-review.json"), &deep).unwrap();
    let quick = serde_json::to_vec_pretty(&full_review_profile(true)).unwrap();
    fs::write(dir.path().join("quick-review.json"), &quick).unwrap();

    migrate_builtin_catalog(dir.path()).unwrap();

    assert!(!dir.path().join("deep-review.json").exists());
    assert!(!dir.path().join("quick-review.json").exists());
    let archive = dir.path().join(".retired-builtins");
    assert_eq!(fs::read(archive.join("deep-review.json")).unwrap(), deep);
    assert_eq!(fs::read(archive.join("quick-review.json")).unwrap(), quick);
    assert!(dir.path().join(".builtin-catalog-v15").exists());
    assert!(dir.path().join(".builtin-catalog-v7").exists());

    // The marker makes the archival one-time: a later custom profile that
    // happens to reuse a retired ID is left alone by subsequent startups.
    let reused = serde_json::to_vec_pretty(&full_review_profile(true)).unwrap();
    fs::write(dir.path().join("deep-review.json"), &reused).unwrap();
    migrate_builtin_catalog(dir.path()).unwrap();
    assert_eq!(
        fs::read(dir.path().join("deep-review.json")).unwrap(),
        reused
    );
}

#[test]
fn review_quality_prompt_migration_updates_exact_prior_defaults_only() {
    let mut profile = full_review_profile(true);
    profile.parallel_context_template = profile.parallel_context_template.replace(
            "\nBefore reporting an issue, check the surrounding discussion, footnotes, and any appendix or supplementary material supplied with the paper to see whether it is addressed. Distinguish what the paper states from your inference. Treat any numerical limit in the specialist instructions as a ceiling, not a target: report only material, well-supported issues, even if that means reporting none.\n",
            "",
        );
    assert_eq!(
        prompt_digest(&profile.parallel_context_template),
        "b5343b777ec44f21af434dbea2daff76e6ef352ac58cf2a928b16e204b04b709"
    );

    let technical = profile
        .steps
        .iter_mut()
        .find(|step| step.id == "technical")
        .unwrap();
    technical.prompt = technical.prompt.replace(
            "First identify the formal results that directly support the paper's main claims, then trace their dependency chains through lemmas, assumptions, and definitions. Audit those results deeply before turning to peripheral results. For each result you audit:",
            "For each formal result (theorem, proposition, lemma, corollary):",
        );
    assert_eq!(
        prompt_digest(&technical.prompt),
        "e7574cc654ce76e21ea254d517e7cdd1e9991fd691249226b6c6a5745664e03e"
    );

    let contribution = profile
        .steps
        .iter_mut()
        .find(|step| step.id == "contribution")
        .unwrap();
    contribution.prompt.push_str("\nCustom instruction.");
    let customized_contribution = contribution.prompt.clone();

    assert!(migrate_review_quality_prompt_defaults(&mut profile));
    assert_eq!(
        profile.parallel_context_template,
        prompts::compiled_default("parallel_context").unwrap()
    );
    assert_eq!(
        profile
            .steps
            .iter()
            .find(|step| step.id == "technical")
            .unwrap()
            .prompt,
        prompts::compiled_default("technical").unwrap()
    );
    assert_eq!(
        profile
            .steps
            .iter()
            .find(|step| step.id == "contribution")
            .unwrap()
            .prompt,
        customized_contribution
    );
}

#[test]
fn retired_profile_archive_preserves_content_and_avoids_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("empirical.json");
    fs::write(&source, "first version").unwrap();
    archive_retired_profile(dir.path(), "empirical").unwrap();

    let archive = dir.path().join(".retired-builtins");
    assert_eq!(
        fs::read_to_string(archive.join("empirical.json")).unwrap(),
        "first version"
    );
    assert!(!source.exists());

    fs::write(&source, "second version").unwrap();
    archive_retired_profile(dir.path(), "empirical").unwrap();
    assert_eq!(
        fs::read_to_string(archive.join("empirical-2.json")).unwrap(),
        "second version"
    );
}

#[test]
fn prompt_parsimony_migration_refreshes_only_stock_grant_orientation() {
    let dir = tempfile::tempdir().unwrap();
    let mut stock = grant_review_profile();
    stock.orientation_prompt = crate::prompts::compiled_default("orientation_generic")
        .unwrap()
        .to_string();
    let stock_path = dir.path().join("grant-review.json");
    fs::write(&stock_path, serde_json::to_vec_pretty(&stock).unwrap()).unwrap();

    crate::pipeline_config::migrations::refresh_parsimonious_prompt_defaults(dir.path()).unwrap();

    let refreshed: ProfileData = serde_json::from_slice(&fs::read(&stock_path).unwrap()).unwrap();
    assert_eq!(
        refreshed.orientation_prompt,
        crate::prompts::compiled_default("orientation_grant").unwrap()
    );

    let mut customized = stock;
    customized
        .orientation_prompt
        .push_str("\nCustom survey rule.");
    fs::write(&stock_path, serde_json::to_vec_pretty(&customized).unwrap()).unwrap();

    crate::pipeline_config::migrations::refresh_parsimonious_prompt_defaults(dir.path()).unwrap();

    let preserved: ProfileData = serde_json::from_slice(&fs::read(&stock_path).unwrap()).unwrap();
    assert_eq!(preserved.orientation_prompt, customized.orientation_prompt);
}

#[test]
fn stale_builtin_auto_review_contract_is_replaced_before_validation() {
    let dir = tempfile::tempdir().unwrap();
    let mut stale = auto_review_profile();
    stale.steps[0].prompt.push_str("\nKeep this user edit.");
    stale.orientation_prompt = "Retired router prompt".to_string();
    stale.orientation_schema.as_mut().unwrap()["x-pipeline-contract"] =
        serde_json::json!("auto-review-v99");
    let path = dir.path().join("auto-paper-review.json");
    fs::write(&path, serde_json::to_vec_pretty(&stale).unwrap()).unwrap();

    refresh_builtin_auto_review_contracts(dir.path()).unwrap();

    let repaired: ProfileData = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    validate_profile_data(&repaired).unwrap();
    assert_eq!(
        repaired
            .orientation_schema
            .as_ref()
            .and_then(|schema| schema.get("x-pipeline-contract"))
            .and_then(serde_json::Value::as_str),
        Some(crate::auto_review::AUTO_REVIEW_CONTRACT)
    );
    assert_eq!(
        repaired.orientation_prompt,
        crate::auto_review::orientation_prompt()
    );
    assert!(repaired.steps[0].prompt.ends_with("Keep this user edit."));
}

#[test]
fn pre_v2_builtin_auto_review_suite_is_archived_and_recreated() {
    let dir = tempfile::tempdir().unwrap();
    let mut stale = auto_review_profile();
    stale.steps[0].prompt = "Old Markdown reviewer prompt".to_string();
    stale.steps[0].output_schema = None;
    stale.orientation_schema.as_mut().unwrap()["x-pipeline-contract"] =
        serde_json::json!("auto-review-v1");
    let path = dir.path().join("auto-paper-review.json");
    fs::write(&path, serde_json::to_vec_pretty(&stale).unwrap()).unwrap();

    crate::pipeline_config::migrations::refresh_stale_auto_review_suites(dir.path()).unwrap();

    // The user's copy is preserved in the archive; the active file is the
    // current structured suite.
    assert!(dir
        .path()
        .join(".retired-builtins/auto-paper-review.json")
        .exists());
    let replaced: ProfileData = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    validate_profile_data(&replaced).unwrap();
    assert_eq!(
        replaced
            .orientation_schema
            .as_ref()
            .and_then(|schema| schema.get("x-pipeline-contract"))
            .and_then(serde_json::Value::as_str),
        Some(crate::auto_review::AUTO_REVIEW_CONTRACT)
    );
    assert!(replaced.steps[0].output_schema.is_some());

    // A current-contract suite is left byte-for-byte unchanged.
    let bytes = fs::read(&path).unwrap();
    crate::pipeline_config::migrations::refresh_stale_auto_review_suites(dir.path()).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn current_builtin_auto_review_contract_is_left_byte_for_byte_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let mut current = auto_review_profile();
    current.steps[0]
        .prompt
        .push_str("\nKeep this current-contract edit.");
    let bytes = serde_json::to_vec_pretty(&current).unwrap();
    let path = dir.path().join("auto-paper-review.json");
    fs::write(&path, &bytes).unwrap();

    refresh_builtin_auto_review_contracts(dir.path()).unwrap();

    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn expanded_builtin_auto_review_prompt_is_compacted_without_touching_custom_prompts() {
    let dir = tempfile::tempdir().unwrap();
    let template = crate::auto_review::orientation_prompt();
    let mut stock = auto_review_profile();
    stock.orientation_prompt = crate::auto_review::expand_orientation_prompt(&template);
    let stock_path = dir.path().join("auto-paper-review.json");
    fs::write(&stock_path, serde_json::to_vec_pretty(&stock).unwrap()).unwrap();

    let mut custom = quick_auto_review_profile();
    custom.orientation_prompt = "Custom compact router {paper_text}".to_string();
    let custom_bytes = serde_json::to_vec_pretty(&custom).unwrap();
    let custom_path = dir.path().join("auto-paper-review-fast.json");
    fs::write(&custom_path, &custom_bytes).unwrap();

    compact_builtin_auto_review_prompts(dir.path()).unwrap();

    let compacted: ProfileData = serde_json::from_slice(&fs::read(&stock_path).unwrap()).unwrap();
    assert_eq!(compacted.orientation_prompt, template);
    assert_eq!(fs::read(custom_path).unwrap(), custom_bytes);
}

fn mark_all_catalog_migrations_except(dir: &Path, skipped: &str) {
    for marker in CATALOG_MIGRATION_MARKERS {
        if *marker != skipped {
            fs::write(dir.join(marker), b"recorded\n").unwrap();
        }
    }
}

#[test]
fn a_builtin_that_fails_current_validation_does_not_block_the_catalog() {
    let dir = tempfile::tempdir().unwrap();
    mark_all_catalog_migrations_except(dir.path(), ".builtin-catalog-v6");
    // A 0.9.x-era edit that current rules reject: an undeclared variable.
    let mut profile = auto_review_profile();
    profile.steps[0]
        .prompt
        .push_str("\nUse the {var:journal} house style.");
    assert!(validate_profile_data(&profile)
        .unwrap_err()
        .contains("undeclared variable"));
    let path = dir.path().join("auto-paper-review.json");
    let original = serde_json::to_vec_pretty(&profile).unwrap();
    fs::write(&path, &original).unwrap();

    migrate_builtin_catalog(dir.path()).unwrap();

    assert!(dir.path().join(".builtin-catalog-v6").exists());
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn the_shipped_free_form_grant_suite_is_replaced_by_the_typed_suite() {
    let dir = tempfile::tempdir().unwrap();
    mark_all_catalog_migrations_except(dir.path(), ".structured-grant-review-contract-v2");
    let current = grant_review_profile();
    let legacy_steps = current
        .steps
        .iter()
        .filter(|step| step.id != "grant_validate")
        .map(|step| StepConfig {
            output_schema: None,
            ..step.clone()
        })
        .collect::<Vec<_>>();
    let legacy = ProfileData::new(
        "Grant Proposal Review",
        legacy_steps,
        MergeConfig::default(),
    );
    let path = dir.path().join("grant-review.json");
    let original = serde_json::to_vec_pretty(&legacy).unwrap();
    fs::write(&path, &original).unwrap();

    migrate_builtin_catalog(dir.path()).unwrap();

    let replaced: ProfileData = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let validate = replaced
        .steps
        .iter()
        .find(|step| step.id == "grant_validate")
        .expect("typed validation step installed");
    assert_eq!(
        validate.output_schema.as_ref().unwrap()[crate::pipeline::structured::SCHEMA_REFERENCE_KEY],
        "findings-v2-validation"
    );
    assert_eq!(
        fs::read(dir.path().join(".retired-builtins/grant-review.json")).unwrap(),
        original
    );
    assert!(dir
        .path()
        .join(".structured-grant-review-contract-v2")
        .exists());
}

#[test]
fn renamed_builtins_move_with_their_edits_and_only_stock_names_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut full = auto_review_profile();
    full.name = "Automatic Paper Review (Full)".into();
    full.steps[0].prompt.push_str("\nCustom instruction.");
    fs::write(
        dir.path().join("auto-review.json"),
        serde_json::to_vec_pretty(&full).unwrap(),
    )
    .unwrap();
    let mut quick = quick_auto_review_profile();
    quick.name = "My quick review".into();
    let quick_bytes = serde_json::to_vec_pretty(&quick).unwrap();
    fs::write(dir.path().join("auto-review-quick.json"), &quick_bytes).unwrap();

    migrate_renamed_builtin_ids(dir.path()).unwrap();

    assert!(!dir.path().join("auto-review.json").exists());
    assert!(!dir.path().join("auto-review-quick.json").exists());
    let moved: ProfileData =
        serde_json::from_slice(&fs::read(dir.path().join("auto-paper-review.json")).unwrap())
            .unwrap();
    assert_eq!(moved.name, "Paper Review");
    assert!(moved.steps[0].prompt.ends_with("Custom instruction."));
    // A customized name is not a stock name, so the file moves byte-for-byte.
    assert_eq!(
        fs::read(dir.path().join("auto-paper-review-fast.json")).unwrap(),
        quick_bytes
    );
    assert!(dir.path().join(".paper-review-ids-v1").exists());
    assert!(CATALOG_MIGRATION_MARKERS.contains(&".paper-review-ids-v1"));

    // One-time: a later custom profile may reuse a former ID.
    fs::write(dir.path().join("auto-review.json"), &quick_bytes).unwrap();
    migrate_renamed_builtin_ids(dir.path()).unwrap();
    assert_eq!(
        fs::read(dir.path().join("auto-review.json")).unwrap(),
        quick_bytes
    );
}

#[test]
fn renamed_builtin_never_overwrites_an_existing_destination() {
    let dir = tempfile::tempdir().unwrap();
    let source = serde_json::to_vec_pretty(&full_review_profile(true)).unwrap();
    let destination = serde_json::to_vec_pretty(&auto_review_profile()).unwrap();
    fs::write(dir.path().join("auto-review.json"), &source).unwrap();
    fs::write(dir.path().join("auto-paper-review.json"), &destination).unwrap();

    migrate_renamed_builtin_ids(dir.path()).unwrap();

    assert_eq!(
        fs::read(dir.path().join("auto-paper-review.json")).unwrap(),
        destination
    );
    assert!(!dir.path().join("auto-review.json").exists());
    assert_eq!(
        fs::read(dir.path().join(".retired-builtins/auto-review.json")).unwrap(),
        source
    );
}

#[test]
fn renamed_ids_resolve_until_a_file_claims_them() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        resolve_renamed_profile_id(dir.path(), "auto-review"),
        "auto-paper-review"
    );
    assert_eq!(
        resolve_renamed_profile_id(dir.path(), "auto-review-quick"),
        "auto-paper-review-fast"
    );
    assert_eq!(resolve_renamed_profile_id(dir.path(), "mine"), "mine");
    fs::write(dir.path().join("auto-review.json"), b"{}").unwrap();
    assert_eq!(
        resolve_renamed_profile_id(dir.path(), "auto-review"),
        "auto-review"
    );
}

#[test]
fn catalog_migrations_never_rewrite_the_stock_slide_review() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("slide-review.json");
    let stock = serde_json::to_vec_pretty(&slide_review_profile()).unwrap();
    fs::write(&path, &stock).unwrap();

    // No markers: every version-marked upgrade runs, including the v6
    // artifact-context refresh that is destructive on a current profile.
    migrate_builtin_catalog(dir.path()).unwrap();

    assert_eq!(fs::read(&path).unwrap(), stock);
}

struct LegacyStore {
    _directory: tempfile::TempDir,
    root: std::path::PathBuf,
    profiles: std::path::PathBuf,
}

fn legacy_store(name: &str, content: &str) -> LegacyStore {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_path_buf();
    let profiles = root.join("profiles");
    fs::create_dir(&profiles).unwrap();
    fs::write(root.join(name), content).unwrap();
    LegacyStore {
        _directory: directory,
        root,
        profiles,
    }
}

const REFEREE: &str =
    r#"{"id":"methods","label":"Methods","prompt":"Check the methods.","enabled":true}"#;

#[test]
fn a_legacy_configuration_without_a_name_migrates_and_its_file_goes_only_then() {
    let store = legacy_store("pipeline.json", &format!(r#"{{"referees":[{REFEREE}]}}"#));
    migrate_legacy_files(&store.root, &store.profiles, false).unwrap();
    let saved: ProfileData =
        serde_json::from_slice(&fs::read(store.profiles.join("migrated.json")).unwrap()).unwrap();
    assert_eq!(saved.name, "Migrated");
    assert_eq!(saved.steps[0].id, "methods");
    assert_eq!(saved.steps[0].prompt, "Check the methods.");
    assert!(!store.root.join("pipeline.json").exists());
}

#[test]
fn a_legacy_configuration_that_cannot_be_read_is_archived_and_survives_the_next_launch() {
    for content in [
        "not json at all",
        r#"{"steps":[{"id":"methods"}]}"#,
        r#"{"name":"Empty"}"#,
    ] {
        let store = legacy_store("pipeline.json", content);
        migrate_legacy_files(&store.root, &store.profiles, false).unwrap();
        migrate_legacy_files(&store.root, &store.profiles, true).unwrap();
        assert!(!store.profiles.join("migrated.json").exists());
        assert_eq!(
            fs::read_to_string(store.profiles.join(".retired-builtins/pipeline.json")).unwrap(),
            content
        );
    }
}

#[test]
fn an_initialized_store_archives_a_leftover_configuration_instead_of_deleting_it() {
    let store = legacy_store("pipeline.json", "my old settings");
    migrate_legacy_files(&store.root, &store.profiles, true).unwrap();
    assert!(!store.root.join("pipeline.json").exists());
    assert_eq!(
        fs::read(store.profiles.join(".retired-builtins/pipeline.json")).unwrap(),
        b"my old settings"
    );
}

#[test]
fn a_legacy_configuration_that_fails_validation_never_blocks_startup() {
    let bad = r#"{"id":"not a valid id!","label":"Bad","prompt":"Keep me.","enabled":true}"#;
    for (name, content) in [
        (
            "pipeline.json",
            format!(r#"{{"name":"Old","referees":[{bad}]}}"#),
        ),
        ("referees.json", format!("[{bad}]")),
    ] {
        let store = legacy_store(name, &content);
        migrate_legacy_files(&store.root, &store.profiles, false).unwrap();
        assert!(!store.profiles.join("migrated.json").exists());
        assert!(!store.root.join(name).exists());
        assert_eq!(
            fs::read_to_string(store.profiles.join(".retired-builtins").join(name)).unwrap(),
            content
        );
    }
}

#[test]
fn a_legacy_configuration_never_replaces_a_migrated_profile() {
    let store = legacy_store("pipeline.json", &format!(r#"{{"referees":[{REFEREE}]}}"#));
    fs::write(store.root.join("referees.json"), format!("[{REFEREE}]")).unwrap();
    fs::write(store.profiles.join("migrated.json"), "saved earlier").unwrap();
    fs::write(store.profiles.join("default.json"), "an old default").unwrap();
    migrate_legacy_files(&store.root, &store.profiles, false).unwrap();
    assert_eq!(
        fs::read(store.profiles.join("migrated.json")).unwrap(),
        b"saved earlier"
    );
    let archive = store.profiles.join(".retired-builtins");
    assert!(archive.join("pipeline.json").is_file());
    assert!(archive.join("referees.json").is_file());
    assert_eq!(
        fs::read(archive.join("default.json")).unwrap(),
        b"an old default"
    );
}

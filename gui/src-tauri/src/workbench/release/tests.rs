//! Tests spanning release recipes, handoffs, evaluation, and portability.

use super::*;
use crate::workbench::research::{self, ImportPaperRequest};
use crate::workbench::store::{
    CreateSessionRequest, CreateWorkspaceRequest, RegisterWorkspaceRootRequest,
    UpdateSessionRequest,
};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::Read as _;

fn store() -> (tempfile::TempDir, Store, String, String) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let workspace = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Research".into(),
            root: None,
            operation_id: "create_workspace".into(),
        })
        .unwrap()
        .record;
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: Some(workspace.id.clone()),
            title: "Audit".into(),
            operation_id: "create_session".into(),
        })
        .unwrap()
        .record;
    (temp, store, workspace.id, session.id)
}

#[test]
fn built_in_recipes_ship_versioned_and_plain_conversation_is_not_one() {
    let (_temp, store, workspace, _session) = store();
    let recipes = list_recipes(&store, Some(&workspace)).unwrap();
    assert_eq!(recipes.len(), 9);
    for id in [
        "limiting_cases",
        "dimensional_check",
        "accounting_identities",
        "comparative_statics",
        "numerical_counterexample",
    ] {
        assert!(recipes.iter().any(|recipe| recipe.id == id), "{id}");
    }
    assert!(recipes
        .iter()
        .all(|recipe| recipe.version == 1 && !recipe.instructions.is_empty()));
    assert!(!recipes.iter().any(|recipe| recipe.id == "plain"));
}

#[test]
fn required_inputs_remain_visible_and_recipe_start_is_idempotent() {
    let (_temp, store, _workspace, session) = store();
    let checks = check_recipe_inputs(&store, &session, "empirical_result_audit").unwrap();
    assert!(checks.iter().any(|check| !check.available));
    let request = StartRecipeRequest {
        session_id: session,
        recipe_id: "empirical_result_audit".into(),
        operation_id: "start_recipe".into(),
    };
    let first = start_recipe(&store, request.clone()).unwrap();
    let second = start_recipe(&store, request).unwrap();
    assert_eq!(first.id, second.id);
    assert!(!first.missing_evidence.is_empty());
}

#[test]
fn recipe_idempotency_key_cannot_be_reused_for_different_inputs() {
    let (_temp, store, _workspace, session) = store();
    start_recipe(
        &store,
        StartRecipeRequest {
            session_id: session.clone(),
            recipe_id: "empirical_result_audit".into(),
            operation_id: "one_recipe_operation".into(),
        },
    )
    .unwrap();
    let error = start_recipe(
        &store,
        StartRecipeRequest {
            session_id: session,
            recipe_id: "theory_audit_recipe".into(),
            operation_id: "one_recipe_operation".into(),
        },
    )
    .unwrap_err();
    assert!(error.message.contains("different inputs"));
}

#[test]
fn recipe_completion_is_derived_from_the_run_snapshot_and_current_inputs() {
    let (_temp, store, _workspace, session) = store();
    let run = start_recipe(
        &store,
        StartRecipeRequest {
            session_id: session,
            recipe_id: "theory_audit_recipe".into(),
            operation_id: "complete_recipe".into(),
        },
    )
    .unwrap();
    let checks = run
        .recipe
        .expected_checks
        .iter()
        .map(|name| json!({"name": name, "status": "recorded"}))
        .collect();
    let completed = complete_recipe(
        &store,
        CompleteRecipeRequest {
            recipe_run_id: run.id,
            artifacts: Vec::new(),
            checks,
            unresolved_issues: Vec::new(),
            missing_evidence: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(completed.status, "incomplete");
    assert!(completed
        .missing_evidence
        .iter()
        .any(|item| item.contains("paper")));
}

#[test]
fn recipe_completion_rejects_unrecorded_expected_checks() {
    let (_temp, store, _workspace, session) = store();
    let run = start_recipe(
        &store,
        StartRecipeRequest {
            session_id: session,
            recipe_id: "theory_audit_recipe".into(),
            operation_id: "incomplete_check_set".into(),
        },
    )
    .unwrap();
    let error = complete_recipe(
        &store,
        CompleteRecipeRequest {
            recipe_run_id: run.id,
            artifacts: Vec::new(),
            checks: Vec::new(),
            unresolved_issues: Vec::new(),
            missing_evidence: Vec::new(),
        },
    )
    .unwrap_err();
    assert!(error.message.contains("every expected check"));
}

#[test]
fn custom_recipe_lists_have_per_item_bounds() {
    let (_temp, store, workspace, _session) = store();
    let recipe = clone_recipe(
        &store,
        CloneRecipeRequest {
            workspace_id: workspace,
            source_recipe_id: "manuscript_consistency".into(),
            name: "Bounded recipe".into(),
        },
    )
    .unwrap();
    let error = update_recipe(
        &store,
        UpdateRecipeRequest {
            recipe_id: recipe.id,
            expected_revision: recipe.revision,
            name: recipe.name,
            description: recipe.description,
            instructions: recipe.instructions,
            required_inputs: vec!["x".repeat(201)],
            required_tools: recipe.required_tools,
            suggested_permission_mode: recipe.suggested_permission_mode,
            expected_checks: recipe.expected_checks,
        },
    )
    .unwrap_err();
    assert!(error.message.contains("Required inputs"));
}

#[test]
fn custom_recipe_lists_reject_duplicates_that_make_completion_ambiguous() {
    let (_temp, store, workspace, _session) = store();
    let recipe = clone_recipe(
        &store,
        CloneRecipeRequest {
            workspace_id: workspace,
            source_recipe_id: "manuscript_consistency".into(),
            name: "Unique checks".into(),
        },
    )
    .unwrap();
    let error = update_recipe(
        &store,
        UpdateRecipeRequest {
            recipe_id: recipe.id,
            expected_revision: recipe.revision,
            name: recipe.name,
            description: recipe.description,
            instructions: recipe.instructions,
            required_inputs: recipe.required_inputs,
            required_tools: recipe.required_tools,
            suggested_permission_mode: recipe.suggested_permission_mode,
            expected_checks: vec!["units".into(), " units ".into()],
        },
    )
    .unwrap_err();
    assert!(error.message.contains("unique"));
}

#[test]
fn selected_recipe_enters_the_immutable_effective_harness() {
    let (_temp, store, _workspace, session) = store();
    store
        .update_session(UpdateSessionRequest {
            session_id: session.clone(),
            expected_revision: 1,
            operation_id: "select_recipe".into(),
            title: None,
            draft: None,
            overrides: Some(json!({"recipeId":"theory_audit_recipe"})),
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
    let effective = crate::workbench::research::resolve_harness(&store, &session).unwrap();
    assert!(effective
        .developer_instructions
        .contains("Optional research recipe"));
    assert!(effective.developer_instructions.contains("limiting cases"));
    assert!(effective
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("missing required inputs")));
}

#[test]
fn performance_samples_use_recorded_budgets() {
    let (_temp, store, _, _) = store();
    let value = record_performance(
        &store,
        RecordPerformanceRequest {
            metric: "warm_conversation_open".into(),
            observed_value: 251.0,
            details: json!({}),
        },
    )
    .unwrap();
    assert_eq!(value["passed"], false);
}

#[test]
fn evaluation_records_keep_comparable_variant_and_usage_fields() {
    let (_temp, store, _, _) = store();
    let recorded = record_evaluation(
        &store,
        RecordEvaluationRequest {
            fixture_id: "sample_unit_mismatch".into(),
            fixture_version: 1,
            variant: "plain_workspace".into(),
            model: Some("qualified-model".into()),
            settings: json!({"effort":"high"}),
            outcome: json!({"correctness":2,"traceability":1}),
            latency_ms: Some(1200),
            input_tokens: Some(100),
            output_tokens: Some(50),
        },
    )
    .unwrap();
    assert_eq!(recorded.variant, "plain_workspace");
    assert_eq!(recorded.output_tokens, Some(50));
    assert_eq!(
        list_evaluations(&store, Some("sample_unit_mismatch"))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn review_handoff_stages_one_immutable_revision_idempotently() {
    let (_temp, store, workspace, session) = store();
    let input_dir = tempfile::tempdir().unwrap();
    let input = input_dir.path().join("paper.tex");
    fs::write(&input, "\\section{Model} Exact revision").unwrap();
    let paper = crate::workbench::research::import_paper(
        &store,
        ImportPaperRequest {
            workspace_id: workspace.clone(),
            paper_id: None,
            title: "Paper".into(),
            role: "manuscript".into(),
            path: input.to_string_lossy().into_owned(),
            operation_id: "import_paper".into(),
        },
    )
    .unwrap();
    let request = PrepareReviewHandoffRequest {
        workspace_id: workspace,
        session_id: Some(session),
        paper_id: paper.paper.id,
        metadata: json!({"label":"candidate"}),
        operation_id: "prepare_handoff".into(),
    };
    let first = prepare_review_handoff(&store, request.clone()).unwrap();
    let second = prepare_review_handoff(&store, request).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(first.version, 1);
    assert_eq!(first.input_interpretation, "document");
    assert!(Path::new(&first.staged_path).is_file());
    assert_ne!(first.staged_path, input.to_string_lossy());

    let mismatch = prepare_review_handoff(
        &store,
        PrepareReviewHandoffRequest {
            metadata: json!({"label":"different"}),
            operation_id: "prepare_handoff".into(),
            workspace_id: first.workspace_id.clone(),
            session_id: first.session_id.clone(),
            paper_id: first.paper_id.clone(),
        },
    )
    .unwrap_err();
    assert!(mismatch.message.contains("different inputs"));
}

#[test]
fn archive_round_trip_is_consistent_and_retires_native_bindings() {
    let (_source_temp, source, workspace, session) = store();
    let input_dir = tempfile::tempdir().unwrap();
    let input = input_dir.path().join("paper.txt");
    fs::write(&input, "A portable immutable manuscript.").unwrap();
    let paper = crate::workbench::research::import_paper(
        &source,
        ImportPaperRequest {
            workspace_id: workspace.clone(),
            paper_id: None,
            title: "Portable paper".into(),
            role: "manuscript".into(),
            path: input.to_string_lossy().into_owned(),
            operation_id: "portable_paper".into(),
        },
    )
    .unwrap();
    let revision_id = paper.revision.unwrap().id;
    let tree = input_dir.path().join("tex-project");
    fs::create_dir_all(&tree).unwrap();
    fs::write(tree.join("main.tex"), "\\input{section}").unwrap();
    fs::write(tree.join("section.tex"), "Portable source tree.").unwrap();
    crate::workbench::research::import_paper(
        &source,
        ImportPaperRequest {
            workspace_id: workspace.clone(),
            paper_id: None,
            title: "Portable tree".into(),
            role: "appendix".into(),
            path: tree.to_string_lossy().into_owned(),
            operation_id: "portable_tree".into(),
        },
    )
    .unwrap();
    let agent_profile = research::clone_preset(
        &source,
        research::ClonePresetRequest {
            workspace_id: None,
            source_workspace_id: None,
            source_preset_id: "writing".into(),
            name: "Portable writer".into(),
            operation_id: "portable-agent-profile".into(),
        },
    )
    .unwrap();
    let binding = source
        .bind_session(&session, "test-runtime", "thread-portable")
        .unwrap();
    source.connection().unwrap().execute(
            "INSERT INTO transcript_items (id,binding_id,turn_id,provider_item_id,item_kind,payload_json,is_final,created_at,updated_at) VALUES ('portable-item',?1,NULL,'provider-portable-item','agentMessage',?2,1,'2026-09-06T00:00:00Z','2026-09-06T00:00:00Z')",
            params![binding.id, json!({"content":[{"type":"output_text","text":"Array-shaped portable message"}]}).to_string()],
        ).unwrap();
    let collection = crate::workbench::desk::insert(
        &source,
        &workspace,
        "collection",
        "Retained references",
        json!({"objects":[]}),
        None,
        "desk-collection",
    )
    .unwrap();
    // Imported immutable records survive, but local execution authority never does.
    source.connection().unwrap().execute("INSERT INTO execution_plan_state(plan_id,fingerprint,authorized,test_status) VALUES(?1,?2,1,'passed')",params![collection.id,collection.content_hash]).unwrap();
    while !crate::workbench::search::advance(&source, &workspace)
        .unwrap()
        .complete
    {}
    let archive_dir = tempfile::tempdir().unwrap();
    let archive = archive_dir.path().join("portable.pwrx");
    let report = export_archive(
        &source,
        ExportArchiveRequest {
            path: archive.to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    assert_eq!(report.workspace_count, 1);
    let inspection = inspect_archive(ExportArchiveRequest {
        path: archive.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(inspection.format_version, 1);
    assert!(inspection.workspace_roots.is_empty());
    let mut zip = zip::ZipArchive::new(File::open(&archive).unwrap()).unwrap();
    let mut manifest_text = String::new();
    zip.by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut manifest_text)
        .unwrap();
    assert!(manifest_text.contains("workspace://blobs"));
    assert!(!manifest_text.contains(&source.root_path().to_string_lossy().into_owned()));
    let mut transcript_json = String::new();
    zip.by_name(&format!("transcripts/{session}.json"))
        .unwrap()
        .read_to_string(&mut transcript_json)
        .unwrap();
    let transcript: Value = serde_json::from_str(&transcript_json).unwrap();
    assert_eq!(transcript["schemaVersion"], 1);
    assert_eq!(
        transcript["items"][0]["payload"]["content"][0]["text"],
        "Array-shaped portable message"
    );
    let mut transcript_markdown = String::new();
    zip.by_name(&format!("transcripts/{session}.md"))
        .unwrap()
        .read_to_string(&mut transcript_markdown)
        .unwrap();
    assert!(transcript_markdown.contains("Array-shaped portable message"));
    drop(zip);
    let destination_temp = tempfile::tempdir().unwrap();
    let destination = Store::open_at(destination_temp.path()).unwrap();
    let restored = import_archive(
        &destination,
        ImportArchiveRequest {
            path: archive.to_string_lossy().into_owned(),
            root_mappings: BTreeMap::new(),
        },
    )
    .unwrap();
    assert_eq!(restored.workspace_count, 1);
    let restored_profile = research::harness_catalog(&destination, None)
        .unwrap()
        .presets
        .into_iter()
        .find(|profile| profile.id == agent_profile.id)
        .unwrap();
    assert_eq!(restored_profile, agent_profile);
    let restored_collection =
        crate::workbench::desk::record(&destination, &workspace, &collection.id).unwrap();
    assert_eq!(restored_collection.content_hash, collection.content_hash);
    let grants: i64 = destination
        .connection()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM execution_plan_state", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(grants, 0);
    assert!(
        !crate::workbench::search::status(&destination, &workspace)
            .unwrap()
            .complete
    );
    while !crate::workbench::search::advance(&destination, &workspace)
        .unwrap()
        .complete
    {}
    assert!(crate::workbench::search::read_object(
        &destination,
        &workspace,
        &collection.reference(),
        1000
    )
    .is_ok());

    assert_eq!(destination.workspace(&workspace).unwrap().name, "Research");
    assert_eq!(
        destination
            .session_snapshot(&session)
            .unwrap()
            .session
            .title,
        "Audit"
    );
    let restored_text = crate::workbench::research::paper_read(
        &destination,
        crate::workbench::research::PaperReadRequest {
            workspace_id: workspace.clone(),
            revision_id,
            start: Some(0),
            length: Some(1024),
        },
    )
    .unwrap();
    assert!(restored_text.text.contains("portable immutable manuscript"));
    let restored_tree: String = destination
            .connection()
            .unwrap()
            .query_row(
                "SELECT a.storage_reference FROM artifacts a WHERE a.workspace_id=?1 AND a.media_kind='source_tree'",
                [&workspace],
                |row| row.get(0),
            )
            .unwrap();
    assert!(Path::new(&restored_tree).starts_with(destination.root_path().join("blobs")));
    assert!(Path::new(&restored_tree).is_dir());
}

#[test]
fn archive_import_rejects_id_collisions() {
    let (_temp, source, _, _) = store();
    let archive_dir = tempfile::tempdir().unwrap();
    let archive = archive_dir.path().join("portable.pwrx");
    export_archive(
        &source,
        ExportArchiveRequest {
            path: archive.to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    let error = import_archive(
        &source,
        ImportArchiveRequest {
            path: archive.to_string_lossy().into_owned(),
            root_mappings: BTreeMap::new(),
        },
    )
    .unwrap_err();
    assert!(error.message.contains("collide"));
}

#[test]
fn archive_import_requires_a_decision_for_every_registered_root() {
    let (_source_temp, source, workspace, _session) = store();
    let registered_root = tempfile::tempdir().unwrap();
    let registered = source
        .register_workspace_root(RegisterWorkspaceRootRequest {
            workspace_id: workspace.clone(),
            root: registered_root.path().to_string_lossy().into_owned(),
            operation_id: "register_portable_root".into(),
            expected_revision: 1,
        })
        .unwrap()
        .record;
    let archived_root = registered.root.unwrap();
    let archive_dir = tempfile::tempdir().unwrap();
    let archive = archive_dir.path().join("rooted.pwrx");
    export_archive(
        &source,
        ExportArchiveRequest {
            path: archive.to_string_lossy().into_owned(),
        },
    )
    .unwrap();

    let destination_temp = tempfile::tempdir().unwrap();
    let destination = Store::open_at(destination_temp.path()).unwrap();
    let error = import_archive(
        &destination,
        ImportArchiveRequest {
            path: archive.to_string_lossy().into_owned(),
            root_mappings: BTreeMap::new(),
        },
    )
    .unwrap_err();
    assert!(error.message.contains("every archived Workspace root"));

    let restored = import_archive(
        &destination,
        ImportArchiveRequest {
            path: archive.to_string_lossy().into_owned(),
            root_mappings: BTreeMap::from([(archived_root, None)]),
        },
    )
    .unwrap();
    assert_eq!(restored.workspace_count, 1);
    assert!(destination.workspace(&workspace).unwrap().root.is_none());
}

#[test]
fn unsafe_archive_names_are_rejected() {
    assert!(!archive::safe_archive_name("../database"));
    assert!(!archive::safe_archive_name("/database"));
    assert!(!archive::safe_archive_name("a\\b"));
    assert!(archive::safe_archive_name("database/research.sqlite3"));
}

#[test]
fn evaluation_fixture_has_twenty_known_recipe_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../docs/workbench/research-evaluation-v1.json"
    ))
    .unwrap();
    assert_eq!(fixture["schemaVersion"], 1);
    let cases = fixture["fixtures"].as_array().unwrap();
    assert_eq!(cases.len(), 20);
    let recipes = builtin_recipes()
        .into_iter()
        .map(|recipe| recipe.id)
        .collect::<HashSet<_>>();
    assert!(cases.iter().all(|case| case
        .get("recipe")
        .and_then(Value::as_str)
        .is_some_and(|recipe| recipes.contains(recipe))));
}

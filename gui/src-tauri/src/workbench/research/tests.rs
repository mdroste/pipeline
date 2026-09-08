//! Cross-domain tests for the modular research harness.

use super::*;
use crate::workbench::store::{CreateSessionRequest, CreateWorkspaceRequest, UpdateSessionRequest};

struct Fixture {
    _temporary: tempfile::TempDir,
    store: Store,
    root: PathBuf,
    workspace_id: String,
    session_id: String,
}

fn fixture() -> Fixture {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("research-root");
    fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temporary.path().join("store")).unwrap();
    let workspace = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Research".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create-workspace".into(),
        })
        .unwrap()
        .record;
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: Some(workspace.id.clone()),
            title: "Question".into(),
            operation_id: "create-session".into(),
        })
        .unwrap()
        .record;
    Fixture {
        _temporary: temporary,
        store,
        root,
        workspace_id: workspace.id,
        session_id: session.id,
    }
}

fn select_preset(fixture: &Fixture, preset: &str, mode: &str) {
    let session = fixture
        .store
        .session_snapshot(&fixture.session_id)
        .unwrap()
        .session;
    fixture
        .store
        .update_session(UpdateSessionRequest {
            session_id: session.id,
            expected_revision: session.revision,
            operation_id: format!("select-{preset}"),
            title: None,
            draft: None,
            overrides: Some(json!({"mode":mode})),
            archived: None,
            preset_id: Some(preset.into()),
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
}

#[test]
fn harness_is_inherited_snapshotted_and_capability_gated() {
    let fixture = fixture();
    save_config(
        &fixture.store,
        SaveWorkspaceConfigRequest {
            workspace_id: Some(fixture.workspace_id.clone()),
            expected_revision: Some(0),
            body: json!({"contextBudgetBytes":16384,"commandNetwork":true}),
            operation_id: "save-config".into(),
        },
    )
    .unwrap();
    create_note(
        &fixture.store,
        CreateNoteRequest {
            workspace_id: fixture.workspace_id.clone(),
            paper_id: None,
            kind: "question".into(),
            body: "What is the sufficient statistic?".into(),
            state: Some("accepted".into()),
            origin: "user".into(),
            pinned: true,
            operation_id: "note-one".into(),
        },
        false,
    )
    .unwrap();
    select_preset(&fixture, "empirical_audit", "edit");
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_eq!(effective.context_budget_bytes, 16_384);
    assert!(effective.command_network);
    assert!(effective.context_preview.contains("sufficient statistic"));
    assert!(effective
        .unavailable_modules
        .contains(&"research_execution".to_string()));
    assert!(!effective
        .dynamic_tools
        .iter()
        .any(|tool| tool["name"] == "workbench_research_run"));
    let _prepared = prepare_turn(&fixture.store, &fixture.session_id).unwrap();
    assert!(fs::read_dir(fixture.store.root_path().join("context"))
        .unwrap()
        .next()
        .is_some());
}

#[test]
fn paper_revisions_preserve_locators_and_stale_prior_evidence() {
    let fixture = fixture();
    let paper_path = fixture.root.join("paper.tex");
    fs::write(&paper_path, "Output rises with productivity.\n").unwrap();
    let first = import_paper(
        &fixture.store,
        ImportPaperRequest {
            workspace_id: fixture.workspace_id.clone(),
            paper_id: None,
            title: "Paper".into(),
            role: "manuscript".into(),
            path: paper_path.to_string_lossy().into_owned(),
            operation_id: "paper-one".into(),
        },
    )
    .unwrap();
    let first_revision = first.revision.unwrap();
    let hit = paper_search(
        &fixture.store,
        &fixture.workspace_id,
        &first_revision.id,
        "productivity",
        10,
    )
    .unwrap()
    .remove(0);
    assert_eq!(hit.excerpt, "Output rises with productivity.");
    let claim = propose_claim(
        &fixture.store,
        ProposeClaimRequest {
            workspace_id: fixture.workspace_id.clone(),
            paper_id: Some(first.paper.id.clone()),
            claim: "Productivity raises output.".into(),
            kind: "mechanism".into(),
            origin: "user".into(),
            operation_id: "claim-one".into(),
        },
        false,
    )
    .unwrap();
    let evidence = propose_evidence(
        &fixture.store,
        ProposeEvidenceRequest {
            workspace_id: fixture.workspace_id.clone(),
            claim_version_id: claim.version_id,
            target_type: "paper_revision".into(),
            target_id: first_revision.id.clone(),
            locator: Some(json!({"start":hit.start,"end":hit.end})),
            relation: "supports".into(),
            assessor: "model:test".into(),
            operation_id: "evidence-one".into(),
        },
        true,
    )
    .unwrap();
    assert_eq!(evidence.assessment, "model_assessed");
    let confirmed = confirm_evidence(
        &fixture.store,
        ConfirmEvidenceRequest {
            evidence_id: evidence.id,
            operation_id: "confirm-one".into(),
        },
    )
    .unwrap();
    assert_eq!(confirmed.assessment, "human_confirmed");
    fs::write(&paper_path, "Output may fall with productivity.\n").unwrap();
    let second = import_paper(
        &fixture.store,
        ImportPaperRequest {
            workspace_id: fixture.workspace_id.clone(),
            paper_id: Some(first.paper.id),
            title: "Paper".into(),
            role: "manuscript".into(),
            path: paper_path.to_string_lossy().into_owned(),
            operation_id: "paper-two".into(),
        },
    )
    .unwrap();
    assert_ne!(
        second.revision.unwrap().content_hash,
        first_revision.content_hash
    );
    let ledger = research_ledger(&fixture.store, &fixture.workspace_id).unwrap();
    assert_eq!(ledger.evidence[0].freshness, "stale");
}

#[test]
fn dynamic_tools_are_scoped_and_exactly_once() {
    let first_fixture = fixture();
    select_preset(&first_fixture, "research_assistant", "inspect");
    first_fixture
        .store
        .bind_session(&first_fixture.session_id, "runtime-one", "thread-one")
        .unwrap();
    let call = json!({"threadId":"thread-one","turnId":"turn-one","callId":"call-one","tool":"workbench_note_propose","arguments":{"kind":"question","body":"Can this be identified?"}});
    let first = handle_dynamic_tool_call(&first_fixture.store, &call).unwrap();
    let repeated = handle_dynamic_tool_call(&first_fixture.store, &call).unwrap();
    assert_eq!(first, repeated);
    assert!(first["success"].as_bool().unwrap());
    assert_eq!(
        list_notes(&first_fixture.store, &first_fixture.workspace_id, true)
            .unwrap()
            .len(),
        1
    );

    let other = fixture();
    let path = other.root.join("other.txt");
    fs::write(&path, "secret evidence").unwrap();
    let paper = import_paper(
        &other.store,
        ImportPaperRequest {
            workspace_id: other.workspace_id,
            paper_id: None,
            title: "Other".into(),
            role: "other".into(),
            path: path.to_string_lossy().into_owned(),
            operation_id: "other-paper".into(),
        },
    )
    .unwrap();
    let foreign = json!({"threadId":"thread-one","turnId":"turn-one","callId":"call-two","tool":"workbench_paper_read","arguments":{"revisionId":paper.revision.unwrap().id}});
    let denied = handle_dynamic_tool_call(&first_fixture.store, &foreign).unwrap();
    assert!(!denied["success"].as_bool().unwrap());
}

#[test]
fn source_duplicates_remain_separate_and_record_access_provenance() {
    let fixture = fixture();
    let path = fixture.root.join("source.bib");
    fs::write(&path, "@article{x, title={A Paper}}").unwrap();
    let request = |operation_id: &str| ImportSourceRequest {
        workspace_id: fixture.workspace_id.clone(),
        title: "A Paper".into(),
        citation_key: Some("x".into()),
        identifiers: json!({"doi":"10.1/example"}),
        version_label: None,
        path: Some(path.to_string_lossy().into_owned()),
        locator: None,
        access_state: "full".into(),
        acquired_via: "local_bibtex".into(),
        operation_id: operation_id.into(),
    };
    let first = import_source(&fixture.store, request("source-one")).unwrap();
    assert!(first.duplicate_candidates.is_empty());
    let second = import_source(&fixture.store, request("source-two")).unwrap();
    assert_eq!(second.duplicate_candidates.len(), 1);
    assert_ne!(first.source.id, second.source.id);
    assert_eq!(second.source.acquired_via, "local_bibtex");
}

#[test]
fn source_tree_identity_tracks_included_files_without_following_symlinks() {
    let fixture = fixture();
    let tree = fixture.root.join("tex-project");
    fs::create_dir(&tree).unwrap();
    fs::write(tree.join("main.tex"), "\\input{section}").unwrap();
    fs::write(tree.join("section.tex"), "First result.").unwrap();
    let first = import_paper(
        &fixture.store,
        ImportPaperRequest {
            workspace_id: fixture.workspace_id.clone(),
            paper_id: None,
            title: "Tree".into(),
            role: "manuscript".into(),
            path: tree.to_string_lossy().into_owned(),
            operation_id: "tree-one".into(),
        },
    )
    .unwrap();
    let first_revision = first.revision.unwrap();
    assert_eq!(first_revision.input_kind, "source_tree");
    assert!(
        paper_search(
            &fixture.store,
            &fixture.workspace_id,
            &first_revision.id,
            "First result",
            5
        )
        .unwrap()
        .len()
            == 1
    );
    fs::write(tree.join("section.tex"), "Revised result.").unwrap();
    let second = import_paper(
        &fixture.store,
        ImportPaperRequest {
            workspace_id: fixture.workspace_id.clone(),
            paper_id: Some(first.paper.id),
            title: "Tree".into(),
            role: "manuscript".into(),
            path: tree.to_string_lossy().into_owned(),
            operation_id: "tree-two".into(),
        },
    )
    .unwrap();
    assert_ne!(
        first_revision.content_hash,
        second.revision.unwrap().content_hash
    );
}

#[test]
fn execution_profiles_require_oldstata_and_tests_before_runs() {
    let fixture = fixture();
    let direct = save_execution_profile(
        &fixture.store,
        SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: fixture.workspace_id.clone(),
            name: "Bad Stata".into(),
            adapter: "stata".into(),
            argv: vec!["stata".into(), "-b".into(), "do x.do".into()],
            cwd: fixture.root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec![],
            timeout_seconds: 30,
            outputs: vec![],
            expected_revision: None,
            operation_id: "bad-stata".into(),
        },
    );
    assert!(direct.unwrap_err().message.contains("oldstata"));
    let profile = save_execution_profile(
        &fixture.store,
        SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: fixture.workspace_id.clone(),
            name: "Fixture".into(),
            adapter: "command".into(),
            argv: vec!["/usr/bin/true".into()],
            cwd: fixture.root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec![],
            timeout_seconds: 30,
            outputs: vec![],
            expected_revision: None,
            operation_id: "profile-one".into(),
        },
    )
    .unwrap();
    assert!(run_execution(
        &fixture.store,
        RunExecutionRequest {
            plan_id: None,
            profile_id: profile.id.clone(),
            session_id: Some(fixture.session_id.clone()),
            test_only: false,
            operation_id: "early-run".into()
        }
    )
    .is_err());
    let preview = preview_host_execution(&fixture.store, &profile.id).unwrap();
    authorize_host_execution(&fixture.store, &profile.id, &preview.fingerprint).unwrap();
    let tested = run_execution(
        &fixture.store,
        RunExecutionRequest {
            plan_id: None,
            profile_id: profile.id.clone(),
            session_id: Some(fixture.session_id.clone()),
            test_only: true,
            operation_id: "test-run".into(),
        },
    )
    .unwrap();
    assert_eq!(tested.outcome, "completed");
    let run = run_execution(
        &fixture.store,
        RunExecutionRequest {
            plan_id: None,
            profile_id: profile.id,
            session_id: Some(fixture.session_id.clone()),
            test_only: false,
            operation_id: "real-run".into(),
        },
    )
    .unwrap();
    assert_eq!(run.outcome, "completed");
    assert_eq!(run.snapshot_consistency, "uncertain");
}

#[test]
fn stata_output_validation_requires_every_declared_log_to_be_clean() {
    let fixture = fixture();
    fs::write(fixture.root.join("first.log"), "completed\n").unwrap();
    fs::write(fixture.root.join("second.log"), "failed\nr(198);\n").unwrap();
    let profile = ExecutionProfile {
        id: "stata-profile".into(),
        workspace_id: fixture.workspace_id,
        name: "Stata validation".into(),
        adapter: "stata".into(),
        argv: vec![
            "/bin/zsh".into(),
            "-lic".into(),
            "oldstata -q -b do test.do".into(),
        ],
        cwd: fixture.root.to_string_lossy().into_owned(),
        environment: json!({}),
        inputs: vec![],
        timeout_seconds: 30,
        outputs: vec!["first.log".into(), "second.log".into()],
        tested_at: None,
        test_status: None,
        revision: 1,
    };
    let (failed, valid) = execution::validate_execution_outputs(&profile, "completed");
    assert!(!valid);
    assert_eq!(failed["checks"][2]["passed"], false);

    fs::write(fixture.root.join("second.log"), "completed\n").unwrap();
    let (passed, valid) = execution::validate_execution_outputs(&profile, "completed");
    assert!(valid);
    assert_eq!(passed["checks"][2]["passed"], true);
}

#[test]
fn numeric_comparison_refuses_undeclared_incompatibilities() {
    let result = |sample: &str, estimate: f64| ResearchResultV1 {
        result_id: format!("result-{sample}"),
        estimand: "ATE".into(),
        specification_id: "spec-one".into(),
        sample_id: sample.into(),
        estimate,
        standard_error: Some(0.1),
        confidence_interval: Some([estimate - 0.2, estimate + 0.2]),
        n: Some(100),
        units: "percentage points".into(),
        transformation: None,
        uncertainty_method: Some("robust".into()),
        source_execution_id: "execution-one".into(),
        artifact_locator: "results.json#1".into(),
    };
    let comparison = compare_results(CompareResultsRequest {
        left: result("sample-one", 1.0),
        right: result("sample-two", 1.01),
        absolute_tolerance: 0.02,
        rationale: None,
    })
    .unwrap();
    assert!(!comparison.comparable);
    assert!(!comparison.passed);
    let justified = compare_results(CompareResultsRequest {
        left: result("sample-one", 1.0),
        right: result("sample-two", 1.01),
        absolute_tolerance: 0.02,
        rationale: Some("Documented overlapping samples".into()),
    })
    .unwrap();
    assert!(justified.comparable && justified.passed);
}

#[test]
fn structured_result_rechecks_create_host_receipts() {
    let fixture = fixture();
    let connection = fixture.store.connection().unwrap();
    for (id, hash) in [
        ("execution-left", "hash-left"),
        ("execution-right", "hash-right"),
    ] {
        connection.execute("INSERT INTO research_executions (id, workspace_id, adapter, command_json, cwd, input_manifest_json, dependency_hash, outcome, output_manifest_json, validation_json, snapshot_consistency, created_at) VALUES (?1, ?2, 'command', '[]', ?3, '{}', ?4, 'completed', '{\"artifacts\":[{\"path\":\"result.json\"}]}', '{}', 'complete', ?5)", params![id, fixture.workspace_id, fixture.root.to_string_lossy(), hash, now()]).unwrap();
    }
    let result = |id: &str, execution: &str, estimate: f64| ResearchResultV1 {
        result_id: id.into(),
        estimand: "ATE".into(),
        specification_id: "spec-one".into(),
        sample_id: "sample-one".into(),
        estimate,
        standard_error: Some(0.1),
        confidence_interval: None,
        n: Some(100),
        units: "points".into(),
        transformation: None,
        uncertainty_method: Some("robust".into()),
        source_execution_id: execution.into(),
        artifact_locator: "result.json#results/0".into(),
    };
    record_structured_result(
        &fixture.store,
        RecordStructuredResultRequest {
            workspace_id: fixture.workspace_id.clone(),
            result: result("result-left", "execution-left", 1.0),
            operation_id: "record-left".into(),
        },
    )
    .unwrap();
    record_structured_result(
        &fixture.store,
        RecordStructuredResultRequest {
            workspace_id: fixture.workspace_id.clone(),
            result: result("result-right", "execution-right", 1.01),
            operation_id: "record-right".into(),
        },
    )
    .unwrap();
    let claim = propose_claim(
        &fixture.store,
        ProposeClaimRequest {
            workspace_id: fixture.workspace_id.clone(),
            paper_id: None,
            claim: "The estimates reproduce.".into(),
            kind: "result".into(),
            origin: "user".into(),
            operation_id: "verification-claim".into(),
        },
        false,
    )
    .unwrap();
    let evidence = propose_evidence(
        &fixture.store,
        ProposeEvidenceRequest {
            workspace_id: fixture.workspace_id.clone(),
            claim_version_id: claim.version_id,
            target_type: "execution".into(),
            target_id: "execution-right".into(),
            locator: None,
            relation: "supports".into(),
            assessor: "user".into(),
            operation_id: "verification-evidence".into(),
        },
        false,
    )
    .unwrap();
    let receipt = verify_evidence_results(
        &fixture.store,
        VerifyEvidenceResultsRequest {
            evidence_id: evidence.id.clone(),
            left_result_id: "result-left".into(),
            right_result_id: "result-right".into(),
            absolute_tolerance: 0.02,
            rationale: None,
            operation_id: "verification-run".into(),
        },
    )
    .unwrap();
    assert!(receipt.passed);
    assert_eq!(
        get_evidence(&fixture.store, &fixture.workspace_id, &evidence.id)
            .unwrap()
            .assessment,
        "check_passed"
    );
}

#[test]
fn derived_availability_and_sections_do_not_change_the_fingerprint() {
    let fixture = fixture();
    select_preset(&fixture, "empirical_audit", "inspect");
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();

    let execution = effective
        .module_availability
        .iter()
        .find(|module| module.id == "research_execution")
        .expect("catalog module availability");
    assert!(!execution.available);
    assert!(execution
        .reasons
        .iter()
        .any(|reason| reason.contains("Edit access mode")));
    assert!(execution
        .reasons
        .iter()
        .any(|reason| reason.contains("passed its test")));
    let paper = effective
        .module_availability
        .iter()
        .find(|module| module.id == "paper_context")
        .unwrap();
    assert!(paper.available && paper.reasons.is_empty());
    assert_eq!(
        effective.module_availability.len(),
        harness_modules().len(),
        "availability covers the whole catalog, not only preset modules"
    );

    let ids = effective
        .instruction_sections
        .iter()
        .map(|section| section.id.as_str())
        .collect::<Vec<_>>();
    assert!(ids.starts_with(&["preamble", "preset"]));
    for section in &effective.instruction_sections {
        assert!(effective.developer_instructions.contains(&section.text));
    }

    let mut hashed = effective.clone();
    hashed.fingerprint = String::new();
    hashed.module_availability.clear();
    hashed.instruction_sections.clear();
    let bytes = serde_json::to_vec(&hashed).unwrap();
    assert_eq!(hash_bytes(&bytes), effective.fingerprint);
    assert!(!String::from_utf8(bytes)
        .unwrap()
        .contains("moduleAvailability"));
}

#[test]
fn agent_profiles_are_read_only_at_source_and_customizations_persist() {
    let fixture = fixture();
    let catalog = harness_catalog(&fixture.store, None).unwrap();
    for id in ["plain", "writing", "code_review", "econ_research"] {
        assert!(catalog.presets.iter().any(|p| p.id == id));
    }
    let default = find_preset(&fixture.store, None, "plain").unwrap();
    assert!(default.built_in && default.instructions.is_empty() && default.modules.is_empty());
    assert_eq!(catalog.prompt_layers[0].text, WORKSPACE_PREAMBLE);
    let source = find_preset(&fixture.store, None, "writing").unwrap();
    let edit =
        |id: &str, revision: i64, instructions: &str, modules: Vec<String>, operation: &str| {
            UpdatePresetRequest {
                base_prompt: None,
                preset_id: id.into(),
                expected_revision: revision,
                name: "My writer".into(),
                description: String::new(),
                instructions: instructions.into(),
                modules,
                operation_id: operation.into(),
            }
        };
    assert!(update_preset(
        &fixture.store,
        edit("plain", 1, "Overwrite default", vec![], "blocked-default")
    )
    .is_err());
    assert!(update_preset(
        &fixture.store,
        edit("writing", 1, "Overwrite starter", vec![], "blocked-starter")
    )
    .is_err());
    let copy = clone_preset(
        &fixture.store,
        ClonePresetRequest {
            source_workspace_id: None,
            workspace_id: None,
            source_preset_id: "writing".into(),
            name: "My writer".into(),
            operation_id: "clone-writer".into(),
        },
    )
    .unwrap();
    assert!(!copy.built_in);
    assert_eq!(copy.instructions, source.instructions);
    let edited = update_preset(
        &fixture.store,
        edit(
            &copy.id,
            1,
            "Write for a general audience.",
            vec!["paper_tools".into()],
            "edit-writer",
        ),
    )
    .unwrap();
    assert_eq!(edited.revision, 2);
    assert!(edited.description.is_empty());
    assert!(update_preset(
        &fixture.store,
        edit(&copy.id, 1, "Stale", vec![], "stale-writer")
    )
    .is_err());
    assert!(update_preset(
        &fixture.store,
        edit(
            &copy.id,
            2,
            "Invalid tools",
            vec!["unknown".into()],
            "invalid-tools"
        )
    )
    .is_err());
    assert!(update_preset(
        &fixture.store,
        edit(&copy.id, 2, "Invalid\0prompt", vec![], "invalid-prompt")
    )
    .is_err());
    let reopened = Store::open_at(fixture.store.root_path()).unwrap();
    assert_eq!(find_preset(&reopened, None, &copy.id).unwrap(), edited);
    assert_eq!(find_preset(&reopened, None, "writing").unwrap(), source);
    select_preset(&fixture, &copy.id, "inspect");
    let effective = resolve_harness(&reopened, &fixture.session_id).unwrap();
    assert!(effective
        .developer_instructions
        .contains("Write for a general audience."));
    assert!(effective
        .dynamic_tools
        .iter()
        .any(|tool| tool["name"] == "workbench_paper_read"));
    assert!(!effective
        .dynamic_tools
        .iter()
        .any(|tool| tool["name"] == "workbench_note_propose"));
    let local = clone_preset(
        &fixture.store,
        ClonePresetRequest {
            source_workspace_id: None,
            workspace_id: Some(fixture.workspace_id.clone()),
            source_preset_id: "plain".into(),
            name: "Local profile".into(),
            operation_id: "clone-local".into(),
        },
    )
    .unwrap();
    assert!(find_preset(&reopened, None, &local.id).is_err());
    assert!(find_preset(&reopened, Some(&fixture.workspace_id), &local.id).is_ok());
}

#[test]
fn default_profile_rename_does_not_retire_existing_bindings() {
    let fixture = fixture();
    let mut effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_eq!(effective.preset.name, "Codex default");
    let fingerprint = effective.fingerprint.clone();
    effective.fingerprint.clear();
    effective.module_availability.clear();
    effective.instruction_sections.clear();
    effective.preset.name = "Plain conversation".into();
    effective.preset.description = "No research instructions or automatic context.".into();
    assert_eq!(
        fingerprint,
        hash_bytes(&serde_json::to_vec(&effective).unwrap())
    );
}

#[test]
fn a_project_profile_can_be_copied_to_all_workspaces() {
    let fixture = fixture();
    let local = clone_preset(
        &fixture.store,
        ClonePresetRequest {
            workspace_id: Some(fixture.workspace_id.clone()),
            source_workspace_id: None,
            source_preset_id: "econ_research".into(),
            name: "Local economist".into(),
            operation_id: "local-econ".into(),
        },
    )
    .unwrap();
    let shared = clone_preset(
        &fixture.store,
        ClonePresetRequest {
            workspace_id: None,
            source_workspace_id: Some(fixture.workspace_id.clone()),
            source_preset_id: local.id.clone(),
            name: "Shared economist".into(),
            operation_id: "shared-econ".into(),
        },
    )
    .unwrap();
    assert!(shared.workspace_id.is_none());
    assert_eq!(shared.instructions, local.instructions);
    assert_eq!(
        find_preset(&fixture.store, None, &shared.id).unwrap(),
        shared
    );
    let legacy: ClonePresetRequest = serde_json::from_value(json!({"workspaceId":null,"sourcePresetId":"plain","name":"Legacy client","operationId":"legacy-clone"})).unwrap();
    assert!(legacy.source_workspace_id.is_none());
}

#[test]
fn base_prompt_overrides_survive_copies_legacy_updates_and_snapshots() {
    let fixture = fixture();
    let copy = clone_preset(&fixture.store, ClonePresetRequest {
        workspace_id: None, source_workspace_id: None, source_preset_id: "writing".into(),
        name: "Writer".into(), operation_id: "base-clone".into(),
    }).unwrap();
    assert_eq!(copy.base_instructions, find_preset(&fixture.store, None, "writing").unwrap().base_instructions);
    assert!(copy.base_instructions.is_some());
    select_preset(&fixture, &copy.id, "inspect");
    let before = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    let request = |revision, change: Option<Value>| {
        let mut value = json!({"presetId":copy.id,"expectedRevision":revision,"name":"Writer","description":"","instructions":"Additional writing guidance","modules":[],"operationId":format!("base-update-{revision}")});
        if let Some(change) = change { value["basePrompt"] = change; }
        serde_json::from_value::<UpdatePresetRequest>(value).unwrap()
    };
    let text = "You are a custom writer.\nPreserve this exact whitespace.\n";
    let updated = update_preset(&fixture.store, request(1, Some(json!({"mode":"replace","text":text})))).unwrap();
    assert_eq!(updated.base_instructions.as_deref(), Some(text));
    let prepared = prepare_turn(&fixture.store, &fixture.session_id).unwrap();
    assert_ne!(before.fingerprint, prepared.effective.fingerprint);
    assert_eq!(prepared.effective.preset.base_instructions.as_deref(), Some(text));
    assert!(!prepared.effective.developer_instructions.contains(text));
    assert!(!prepared.effective.developer_instructions.contains("Preserve Codex base instructions."));
    assert!(prepared.effective.developer_instructions.contains("Additional writing guidance"));
    let snapshot: EffectiveHarness = serde_json::from_value(serde_json::to_value(&prepared.effective).unwrap()).unwrap();
    assert_eq!(snapshot.preset.base_instructions.as_deref(), Some(text));
    let legacy_update = update_preset(&fixture.store, request(2, None)).unwrap();
    assert_eq!(legacy_update.base_instructions.as_deref(), Some(text));
    let inherited = update_preset(&fixture.store, request(3, Some(json!({"mode":"codexDefault"})))).unwrap();
    assert!(inherited.base_instructions.is_none());
    let inherited = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_ne!(prepared.effective.fingerprint, inherited.fingerprint);
    assert!(inherited.developer_instructions.contains("Preserve Codex base instructions."));
    let value = serde_json::to_value(&inherited).unwrap();
    assert!(value["preset"].get("baseInstructions").is_none());
    let legacy: EffectiveHarness = serde_json::from_value(value).unwrap();
    assert!(legacy.preset.base_instructions.is_none());
    for invalid in ["   ".to_string(), "NUL\0prompt".into(), "x".repeat(MAX_INSTRUCTIONS_BYTES + 1)] {
        assert!(update_preset(&fixture.store, request(4, Some(json!({"mode":"replace","text":invalid})))).is_err());
    }
    assert_eq!(find_preset(&fixture.store, None, &copy.id).unwrap().revision, 4);
}

#[test]
fn schema_13_custom_profiles_migrate_to_inheriting_the_native_default() {
    let fixture = fixture();
    let copy = clone_preset(&fixture.store, ClonePresetRequest {
        workspace_id: None, source_workspace_id: None, source_preset_id: "plain".into(),
        name: "Existing profile".into(), operation_id: "legacy-base-clone".into(),
    }).unwrap();
    let connection = fixture.store.connection().unwrap();
    connection.execute_batch("ALTER TABLE presets DROP COLUMN base_instructions; PRAGMA user_version = 13;").unwrap();
    drop(connection);
    let migrated = Store::open_at(fixture.store.root_path()).unwrap();
    assert_eq!(find_preset(&migrated, None, &copy.id).unwrap(), copy);
}

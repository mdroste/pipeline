//! Built-in agent profiles: read-only sources, customizations, the current
//! lineup with retired IDs mapped, base-prompt overrides, and the paper tools
//! every project conversation gets.

use super::*;

#[test]
fn agent_profiles_are_read_only_at_source_and_customizations_persist() {
    let fixture = fixture();
    let catalog = harness_catalog(&fixture.store, None).unwrap();
    for id in ["plain", "research", "writing", "data_code"] {
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
    assert_eq!(effective.preset.name, "Default");
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
            source_preset_id: "research".into(),
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
fn retired_profiles_and_modules_resolve_to_the_current_lineup() {
    let fixture = fixture();
    // A conversation saved on a retired built-in uses its successor.
    select_preset(&fixture, "empirical_audit", "inspect");
    let session = fixture
        .store
        .session_snapshot(&fixture.session_id)
        .unwrap()
        .session;
    assert_eq!(session.preset_id.as_deref(), Some("research"));
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_eq!(effective.preset.id, "research");
    assert!(effective.preset.base_instructions.is_none());
    for (retired, current) in [
        ("research_assistant", "research"),
        ("econ_research", "research"),
        ("theory_audit", "research"),
        ("literature_review", "research"),
        ("paper_revision", "writing"),
        ("code_review", "data_code"),
    ] {
        assert_eq!(
            find_preset(&fixture.store, None, retired).unwrap().id,
            current
        );
    }
    // A copy made before the change keeps its own text, drops the instruction
    // packs, and restores from the successor.
    fixture
        .store
        .connection()
        .unwrap()
        .execute(
            "INSERT INTO presets (id,name,description,instructions,modules_json,source_preset_id,revision,created_at,updated_at) VALUES ('preset-old','Old audit','','Check the estimand.',?1,'empirical_audit',1,'t','t')",
            params![json!(["research_structure", "empirical_audit", "paper_context"]).to_string()],
        )
        .unwrap();
    let old = find_preset(&fixture.store, None, "preset-old").unwrap();
    assert_eq!(old.modules, vec!["paper_context".to_string()]);
    assert_eq!(old.source_preset_id.as_deref(), Some("research"));
    select_preset(&fixture, "preset-old", "inspect");
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_eq!(effective.preset.id, "preset-old");
    assert!(effective
        .developer_instructions
        .contains("Check the estimand."));
}

#[test]
fn paper_tools_follow_the_project_and_an_attached_paper_brings_its_context() {
    let fixture = fixture();
    select_preset(&fixture, "plain", "inspect");
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(effective
        .enabled_modules
        .contains(&"paper_tools".to_string()));
    assert!(!effective
        .enabled_modules
        .contains(&"paper_context".to_string()));
    assert!(effective.unavailable_modules.is_empty());
    assert!(effective
        .dynamic_tools
        .iter()
        .any(|tool| tool["name"] == "workbench_research_search_v1"));
    assert!(effective
        .developer_instructions
        .contains("Never invent citations, quotations, data, estimates, or results of checks you did not run."));

    let path = fixture.root.join("attached.txt");
    fs::write(&path, "An attached argument about sufficient statistics").unwrap();
    let paper = import_paper(
        &fixture.store,
        ImportPaperRequest {
            workspace_id: fixture.workspace_id.clone(),
            paper_id: None,
            title: "Attached".into(),
            role: "other".into(),
            document_type: None,
            path: path.to_string_lossy().into_owned(),
            operation_id: "attached-paper".into(),
        },
    )
    .unwrap();
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
            operation_id: "attach-paper".into(),
            title: None,
            draft: None,
            overrides: None,
            archived: None,
            preset_id: None,
            paper_id: Some(paper.paper.id),
            clear_paper: None,
        })
        .unwrap();
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_eq!(effective.preset.id, "plain");
    assert!(effective
        .enabled_modules
        .contains(&"paper_context".to_string()));
    assert!(effective.context_preview.contains("sufficient statistics"));
}

#[test]
fn base_prompt_overrides_survive_copies_legacy_updates_and_snapshots() {
    let fixture = fixture();
    let copy = clone_preset(
        &fixture.store,
        ClonePresetRequest {
            workspace_id: None,
            source_workspace_id: None,
            source_preset_id: "writing".into(),
            name: "Writer".into(),
            operation_id: "base-clone".into(),
        },
    )
    .unwrap();
    assert_eq!(
        copy.base_instructions,
        find_preset(&fixture.store, None, "writing")
            .unwrap()
            .base_instructions
    );
    // Built-in profiles keep the connection's base prompt.
    assert!(copy.base_instructions.is_none());
    select_preset(&fixture, &copy.id, "inspect");
    let before = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    let request = |revision, change: Option<Value>| {
        let mut value = json!({"presetId":copy.id,"expectedRevision":revision,"name":"Writer","description":"","instructions":"Additional writing guidance","modules":[],"operationId":format!("base-update-{revision}")});
        if let Some(change) = change {
            value["basePrompt"] = change;
        }
        serde_json::from_value::<UpdatePresetRequest>(value).unwrap()
    };
    let text = "You are a custom writer.\nPreserve this exact whitespace.\n";
    let updated = update_preset(
        &fixture.store,
        request(1, Some(json!({"mode":"replace","text":text}))),
    )
    .unwrap();
    assert_eq!(updated.base_instructions.as_deref(), Some(text));
    let second = clone_preset(
        &fixture.store,
        ClonePresetRequest {
            workspace_id: None,
            source_workspace_id: None,
            source_preset_id: copy.id.clone(),
            name: "Second writer".into(),
            operation_id: "base-clone-again".into(),
        },
    )
    .unwrap();
    assert_eq!(second.base_instructions.as_deref(), Some(text));
    let prepared = prepare_turn(&fixture.store, &fixture.session_id).unwrap();
    assert_ne!(before.fingerprint, prepared.effective.fingerprint);
    assert_eq!(
        prepared.effective.preset.base_instructions.as_deref(),
        Some(text)
    );
    assert!(!prepared.effective.developer_instructions.contains(text));
    assert!(!prepared
        .effective
        .developer_instructions
        .contains("Preserve Codex base instructions."));
    assert!(prepared
        .effective
        .developer_instructions
        .contains("Additional writing guidance"));
    let snapshot: EffectiveHarness =
        serde_json::from_value(serde_json::to_value(&prepared.effective).unwrap()).unwrap();
    assert_eq!(snapshot.preset.base_instructions.as_deref(), Some(text));
    let legacy_update = update_preset(&fixture.store, request(2, None)).unwrap();
    assert_eq!(legacy_update.base_instructions.as_deref(), Some(text));
    let inherited = update_preset(
        &fixture.store,
        request(3, Some(json!({"mode":"codexDefault"}))),
    )
    .unwrap();
    assert!(inherited.base_instructions.is_none());
    let inherited = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_ne!(prepared.effective.fingerprint, inherited.fingerprint);
    assert!(inherited
        .developer_instructions
        .contains("Preserve Codex base instructions."));
    let value = serde_json::to_value(&inherited).unwrap();
    assert!(value["preset"].get("baseInstructions").is_none());
    let legacy: EffectiveHarness = serde_json::from_value(value).unwrap();
    assert!(legacy.preset.base_instructions.is_none());
    for invalid in [
        "   ".to_string(),
        "NUL\0prompt".into(),
        "x".repeat(MAX_INSTRUCTIONS_BYTES + 1),
    ] {
        assert!(update_preset(
            &fixture.store,
            request(4, Some(json!({"mode":"replace","text":invalid})))
        )
        .is_err());
    }
    assert_eq!(
        find_preset(&fixture.store, None, &copy.id)
            .unwrap()
            .revision,
        4
    );
}

#[test]
fn schema_13_custom_profiles_migrate_to_inheriting_the_native_default() {
    let fixture = fixture();
    let copy = clone_preset(
        &fixture.store,
        ClonePresetRequest {
            workspace_id: None,
            source_workspace_id: None,
            source_preset_id: "plain".into(),
            name: "Existing profile".into(),
            operation_id: "legacy-base-clone".into(),
        },
    )
    .unwrap();
    let legacy_root = fixture._temporary.path().join("legacy-store");
    let connection = crate::workbench::store::historical_test_database(&legacy_root, 13);
    connection
        .execute(
            "INSERT INTO presets (id,name,description,instructions,modules_json,source_preset_id,revision,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,'t','t')",
            params![copy.id, copy.name, copy.description, copy.instructions, serde_json::to_string(&copy.modules).unwrap(), copy.source_preset_id, copy.revision],
        )
        .unwrap();
    drop(connection);
    let migrated = Store::open_at(&legacy_root).unwrap();
    assert_eq!(find_preset(&migrated, None, &copy.id).unwrap(), copy);
}

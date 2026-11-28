//! The project's field and conventions reach every conversation in the project,
//! on every connection, as the researcher's own instructions.

use super::*;

const CONVENTIONS: &str =
    "Economics. Use the field's notation; report standard errors clustered as in the paper.";

/// Saves the project's setting, or removes it with `None`, keeping the rest of
/// the project's assistant settings.
fn set_conventions(fixture: &Fixture, value: Option<Value>) -> WorkbenchResult<WorkspaceConfig> {
    let current = load_config(&fixture.store, Some(&fixture.workspace_id))?;
    let mut body = current.body.as_object().cloned().unwrap_or_default();
    match value {
        Some(value) => body.insert(FIELD_CONVENTIONS_KEY.into(), value),
        None => body.remove(FIELD_CONVENTIONS_KEY),
    };
    save_config(
        &fixture.store,
        SaveWorkspaceConfigRequest {
            workspace_id: Some(fixture.workspace_id.clone()),
            expected_revision: Some(current.revision),
            body: Value::Object(body),
            operation_id: format!("conventions-{}", current.revision),
        },
    )
}

fn set_session(fixture: &Fixture, preset: &str, overrides: Value) {
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
            operation_id: format!("session-{}", session.revision),
            title: None,
            draft: None,
            overrides: Some(overrides),
            archived: None,
            preset_id: Some(preset.into()),
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
}

fn section_ids(effective: &EffectiveHarness) -> Vec<&str> {
    effective
        .instruction_sections
        .iter()
        .map(|section| section.id.as_str())
        .collect()
}

#[test]
fn field_conventions_follow_the_profile_on_every_connection() {
    let fixture = fixture();
    create_note(
        &fixture.store,
        CreateNoteRequest {
            workspace_id: fixture.workspace_id.clone(),
            paper_id: None,
            kind: "question".into(),
            body: "Which elasticity does the paper estimate?".into(),
            state: Some("accepted".into()),
            origin: "user".into(),
            pinned: true,
            operation_id: "note-elasticity".into(),
        },
        false,
    )
    .unwrap();
    set_conventions(&fixture, Some(json!(format!("  {CONVENTIONS}\n")))).unwrap();
    for runtime in [
        None,
        Some("codex"),
        Some("claude"),
        Some("claude-code-api"),
        Some("openai-api"),
        Some("anthropic-api"),
        Some("google-api"),
        Some("local"),
    ] {
        let overrides = runtime.map_or(json!({}), |runtime| json!({ "runtime": runtime }));
        set_session(&fixture, "research", overrides);
        let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
        let ids = section_ids(&effective);
        assert!(
            ids.starts_with(&["preamble", "preset", "field_conventions"]),
            "{runtime:?}: {ids:?}"
        );
        let section = &effective.instruction_sections[2];
        assert_eq!(section.label, "Project field and conventions");
        assert!(section.text.ends_with(CONVENTIONS), "{runtime:?}");
        let developer = &effective.developer_instructions;
        assert!(developer.contains(&section.text), "{runtime:?}");
        let conventions = developer.find(CONVENTIONS).unwrap();
        assert!(developer.find(&effective.preset.instructions).unwrap() < conventions);
        // The researcher's instruction, not one of the sources sent as material.
        assert!(
            effective.context_preview.contains("elasticity"),
            "{runtime:?}"
        );
        assert!(!effective.context_preview.contains(CONVENTIONS));
        assert!(conventions < developer.find("<workspace_context").unwrap());
    }
}

#[test]
fn empty_field_conventions_add_no_section() {
    let fixture = fixture();
    set_session(&fixture, "research", json!({}));
    let unset = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    for value in ["", " \n\t "] {
        set_conventions(&fixture, Some(json!(value))).unwrap();
        let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
        assert!(!section_ids(&effective).contains(&"field_conventions"));
        assert_eq!(
            effective.developer_instructions,
            unset.developer_instructions
        );
        assert_eq!(effective.fingerprint, unset.fingerprint);
    }
}

#[test]
fn field_conventions_change_the_fingerprint_and_clearing_restores_it() {
    let fixture = fixture();
    set_session(&fixture, "plain", json!({}));
    let before = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    set_conventions(&fixture, Some(json!(CONVENTIONS))).unwrap();
    let set = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(section_ids(&set).starts_with(&["preamble", "field_conventions"]));
    assert_ne!(set.fingerprint, before.fingerprint);
    set_conventions(
        &fixture,
        Some(json!("Political science. Report robust standard errors.")),
    )
    .unwrap();
    let edited = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_ne!(edited.fingerprint, set.fingerprint);
    assert_ne!(edited.fingerprint, before.fingerprint);
    set_conventions(&fixture, None).unwrap();
    let cleared = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_eq!(cleared.fingerprint, before.fingerprint);
}

#[test]
fn only_the_project_sets_field_conventions_within_the_limit() {
    let fixture = fixture();
    let global = save_config(
        &fixture.store,
        SaveWorkspaceConfigRequest {
            workspace_id: None,
            expected_revision: Some(0),
            body: json!({ FIELD_CONVENTIONS_KEY: CONVENTIONS }),
            operation_id: "global-conventions".into(),
        },
    )
    .unwrap_err();
    assert!(
        global.message.contains("each project"),
        "{}",
        global.message
    );
    for value in [
        json!("x".repeat(MAX_FIELD_CONVENTIONS_BYTES + 1)),
        json!("Economics\0"),
        json!(["Economics"]),
    ] {
        let error = set_conventions(&fixture, Some(value)).unwrap_err();
        assert!(error.message.contains("at most 4 KiB"), "{}", error.message);
    }
    // A conversation cannot set or replace the project's conventions.
    set_session(
        &fixture,
        "plain",
        json!({ FIELD_CONVENTIONS_KEY: CONVENTIONS }),
    );
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(!section_ids(&effective).contains(&"field_conventions"));
    assert!(effective.value_sources.get(FIELD_CONVENTIONS_KEY).is_none());

    let longest = "é".repeat(MAX_FIELD_CONVENTIONS_BYTES / 2);
    set_conventions(&fixture, Some(json!(longest))).unwrap();
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    let section = effective
        .instruction_sections
        .iter()
        .find(|section| section.id == "field_conventions")
        .unwrap();
    assert!(section.text.ends_with(&longest));
    assert!(!effective.developer_instructions.contains(CONVENTIONS));
}

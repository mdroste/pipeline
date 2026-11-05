//! What a long automation compares, and how a source limit fits the instructions.

use super::*;

fn set_overrides(fixture: &Fixture, overrides: Value, operation: &str) {
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
            operation_id: operation.into(),
            title: None,
            draft: None,
            overrides: Some(overrides),
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
}

#[test]
fn ordinary_conversation_changes_do_not_end_a_long_automation() {
    use crate::workbench::missions::refresh_scope;

    let fixture = fixture();
    let (scope, authority) = bound_scope(&fixture);
    set_overrides(
        &fixture,
        json!({"model":"another-model","reasoningEffort":"high"}),
        "ordinary-settings",
    );
    refresh_scope(&fixture.store, &scope, &authority).unwrap();

    set_overrides(&fixture, json!({"mode":"edit"}), "wider-access");
    let refused = refresh_scope(&fixture.store, &scope, &authority).unwrap_err();
    assert!(
        refused.message.contains("permissions"),
        "{}",
        refused.message
    );
}

#[test]
fn a_first_edit_turn_in_the_source_conversation_does_not_end_a_discovery_run() {
    use crate::workbench::missions::{readopt_scope, refresh_source_scope, source_authority};

    let fixture = fixture();
    set_overrides(&fixture, json!({"mode":"edit"}), "edit-mode");
    let (scope, _) = bound_scope(&fixture);
    let authority = source_authority(&fixture.store, &fixture.session_id).unwrap();
    crate::workbench::project::conversation_copy::prepare(&fixture.store, &fixture.session_id)
        .unwrap();
    let session = fixture
        .store
        .session_snapshot(&fixture.session_id)
        .unwrap()
        .session;
    assert!(session.overrides["projectCheckpointId"].is_string());

    let refreshed = refresh_source_scope(&fixture.store, &scope, &authority).unwrap();
    assert_eq!(refreshed.runtime_root, scope.runtime_root);
    readopt_scope(&fixture.store, &scope, &authority, false).unwrap();
    assert!(readopt_scope(&fixture.store, &scope, &authority, true).is_err());

    assert!(refresh_source_scope(&fixture.store, &scope, "source-v1:other").is_err());
}

#[test]
fn a_fingerprint_saved_before_the_current_format_still_answers() {
    use crate::workbench::missions::{legacy_authority, refresh_scope};

    let fixture = fixture();
    let (scope, _) = bound_scope(&fixture);
    let saved = legacy_authority(&fixture.store, &fixture.session_id).unwrap();
    assert!(refresh_scope(&fixture.store, &scope, &saved).is_ok());
    assert!(refresh_scope(&fixture.store, &scope, "stale").is_err());
}

#[test]
fn only_a_marked_conversation_is_an_automation_role() {
    use crate::workbench::missions::is_role;

    let fixture = fixture();
    assert!(!is_role(&fixture.store, &fixture.session_id).unwrap());
    set_overrides(&fixture, json!({"automationRole":true}), "role");
    assert!(is_role(&fixture.store, &fixture.session_id).unwrap());
}

#[test]
fn the_largest_source_limit_shortens_sources_instead_of_breaking_the_conversation() {
    let fixture = fixture();
    save_config(
        &fixture.store,
        SaveWorkspaceConfigRequest {
            workspace_id: Some(fixture.workspace_id.clone()),
            expected_revision: Some(0),
            body: json!({"contextBudgetBytes":524288}),
            operation_id: "save-config".into(),
        },
    )
    .unwrap();
    for index in 0..120 {
        create_note(
            &fixture.store,
            CreateNoteRequest {
                workspace_id: fixture.workspace_id.clone(),
                paper_id: None,
                kind: "question".into(),
                // Each `<` takes six bytes once encoded.
                body: format!("{index} {}", "<é\"".repeat(1_500)),
                state: Some("accepted".into()),
                origin: "user".into(),
                pinned: true,
                operation_id: format!("note-{index}"),
            },
            false,
        )
        .unwrap();
    }
    select_preset(&fixture, "empirical_audit", "edit");
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(effective.context_truncated);
    assert!(effective.developer_instructions.len() > 200 * 1024);
    assert!(effective
        .developer_instructions
        .contains("</workspace_context>"));
}

#[test]
fn fitted_context_stays_within_its_limit_on_a_character_boundary() {
    let text = "<é\"\n".repeat(1_000);
    let (whole, kept) = super::harness::fit_encoded_context(&text, usize::MAX).unwrap();
    assert_eq!(kept, text.len());
    assert!(whole.len() > text.len());
    for limit in [0, 1, 2, 3, 7, 100, 4_096] {
        let (encoded, kept) = super::harness::fit_encoded_context(&text, limit).unwrap();
        assert!(kept == 0 || encoded.len() <= limit, "{limit}");
        assert!(text.is_char_boundary(kept), "{limit}");
    }
}

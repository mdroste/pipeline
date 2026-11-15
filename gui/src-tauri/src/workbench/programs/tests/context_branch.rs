//! Permissions for new conversations created from selected research context.
use super::*;

#[test]
fn selected_context_branches_override_project_edit_authority() {
    for mode in ["direct", "review"] {
        let f = fixture(true);
        f.store
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO workspace_configs VALUES(?1,?1,1,?2,1,'now')",
                rusqlite::params![f.ws, json!({"mode":"edit","fileMode":mode}).to_string()],
            )
            .unwrap();
        let note = research::create_note(
            &f.store,
            research::CreateNoteRequest {
                workspace_id: f.ws.clone(),
                paper_id: None,
                kind: "decision".into(),
                body: "Research context".into(),
                state: Some("accepted".into()),
                origin: "user".into(),
                pinned: false,
                operation_id: "note".into(),
            },
            false,
        )
        .unwrap();
        let branch = followups::branch(
            &f.store,
            &f.session,
            "Sources",
            vec![desk::ContextItem {
                object: desk::ResearchObjectRef {
                    kind: "note".into(),
                    id: note.id,
                    revision: "1".into(),
                    start: None,
                    end: None,
                },
                role: "source".into(),
            }],
            "branch",
        )
        .unwrap();
        let resolved =
            research::resolve_harness(&f.store, branch["session"]["id"].as_str().unwrap()).unwrap();
        assert_eq!(resolved.mode, "inspect");
        assert!(resolved.dynamic_tools.iter().all(|tool| !matches!(
            tool["name"].as_str(),
            Some("project_create" | "project_edit")
        )));
        assert_eq!(resolved.permission_profile, "workbench-inspect");
    }
}

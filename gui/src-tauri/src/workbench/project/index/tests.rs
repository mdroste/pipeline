use super::*;
use crate::workbench::store::CreateWorkspaceRequest;

fn workspace(store: &Store, name: &str) -> String {
    store
        .create_workspace(CreateWorkspaceRequest {
            name: name.into(),
            root: None,
            operation_id: name.into(),
        })
        .unwrap()
        .record
        .id
}

fn note(store: &Store, ws: &str, id: &str, kind: &str, state: &str, text: &str) {
    store.connection().unwrap().execute(
        "INSERT INTO research_notes (id,workspace_id,kind,state,body,origin,pinned,revision,created_at,updated_at)
         VALUES (?1,?2,?3,?4,?5,'user',0,1,'2026-09-01T00:00:00+00:00','2026-09-01T00:00:00+00:00')",
        params![id,ws,kind,state,text],
    ).unwrap();
}

#[test]
fn brief_is_accepted_scoped_curated_and_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let first = workspace(&store, "first");
    let second = workspace(&store, "second");
    note(
        &store,
        &first,
        "question",
        "question",
        "accepted",
        "Accepted research question",
    );
    note(
        &store,
        &first,
        "proposed",
        "question",
        "proposed",
        "Unaccepted assertion",
    );
    note(
        &store,
        &second,
        "private",
        "question",
        "accepted",
        "Other project question",
    );
    let initial = project_index(&store).unwrap();
    let item = initial.iter().find(|p| p.id == first).unwrap();
    assert_eq!(
        item.brief.as_ref().unwrap().title,
        "Accepted research question"
    );
    assert_eq!(item.proposed_notes, 1);
    assert!(item.conversation.is_none());
    note(
        &store,
        &first,
        "curated",
        "decision",
        "accepted",
        &"界".repeat(1000),
    );
    store
        .connection()
        .unwrap()
        .execute(
            "INSERT INTO project_records VALUES (?1,?2,'home',1,?3,'2026-09-01T00:00:00+00:00')",
            params![
                format!("home_{first}"),
                first,
                json!({"briefNoteIds":["curated","private","proposed"]}).to_string()
            ],
        )
        .unwrap();
    let result = project_index(&store).unwrap();
    let item = result.iter().find(|p| p.id == first).unwrap();
    assert_eq!(item.brief.as_ref().unwrap().id, "curated");
    assert_eq!(item.brief.as_ref().unwrap().title.chars().count(), 600);
}

#[test]
fn active_conversation_is_exact_and_archive_and_draft_stay_out_of_index() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let first = workspace(&store, "first");
    let second = workspace(&store, "second");
    let connection = store.connection().unwrap();
    connection
        .execute(
            "UPDATE workspaces SET updated_at='2026-01-01T00:00:00+00:00'",
            [],
        )
        .unwrap();
    for (id, ws, at, archived) in [
        ("older", &first, "2026-09-01T00:00:00+00:00", None),
        ("latest", &first, "2026-09-03T00:00:00+00:00", None),
        (
            "archived",
            &first,
            "2026-09-05T00:00:00+00:00",
            Some("archived"),
        ),
        ("second", &second, "2026-09-02T00:00:00+00:00", None),
    ] {
        connection.execute("INSERT INTO sessions (id,workspace_id,title,draft,archived_at,created_at,updated_at) VALUES (?1,?2,?1,'private draft',?3,?4,?4)", params![id,ws,archived,at]).unwrap();
    }
    let result = project_index(&store).unwrap();
    assert_eq!(result[0].id, first);
    assert_eq!(result[0].conversation.as_ref().unwrap().id, "latest");
    assert_eq!(result[0].activity.as_ref().unwrap().id, "latest");
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("private draft"));
    connection
        .execute(
            "UPDATE workspaces SET archived_at='archived' WHERE id=?1",
            [&first],
        )
        .unwrap();
    assert_eq!(project_index(&store).unwrap().len(), 1);
}

#[test]
fn attention_and_next_steps_exclude_completed_tasks_and_prefer_work_over_deferred() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let ws = workspace(&store, "research");
    let connection = store.connection().unwrap();
    for (id, status) in [
        ("done", "completed"),
        ("later", "deferred"),
        ("work", "investigating"),
    ] {
        connection.execute("INSERT INTO project_records VALUES (?1,?2,'task',1,?3,'2026-09-01T00:00:00+00:00')", params![id,ws,json!({"objective":id,"status":status}).to_string()]).unwrap();
    }
    connection.execute("INSERT INTO project_records VALUES ('recovery',?1,'application',1,'{\"state\":\"recovery_required\"}','2026-09-01T00:00:00+00:00')", [&ws]).unwrap();
    let result = project_index(&store).unwrap();
    assert_eq!(result[0].next_task.as_ref().unwrap().id, "work");
    assert_eq!(result[0].interrupted_edits, 1);
}

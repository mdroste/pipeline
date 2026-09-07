//! Storage migration, concurrency, persistence, and projection tests.

use super::*;

fn fixture() -> (tempfile::TempDir, Store, PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let store_root = temporary.path().join("workbench-store");
    let workspace_root = temporary.path().join("research-workspace");
    std::fs::create_dir(&workspace_root).unwrap();
    let store = Store::open_at(&store_root).unwrap();
    (temporary, store, workspace_root)
}

#[test]
fn creates_private_versioned_layout() {
    let (_temporary, store, _workspace) = fixture();
    assert!(store.database_path().is_file());
    for child in ["blobs", "jobs", "context", "codex", "backups"] {
        assert!(store.root.join(child).is_dir());
    }
    let connection = store.connection().unwrap();
    assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
    assert!(connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'sessions')",
            [],
            |row| row.get::<_, bool>(0)
        )
        .unwrap());
    assert_eq!(
        connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(&store.root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&store.database)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn concurrent_first_open_serializes_schema_initialization() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("concurrent-store");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let threads = (0..2)
        .map(|_| {
            let root = root.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let store = Store::open_at(&root).unwrap();
                let connection = store.connection().unwrap();
                assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
fn rejects_newer_store_without_modifying_it() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workbench");
    std::fs::create_dir_all(&root).unwrap();
    let connection = Connection::open(root.join("research.sqlite3")).unwrap();
    connection
        .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
        .unwrap();
    drop(connection);
    let error = Store::open_at(&root).unwrap_err();
    assert_eq!(error.code, "newer_store_schema");
}

#[test]
fn migration_backs_up_an_existing_store() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workbench");
    std::fs::create_dir_all(&root).unwrap();
    let connection = Connection::open(root.join("research.sqlite3")).unwrap();
    connection
        .execute("CREATE TABLE legacy_fixture (value TEXT)", [])
        .unwrap();
    connection
        .execute("INSERT INTO legacy_fixture VALUES ('kept')", [])
        .unwrap();
    drop(connection);
    let _store = Store::open_at(&root).unwrap();
    let backup = std::fs::read_dir(root.join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let backup = Connection::open(backup).unwrap();
    assert_eq!(
        backup
            .query_row("SELECT value FROM legacy_fixture", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "kept"
    );
}

#[test]
fn migration_two_preserves_the_complete_transitional_conversation_graph() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workbench");
    let workspace_root = temporary.path().join("research-root");
    let review_state = temporary.path().join("review-project.json");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir(&workspace_root).unwrap();
    std::fs::write(&review_state, b"review-owned").unwrap();
    let database = root.join("research.sqlite3");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(include_str!("../migrations/001_initial.sql"))
        .unwrap();
    let root_text = workspace_root.to_string_lossy().into_owned();
    connection
            .execute(
                "INSERT INTO project_workspaces (project_id, root, root_identity, created_at, updated_at) VALUES ('legacy-project', ?1, 'legacy-root-id', '2026-09-06T00:00:00Z', '2026-09-06T00:00:00Z')",
                [&root_text],
            )
            .unwrap();
    connection
            .execute("INSERT INTO sessions (id, project_id, title, overrides_json, draft, revision, created_at, updated_at) VALUES ('session-legacy', 'legacy-project', 'Preserved chat', '{}', 'draft', 2, '2026-09-06T00:00:00Z', '2026-09-06T00:00:00Z')", [])
            .unwrap();
    connection
            .execute("INSERT INTO session_bindings (id, session_id, runtime_namespace, provider_thread_id, incarnation, created_at) VALUES ('binding-legacy', 'session-legacy', 'runtime-legacy', 'thread-legacy', 1, '2026-09-06T00:00:00Z')", [])
            .unwrap();
    connection
            .execute("INSERT INTO turns (id, binding_id, client_submission_id, provider_turn_id, state, created_at, updated_at) VALUES ('turn-legacy', 'binding-legacy', 'submission-legacy', 'provider-turn-legacy', 'completed', '2026-09-06T00:00:00Z', '2026-09-06T00:00:00Z')", [])
            .unwrap();
    connection
            .execute("INSERT INTO transcript_items (id, binding_id, turn_id, provider_item_id, item_kind, payload_json, is_final, created_at, updated_at) VALUES ('item-legacy', 'binding-legacy', 'turn-legacy', 'provider-item-legacy', 'agentMessage', '{\"text\":\"preserved\"}', 1, '2026-09-06T00:00:00Z', '2026-09-06T00:00:00Z')", [])
            .unwrap();
    connection
            .execute("INSERT INTO change_log (operation_id, entity_type, entity_id, action, project_id, session_id, details_json, created_at) VALUES ('operation-legacy', 'session', 'session-legacy', 'session_created', 'legacy-project', 'session-legacy', '{}', '2026-09-06T00:00:00Z')", [])
            .unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    drop(connection);

    let store = Store::open_at(&root).unwrap();
    let workspace = store.workspace("legacy-project").unwrap();
    assert_eq!(workspace.name, "legacy-project");
    assert_eq!(workspace.root.as_deref(), Some(root_text.as_str()));
    assert!(workspace.missing_root_at.is_none());
    let snapshot = store.session_snapshot("session-legacy").unwrap();
    assert_eq!(
        snapshot.session.workspace_id.as_deref(),
        Some("legacy-project")
    );
    assert_eq!(snapshot.workspace.unwrap().id, "legacy-project");
    let connection = store.connection().unwrap();
    assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
    for table in ["session_bindings", "turns", "transcript_items"] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1, "{table} row should survive migration");
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT workspace_id FROM change_log WHERE operation_id = 'operation-legacy'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "legacy-project"
    );
    assert!(!connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'project_workspaces')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .unwrap());
    assert!(connection
        .query_row("PRAGMA foreign_key_check", [], |row| row
            .get::<_, String>(0))
        .optional()
        .unwrap()
        .is_none());
    assert_eq!(std::fs::read_dir(root.join("backups")).unwrap().count(), 1);
    assert_eq!(std::fs::read(review_state).unwrap(), b"review-owned");
}

#[test]
fn failed_migration_rolls_back_and_preserves_a_usable_backup() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workbench");
    std::fs::create_dir_all(&root).unwrap();
    let database = root.join("research.sqlite3");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute("CREATE TABLE legacy_fixture (value TEXT)", [])
        .unwrap();
    connection
        .execute("INSERT INTO legacy_fixture VALUES ('kept')", [])
        .unwrap();
    drop(connection);

    secure_directory(&root.join("backups")).unwrap();
    let store = Store {
        root: root.clone(),
        database: database.clone(),
        backups: root.join("backups"),
    };
    let error = store
        .initialize_with(|connection, _version| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            transaction
                .execute("CREATE TABLE partial_migration (value TEXT)", [])
                .unwrap();
            Err(WorkbenchError::storage(
                "Injected migration failure",
                "fixture",
            ))
        })
        .unwrap_err();
    assert_eq!(error.code, "storage_error");

    let connection = Connection::open(&database).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT value FROM legacy_fixture", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap(),
        "kept"
    );
    assert!(!connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'partial_migration')",
            [],
            |row| row.get::<_, bool>(0)
        )
        .unwrap());

    let backup = std::fs::read_dir(root.join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let backup = Connection::open(backup).unwrap();
    assert_eq!(
        backup
            .query_row("SELECT value FROM legacy_fixture", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap(),
        "kept"
    );
}

#[test]
fn workspace_and_session_lifecycle_survive_reopen_without_review_state() {
    let (temporary, store, workspace_root) = fixture();
    let review_state = temporary.path().join("review-project.json");
    std::fs::write(&review_state, b"review-owned").unwrap();
    let workspace = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Macro paper".to_string(),
            root: Some(workspace_root.to_string_lossy().into_owned()),
            operation_id: "workspace-create-1".to_string(),
        })
        .unwrap();
    assert_eq!(workspace.record.revision, 1);
    assert!(workspace.record.id.starts_with("workspace_"));

    let created = store
        .create_session(CreateSessionRequest {
            workspace_id: Some(workspace.record.id.clone()),
            title: "Identification notes".to_string(),
            operation_id: "session-create-1".to_string(),
        })
        .unwrap();
    let updated = store
        .update_session(UpdateSessionRequest {
            session_id: created.record.id.clone(),
            expected_revision: 1,
            operation_id: "session-update-1".to_string(),
            title: None,
            draft: Some("Check the normalization.".to_string()),
            overrides: Some(json!({ "reasoningEffort": "high" })),
            archived: Some(true),
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
    assert_eq!(updated.record.revision, 2);
    assert!(updated.record.archived_at.is_some());
    assert!(store
        .list_sessions(Some(&workspace.record.id), false)
        .unwrap()
        .sessions
        .is_empty());
    assert_eq!(
        store
            .list_sessions(Some(&workspace.record.id), true)
            .unwrap()
            .sessions
            .len(),
        1
    );

    let reopened = Store::open_at(&store.root).unwrap();
    let snapshot = reopened.session_snapshot(&created.record.id).unwrap();
    assert_eq!(snapshot.workspace.as_ref().unwrap().id, workspace.record.id);
    assert_eq!(snapshot.session.draft, "Check the normalization.");
    assert_eq!(snapshot.session.overrides["reasoningEffort"], "high");
    assert!(snapshot.sequence >= 3);

    let stale = reopened
        .update_session(UpdateSessionRequest {
            session_id: created.record.id,
            expected_revision: 1,
            operation_id: "stale-update".to_string(),
            title: Some("Stale".to_string()),
            draft: None,
            overrides: None,
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap_err();
    assert_eq!(stale.code, "storage_conflict");

    let archived_workspace = reopened
        .update_workspace(UpdateWorkspaceRequest {
            workspace_id: workspace.record.id.clone(),
            expected_revision: 1,
            operation_id: "workspace-archive-1".to_string(),
            name: Some("Renamed macro paper".to_string()),
            archived: Some(true),
        })
        .unwrap();
    assert!(archived_workspace.record.archived_at.is_some());
    assert!(reopened
        .list_workspaces(false)
        .unwrap()
        .workspaces
        .is_empty());
    assert_eq!(reopened.list_workspaces(true).unwrap().workspaces.len(), 1);
    assert_eq!(std::fs::read(review_state).unwrap(), b"review-owned");
}

#[test]
fn rootless_and_unfiled_conversations_are_first_class() {
    let (_temporary, store, workspace_root) = fixture();
    let workspace = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Ideas".to_string(),
            root: None,
            operation_id: "rootless-workspace".to_string(),
        })
        .unwrap();
    assert!(workspace.record.root.is_none());
    let rooted = store
        .register_workspace_root(RegisterWorkspaceRootRequest {
            workspace_id: workspace.record.id.clone(),
            root: workspace_root.to_string_lossy().into_owned(),
            operation_id: "register-root".to_string(),
            expected_revision: 1,
        })
        .unwrap();
    assert!(rooted.record.root.is_some());
    let rootless = store
        .clear_workspace_root(ClearWorkspaceRootRequest {
            workspace_id: workspace.record.id.clone(),
            operation_id: "clear-root".to_string(),
            expected_revision: 2,
        })
        .unwrap();
    assert!(rootless.record.root.is_none());
    assert!(rootless.record.root_identity.is_none());
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Unfiled chat".to_string(),
            operation_id: "unfiled-session".to_string(),
        })
        .unwrap();
    assert!(session.record.workspace_id.is_none());
    assert!(store
        .session_snapshot(&session.record.id)
        .unwrap()
        .workspace
        .is_none());
    assert_eq!(store.list_sessions(None, false).unwrap().sessions.len(), 1);
    assert!(store
        .list_sessions(Some(&workspace.record.id), false)
        .unwrap()
        .sessions
        .is_empty());
}

#[test]
fn root_reconciliation_tracks_the_folder_not_review_metadata() {
    let (_temporary, store, workspace_root) = fixture();
    let workspace = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Empirical project".to_string(),
            root: Some(workspace_root.to_string_lossy().into_owned()),
            operation_id: "workspace-for-root-check".to_string(),
        })
        .unwrap();
    std::fs::remove_dir(&workspace_root).unwrap();
    let missing = store
        .reconcile_workspace_roots("reconcile-missing-root")
        .unwrap();
    assert_eq!(missing.missing, 1);
    assert!(store
        .workspace(&workspace.record.id)
        .unwrap()
        .missing_root_at
        .is_some());

    std::fs::create_dir(&workspace_root).unwrap();
    let restored = store
        .reconcile_workspace_roots("reconcile-restored-root")
        .unwrap();
    assert_eq!(restored.restored, 1);
    assert!(store
        .workspace(&workspace.record.id)
        .unwrap()
        .missing_root_at
        .is_none());
}

#[test]
fn rejects_workspace_overlap_and_duplicate_operations() {
    let (_temporary, store, workspace_root) = fixture();
    let overlap = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Overlap".to_string(),
            root: Some(store.root.to_string_lossy().into_owned()),
            operation_id: "overlap".to_string(),
        })
        .unwrap_err();
    assert_eq!(overlap.code, "invalid_input");

    let request = CreateWorkspaceRequest {
        name: "Macro project".to_string(),
        root: Some(workspace_root.to_string_lossy().into_owned()),
        operation_id: "duplicate-op".to_string(),
    };
    store.create_workspace(request.clone()).unwrap();
    let duplicate = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Duplicate".to_string(),
            operation_id: request.operation_id,
        })
        .unwrap_err();
    assert_eq!(duplicate.code, "storage_conflict");
}

#[test]
fn rust_dto_accepts_the_shared_frontend_fixture() {
    let fixture = include_str!("../../../../tests/fixtures/workbench/store-dto-v2.json");
    let snapshot: SessionSnapshot = serde_json::from_str(fixture).unwrap();
    assert_eq!(snapshot.workspace.as_ref().unwrap().id, "workspace_fixture");
    assert_eq!(snapshot.session.id, "session_fixture");
    assert_eq!(snapshot.session.overrides["reasoningEffort"], "high");
    assert_eq!(snapshot.sequence, 12);
    let encoded = serde_json::to_value(snapshot).unwrap();
    assert!(encoded
        .get("workspace")
        .unwrap()
        .get("rootIdentity")
        .is_some());
    assert!(encoded.get("session").unwrap().get("workspaceId").is_some());
}

#[test]
fn completed_codex_events_reconcile_into_durable_app_owned_projection() {
    use crate::workbench::codex::NormalizedEvent;

    let (_temporary, store, workspace_root) = fixture();
    let workspace = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Projection workspace".to_string(),
            root: Some(workspace_root.to_string_lossy().into_owned()),
            operation_id: "projection-workspace".to_string(),
        })
        .unwrap()
        .record;
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: Some(workspace.id),
            title: "Durable transcript".to_string(),
            operation_id: "projection-session".to_string(),
        })
        .unwrap()
        .record;
    let binding = store
        .bind_session(&session.id, "runtime-7", "thread-provider-1")
        .unwrap();
    let started = NormalizedEvent::TurnStarted {
        epoch: 7,
        thread_id: "thread-provider-1".to_string(),
        turn_id: "turn-provider-1".to_string(),
        turn: json!({
            "id":"turn-provider-1",
            "status":"inProgress",
            "items":[]
        }),
    };
    store.project_codex_event(&binding.id, &started).unwrap();
    store
        .record_turn_submission(&binding.id, "submission-1", "turn-provider-1")
        .unwrap();
    let completed_item = NormalizedEvent::ItemCompleted {
        epoch: 7,
        thread_id: "thread-provider-1".to_string(),
        turn_id: "turn-provider-1".to_string(),
        item_id: "item-provider-1".to_string(),
        item_kind: "agentMessage".to_string(),
        item: json!({
            "id":"item-provider-1",
            "type":"agentMessage",
            "text":"The normalization is locally identified."
        }),
    };
    let first_sequence = store
        .project_codex_event(&binding.id, &completed_item)
        .unwrap()
        .unwrap();
    assert_eq!(
        store
            .project_codex_event(&binding.id, &completed_item)
            .unwrap(),
        Some(first_sequence),
        "replayed provider events must be idempotent"
    );
    let completed_turn = NormalizedEvent::TurnCompleted {
        epoch: 7,
        thread_id: "thread-provider-1".to_string(),
        turn_id: "turn-provider-1".to_string(),
        status: "completed".to_string(),
        turn: json!({
            "id":"turn-provider-1",
            "status":"completed",
            "items":[{
                "id":"item-provider-1",
                "type":"agentMessage",
                "text":"The final normalization result."
            }]
        }),
    };
    store
        .project_codex_event(&binding.id, &completed_turn)
        .unwrap();

    let reopened = Store::open_at(&store.root).unwrap();
    let connection = reopened.connection().unwrap();
    assert_eq!(
            connection
                .query_row(
                    "SELECT state FROM turns WHERE binding_id = ?1 AND provider_turn_id = 'turn-provider-1' AND client_submission_id = 'submission-1'",
                    [&binding.id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "completed"
        );
    let (payload, is_final): (String, bool) = connection
            .query_row(
                "SELECT payload_json, is_final FROM transcript_items WHERE binding_id = ?1 AND provider_item_id = 'item-provider-1'",
                [&binding.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
    assert!(is_final);
    assert_eq!(
        serde_json::from_str::<Value>(&payload).unwrap()["text"],
        "The final normalization result."
    );
    let hydrated = reopened.conversation_snapshot(&session.id).unwrap();
    assert_eq!(hydrated.active_binding.as_ref().unwrap().id, binding.id);
    assert_eq!(hydrated.turns.len(), 1);
    assert_eq!(hydrated.turns[0].state, "completed");
    assert_eq!(hydrated.items.len(), 1);
    assert_eq!(
        hydrated.items[0].payload["text"],
        "The final normalization result."
    );

    let resumed = reopened
        .bind_session(&session.id, "runtime-8", "thread-provider-1")
        .unwrap();
    reopened
        .project_codex_event(
            &resumed.id,
            &NormalizedEvent::ItemCompleted {
                epoch: 8,
                thread_id: "thread-provider-1".to_string(),
                turn_id: "turn-provider-1".to_string(),
                item_id: "item-provider-1".to_string(),
                item_kind: "agentMessage".to_string(),
                item: json!({
                    "id":"item-provider-1",
                    "type":"agentMessage",
                    "text":"The reconciled result."
                }),
            },
        )
        .unwrap();
    let reconciled = reopened.conversation_snapshot(&session.id).unwrap();
    assert_eq!(reconciled.items.len(), 1);
    assert_eq!(
        reconciled.items[0].payload["text"],
        "The reconciled result."
    );
}

#[test]
fn migration_eight_keeps_project_record_history_trigger_and_new_kinds() {
    let (_temporary, store, _workspace_root) = fixture();
    let workspace = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Theory".into(),
            root: None,
            operation_id: "create".into(),
        })
        .unwrap()
        .record;
    let connection = store.connection().unwrap();
    for kind in ["theory", "check", "direction"] {
        connection
            .execute(
                "INSERT INTO project_records(id,workspace_id,kind,revision,body_json,updated_at) VALUES(?1,?2,?3,1,'{}','t1')",
                rusqlite::params![format!("{kind}_1"), workspace.id, kind],
            )
            .unwrap();
    }
    assert!(connection
        .execute(
            "INSERT INTO project_records(id,workspace_id,kind,revision,body_json,updated_at) VALUES('bad',?1,'unknown',1,'{}','t1')",
            [&workspace.id],
        )
        .is_err());
    connection
        .execute(
            "UPDATE project_records SET revision=2, body_json='{\"a\":1}', updated_at='t2' WHERE id='theory_1'",
            [],
        )
        .unwrap();
    let (revision, body): (i64, String) = connection
        .query_row(
            "SELECT revision, body_json FROM project_record_history WHERE object_id='theory_1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((revision, body.as_str()), (1, "{}"));
}

#[test]
fn moving_a_conversation_refiles_it_and_clears_workspace_bound_context() {
    let (_temporary, store, workspace_root) = fixture();
    let workspace = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Labor".to_string(),
            root: Some(workspace_root.to_string_lossy().into_owned()),
            operation_id: "move-workspace".to_string(),
        })
        .unwrap()
        .record;
    let unfiled = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "New conversation".to_string(),
            operation_id: "move-session".to_string(),
        })
        .unwrap()
        .record;
    let wrong_target = store
        .move_session(MoveSessionRequest {
            session_id: unfiled.id.clone(),
            expected_revision: unfiled.revision,
            operation_id: "move-same".to_string(),
            workspace_id: None,
        })
        .unwrap_err();
    assert_eq!(wrong_target.code, "invalid_input");
    let moved = store
        .move_session(MoveSessionRequest {
            session_id: unfiled.id.clone(),
            expected_revision: unfiled.revision,
            operation_id: "move-into".to_string(),
            workspace_id: Some(workspace.id.clone()),
        })
        .unwrap()
        .record;
    assert_eq!(moved.workspace_id.as_deref(), Some(workspace.id.as_str()));
    assert_eq!(moved.revision, unfiled.revision + 1);
    assert!(store
        .list_sessions(Some(&workspace.id), false)
        .unwrap()
        .sessions
        .iter()
        .any(|session| session.id == unfiled.id));
    assert!(store
        .list_sessions(None, false)
        .unwrap()
        .sessions
        .is_empty());

    // A stale revision conflicts; a task conversation stays with its project.
    assert_eq!(
        store
            .move_session(MoveSessionRequest {
                session_id: unfiled.id.clone(),
                expected_revision: unfiled.revision,
                operation_id: "move-stale".to_string(),
                workspace_id: None,
            })
            .unwrap_err()
            .code,
        "storage_conflict"
    );
    let task = store
        .update_session(UpdateSessionRequest {
            session_id: unfiled.id.clone(),
            expected_revision: moved.revision,
            operation_id: "move-task".to_string(),
            title: None,
            draft: None,
            overrides: Some(json!({"projectCheckpointId":"checkpoint-1"})),
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap()
        .record;
    assert_eq!(
        store
            .move_session(MoveSessionRequest {
                session_id: task.id.clone(),
                expected_revision: task.revision,
                operation_id: "move-task-out".to_string(),
                workspace_id: None,
            })
            .unwrap_err()
            .code,
        "invalid_input"
    );
}

#[test]
fn deleting_a_conversation_removes_its_native_projection_graph() {
    use crate::workbench::codex::NormalizedEvent;
    let (_temporary, store, _workspace_root) = fixture();
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Disposable".to_string(),
            operation_id: "delete-session-create".to_string(),
        })
        .unwrap()
        .record;
    let binding = store
        .bind_session(&session.id, "runtime-9", "thread-delete-1")
        .unwrap();
    store
        .record_turn_submission(&binding.id, "submission-1", "turn-delete-1")
        .unwrap();
    store
        .project_codex_event(
            &binding.id,
            &NormalizedEvent::ItemCompleted {
                epoch: 9,
                thread_id: "thread-delete-1".to_string(),
                turn_id: "turn-delete-1".to_string(),
                item_id: "item-delete-1".to_string(),
                item_kind: "agentMessage".to_string(),
                item: json!({"id":"item-delete-1","type":"agentMessage","text":"gone"}),
            },
        )
        .unwrap();
    assert_eq!(
        store
            .session_for_thread("runtime-9", "thread-delete-1")
            .unwrap()
            .as_deref(),
        Some(session.id.as_str())
    );
    store
        .delete_session(DeleteSessionRequest {
            session_id: session.id.clone(),
            operation_id: "delete-session".to_string(),
        })
        .unwrap();
    assert!(store.session_snapshot(&session.id).is_err());
    assert_eq!(
        store
            .session_for_thread("runtime-9", "thread-delete-1")
            .unwrap(),
        None
    );
    let connection = store.connection().unwrap();
    for table in ["session_bindings", "turns", "transcript_items"] {
        let remaining: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(remaining, 0, "{table} should be empty after deletion");
    }
    assert_eq!(
        store
            .delete_session(DeleteSessionRequest {
                session_id: session.id,
                operation_id: "delete-session".to_string(),
            })
            .unwrap_err()
            .code,
        "storage_conflict",
        "a replayed operation id is rejected"
    );
}

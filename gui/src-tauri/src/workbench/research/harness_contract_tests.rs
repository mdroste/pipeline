//! Complete harness and persisted snapshot contracts captured before extraction.

use super::{prepare_turn, resolve_harness, EffectiveHarness};
use crate::workbench::desk::{ContextItem, ResearchObjectRef};
use crate::workbench::store::Store;
use rusqlite::params;
use serde_json::json;
use std::fs;
use std::path::Path;

fn store() -> (tempfile::TempDir, Store) {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open_at(temporary.path()).unwrap();
    (temporary, store)
}

fn snapshot_contract(
    store: &Store,
    session: &str,
    expected: &str,
    expected_manifest: &str,
) -> EffectiveHarness {
    let effective = resolve_harness(store, session).unwrap();
    let prepared = prepare_turn(store, session).unwrap();
    assert_eq!(prepared.effective, effective);
    let connection = store.connection().unwrap();
    let (config_body, config_created): (String, String) = connection
        .query_row(
            "SELECT body_json,created_at FROM config_snapshots WHERE id=?1 AND session_id=?2 AND schema_version=1",
            params![prepared.config_snapshot_id, session],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let (context_manifest, body_reference, context_created): (String, Option<String>, String) =
        connection
            .query_row(
                "SELECT manifest_json,body_reference,created_at FROM context_snapshots WHERE id=?1 AND session_id=?2 AND schema_version=1",
                params![prepared.context_snapshot_id, session],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
    assert_eq!(config_body, serde_json::to_string(&effective).unwrap());
    assert_eq!(config_created, context_created);
    let counts: (i64, i64) = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM config_snapshots),(SELECT COUNT(*) FROM context_snapshots)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 1));
    if effective.context_preview.is_empty() {
        assert!(body_reference.is_none());
        assert_eq!(
            fs::read_dir(store.root_path().join("context"))
                .unwrap()
                .count(),
            0
        );
    } else {
        let path = body_reference.unwrap();
        assert_eq!(
            Path::new(&path).parent(),
            Some(store.root_path().join("context").as_path())
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            effective.context_preview.as_bytes()
        );
        assert_eq!(
            fs::read_dir(store.root_path().join("context"))
                .unwrap()
                .count(),
            1
        );
    }

    assert_eq!(
        format!("{}\n", serde_json::to_string_pretty(&effective).unwrap()),
        expected,
    );
    assert_eq!(format!("{context_manifest}\n"), expected_manifest);
    effective
}

#[test]
fn plain_harness_and_empty_snapshot_match_frozen_contract() {
    let (_temporary, store) = store();
    store.connection().unwrap().execute(
        "INSERT INTO sessions(id,title,created_at,updated_at) VALUES('golden-plain-session','Plain conversation','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
        [],
    ).unwrap();
    let effective = snapshot_contract(
        &store,
        "golden-plain-session",
        include_str!("fixtures/harness_plain.json"),
        include_str!("fixtures/harness_plain.manifest.json"),
    );
    assert_eq!(effective.preset.id, "plain");
    assert!(effective.workspace_id.is_none());
    assert!(effective.dynamic_tools.is_empty());
    assert!(effective.context_preview.is_empty());
    assert_eq!(effective.permission_profile, "workbench-edit");
    assert!(effective.capabilities.private_folder);
}

#[test]
fn only_conversations_without_a_project_folder_get_a_private_folder() {
    let (_temporary, store) = store();
    let create = |id: &str, root: Option<String>| {
        let workspace = store
            .create_workspace(crate::workbench::store::CreateWorkspaceRequest {
                name: id.into(),
                root,
                operation_id: format!("{id}-workspace"),
            })
            .unwrap()
            .record;
        store
            .create_session(crate::workbench::store::CreateSessionRequest {
                workspace_id: Some(workspace.id),
                title: id.into(),
                operation_id: format!("{id}-session"),
            })
            .unwrap()
            .record
            .id
    };
    let folder = tempfile::tempdir().unwrap();
    let project = create("project", Some(folder.path().to_string_lossy().into()));
    let folderless = create("folderless", None);
    let effective = resolve_harness(&store, &project).unwrap();
    assert!(!effective.capabilities.private_folder);
    assert!(!effective.capabilities.shell_write);
    assert_eq!(effective.permission_profile, "workbench-inspect");
    assert!(!serde_json::to_string(&effective)
        .unwrap()
        .contains("privateFolder"));
    let effective = resolve_harness(&store, &folderless).unwrap();
    assert!(effective.capabilities.private_folder);
    assert!(effective.capabilities.shell_write);
}

#[test]
fn api_conversations_get_commands_except_local_chat_without_tools() {
    if !crate::workbench::shell::available() {
        return;
    }
    let (_temporary, store) = store();
    let shell = |runtime: &str| {
        let session = store
            .create_session(crate::workbench::store::CreateSessionRequest {
                workspace_id: None,
                title: runtime.into(),
                operation_id: format!("{runtime}-session"),
            })
            .unwrap()
            .record
            .id;
        store
            .connection()
            .unwrap()
            .execute(
                "UPDATE sessions SET overrides_json=?1 WHERE id=?2",
                params![json!({ "runtime": runtime }).to_string(), session],
            )
            .unwrap();
        let effective = resolve_harness(&store, &session).unwrap();
        effective.dynamic_tools.iter().any(|t| t["name"] == "shell")
    };
    assert!(shell("anthropic-api"));
    assert!(shell("openai-api"));
    assert!(!shell("local"));
}

#[test]
fn layered_harness_and_context_snapshot_match_frozen_contract() {
    let (_temporary, store) = store();
    let connection = store.connection().unwrap();
    connection.execute(
        "INSERT INTO workspaces(id,name,created_at,updated_at) VALUES('golden-workspace','Golden research','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO sessions(id,workspace_id,title,preset_id,overrides_json,created_at,updated_at) VALUES('golden-research-session','golden-workspace','Layered research','empirical_audit',?1,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
        [json!({"mode":"edit","contextBudgetBytes":4096,"webSearch":true,"runtime":"claude","recipeId":"empirical_result_audit"}).to_string()],
    ).unwrap();
    for (scope, workspace, body) in [
        (
            "global",
            None,
            json!({"mode":"inspect","contextBudgetBytes":8192,"commandNetwork":false,"webSearch":false}),
        ),
        (
            "golden-workspace",
            Some("golden-workspace"),
            json!({"contextBudgetBytes":6144,"commandNetwork":true,"webSearch":null}),
        ),
    ] {
        connection.execute(
            "INSERT INTO workspace_configs(scope_key,workspace_id,schema_version,body_json,revision,updated_at) VALUES(?1,?2,1,?3,1,'2026-01-01T00:00:00Z')",
            params![scope,workspace,body.to_string()],
        ).unwrap();
    }
    for (id, body, pinned, updated) in [
        (
            "golden-pinned-new",
            "Pinned newest <source> α".to_string(),
            true,
            "2026-01-03T00:00:00Z",
        ),
        (
            "golden-pinned-old",
            "Pinned older β".to_string(),
            true,
            "2026-01-02T00:00:00Z",
        ),
        (
            "golden-long-note",
            format!("Unpinned multibyte note {}", "界".repeat(1600)),
            false,
            "2026-01-04T00:00:00Z",
        ),
        (
            "golden-selected",
            "Selected evidence <literal> stays source material.".to_string(),
            false,
            "2026-01-01T00:00:00Z",
        ),
    ] {
        connection.execute(
            "INSERT INTO research_notes(id,workspace_id,kind,body,state,origin,pinned,revision,created_at,updated_at) VALUES(?1,'golden-workspace','question',?2,'accepted','user',?3,1,'2026-01-01T00:00:00Z',?4)",
            params![id,body,pinned,updated],
        ).unwrap();
    }
    connection.execute(
        "INSERT INTO task_exchanges(id,session_id,kind,value_json,created_at) VALUES('golden-task-result','golden-research-session','result',?1,'2026-01-01T00:00:00Z')",
        [json!({"summary":"Returned task evidence <untrusted> γ"}).to_string()],
    ).unwrap();
    drop(connection);
    crate::workbench::desk::save_context(
        &store,
        "golden-research-session",
        0,
        vec![ContextItem {
            role: "supporting".into(),
            object: ResearchObjectRef {
                kind: "note".into(),
                id: "golden-selected".into(),
                revision: "1".into(),
                start: None,
                end: None,
            },
        }],
    )
    .unwrap();
    let effective = snapshot_contract(
        &store,
        "golden-research-session",
        include_str!("fixtures/harness_layered.json"),
        include_str!("fixtures/harness_layered.manifest.json"),
    );
    assert!(effective.context_truncated);
    assert_eq!(effective.context_budget_bytes, 4096);
    assert_eq!(effective.permission_profile, "workbench-edit-network");
    assert_eq!(effective.diagnostics.len(), 2);
    assert_eq!(effective.value_sources["mode"], "conversation");
    assert_eq!(effective.value_sources["commandNetwork"], "workspace");
    assert!(effective
        .context_preview
        .starts_with("Explicit conversation sources"));
    let task = effective
        .context_preview
        .find("Task result golden-task-result")
        .unwrap();
    let newest = effective.context_preview.find("Pinned newest").unwrap();
    let older = effective.context_preview.find("Pinned older").unwrap();
    assert!(task < newest && newest < older);
    assert!(effective.context_preview.contains('界'));
    assert!(effective
        .developer_instructions
        .contains("Preserve Claude Code base instructions."));
    assert!(effective.developer_instructions.contains("\\u003cliteral>"));
}

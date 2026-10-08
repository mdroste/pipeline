use super::*;

#[cfg(unix)] // Unix tools or journalled project writes
#[test]
fn isolated_roles_are_idempotent_private_and_revocable() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open_at(&temporary.path().join("store")).unwrap();
    let session = source(&store, "start-op", "Historical sources", false).unwrap();
    assert_eq!(
        source(&store, "start-op", "Historical sources", false).unwrap(),
        session
    );
    let binding = tasks::binding(&store, &session).unwrap();
    let scope = Scope {
        session_id: Some(session.clone()),
        workspace_id: binding.workspace_id.clone(),
        ..Scope::default()
    };
    let deadline = chrono::Utc::now().timestamp() + 3600;
    let (author, _) = role(&store, &scope, "portfolio", "paper-1", true, deadline).unwrap();
    let (again, _) = role(&store, &scope, "portfolio", "paper-1", true, deadline).unwrap();
    assert_eq!(author.session_id, again.session_id);
    let (referee, _) = role(&store, &scope, "portfolio", "referee-1", false, deadline).unwrap();
    let (other, _) = role(&store, &scope, "portfolio", "paper-2", true, deadline).unwrap();
    assert_ne!(author.runtime_root, referee.runtime_root);
    assert_ne!(author.runtime_root, other.runtime_root);
    assert_ne!(
        author.runtime_root.as_deref(),
        Some(binding.runtime_root.as_str())
    );
    let author_id = author.session_id.as_ref().unwrap();
    let h = research::resolve_harness(&store, author_id).unwrap();
    assert_eq!(h.permission_profile, "discovery-edit");
    assert!(!h.command_network);
    assert_eq!(h.preset.modules, ["paper_tools"]);
    assert!(h.preset.base_instructions.is_none());
    assert_eq!(
        research::resolve_harness(&store, referee.session_id.as_ref().unwrap())
            .unwrap()
            .permission_profile,
        "discovery-inspect"
    );
    let root = runtime_root(&store, author_id).unwrap().unwrap();
    super::super::project::materialize_research_files(
        &root,
        vec![("input.txt".into(), b"captured source".to_vec())],
    )
    .unwrap();
    super::super::project::materialize_research_files(
        &root,
        vec![("input.txt".into(), b"captured source".to_vec())],
    )
    .unwrap();
    assert!(super::super::project::materialize_research_files(
        &root,
        vec![("input.txt".into(), b"changed source".to_vec())]
    )
    .is_err());
    assert_eq!(
        store
            .list_sessions(binding.workspace_id.as_deref(), false)
            .unwrap()
            .sessions
            .len(),
        1
    );
    assert_eq!(tasks::session_choices(&store).unwrap().len(), 1);
    revoke(&store, "portfolio").unwrap();
    assert!(runtime_root(&store, author_id).is_err());
    assert!(research::resolve_harness(&store, author_id).is_err());
    assert!(research::resolve_harness(&store, &session).is_ok());
}

#[test]
fn partial_setup_recovers_and_expired_grants_cannot_be_reused() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open_at(&temporary.path().join("store")).unwrap();
    let operation = "interrupted-start";
    store
        .create_workspace(CreateWorkspaceRequest {
            name: "Self-discovery: test".into(),
            root: None,
            operation_id: format!("discovery-project-{}", key(operation)),
        })
        .unwrap();
    let session = source(&store, operation, "test", true).unwrap();
    assert_eq!(store.list_workspaces(false).unwrap().workspaces.len(), 1);
    let binding = tasks::binding(&store, &session).unwrap();
    assert!(super::super::acquisition::network_enabled(
        &store,
        binding.workspace_id.as_ref().unwrap()
    )
    .unwrap());
    let scope = Scope {
        session_id: Some(session),
        workspace_id: binding.workspace_id,
        ..Scope::default()
    };
    let (author, _) = role(
        &store,
        &scope,
        "portfolio",
        "paper-1",
        true,
        chrono::Utc::now().timestamp() + 3600,
    )
    .unwrap();
    let id = author.session_id.unwrap();
    store
        .connection()
        .unwrap()
        .execute("DELETE FROM discovery_roles WHERE session_id=?1", [&id])
        .unwrap();
    assert_eq!(
        role(
            &store,
            &scope,
            "portfolio",
            "paper-1",
            true,
            chrono::Utc::now().timestamp() + 3600
        )
        .unwrap()
        .0
        .session_id,
        Some(id.clone())
    );
    store
        .connection()
        .unwrap()
        .execute(
            "UPDATE discovery_roles SET deadline_at=0 WHERE session_id=?1",
            [&id],
        )
        .unwrap();
    assert!(role(
        &store,
        &scope,
        "portfolio",
        "paper-1",
        true,
        chrono::Utc::now().timestamp() + 3600
    )
    .is_err());
}

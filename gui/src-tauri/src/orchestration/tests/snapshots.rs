use super::*;

#[test]
fn text_artifacts_are_immutable_and_hash_checked() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let a = adapters::snapshot(&s, &Scope::default(), json!("Original paper"), "paper.md").unwrap();
    let b = adapters::snapshot(&s, &Scope::default(), json!("Revised paper"), "paper.md").unwrap();
    assert_ne!(a["hash"], b["hash"]);
    assert!(adapters::artifact_path(&s, &a).is_ok());
    std::fs::write(a["path"].as_str().unwrap(), "Changed externally").unwrap();
    assert!(adapters::artifact_path(&s, &a).is_err());
    assert!(adapters::snapshot(&s, &Scope::default(), json!("bad"), "../escape").is_err());
}

#[test]
fn file_and_tex_tree_snapshots_are_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("paper");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("main.tex"), "\\input{chapter}").unwrap();
    std::fs::write(source.join("chapter.tex"), "Research").unwrap();
    let s = Store::open(&temp.path().join("tasks")).unwrap();
    let scope = Scope {
        runtime_root: Some(source.to_string_lossy().into()),
        root_identity: Some(crate::workbench::store::root_identity(&source).unwrap()),
        ..Scope::default()
    };
    let artifact = adapters::snapshot(&s, &scope, json!({"path":source}), "paper").unwrap();
    assert_eq!(artifact["kind"], "artifactTree");
    assert_eq!(artifact["manifest"].as_array().unwrap().len(), 2);
    assert!(adapters::artifact_path(&s, &artifact).is_ok());
    let relative_tree = adapters::snapshot(&s, &scope, json!({"path":"."}), "paper").unwrap();
    assert_eq!(relative_tree["hash"], artifact["hash"]);
    let relative_file =
        adapters::snapshot(&s, &scope, json!({"path":"chapter.tex"}), "chapter.tex").unwrap();
    assert_eq!(relative_file["kind"], "artifact");
    assert_eq!(
        std::fs::read_to_string(adapters::artifact_path(&s, &relative_file).unwrap()).unwrap(),
        "Research"
    );
    let outside = temp.path().join("outside.md");
    std::fs::write(&outside, "Outside").unwrap();
    assert!(adapters::snapshot(&s, &scope, json!({"path":outside}), "paper.md").is_err());
}

#[test]
fn the_review_template_saves_only_the_assistants_final_answer() {
    use crate::agent_runtime::events::NormalizedEvent;
    use crate::workbench::store::{CreateSessionRequest, Store as Workspace};
    let temp = tempfile::tempdir().unwrap();
    let w = Workspace::open_at(&temp.path().join("workspace")).unwrap();
    let session = w
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Draft".into(),
            operation_id: "draft-session".into(),
        })
        .unwrap()
        .record
        .id;
    let binding = w
        .bind_session(&session, "test-runtime", "thread-1")
        .unwrap();
    w.record_turn_submission(&binding.id, "task-draft", "turn-1")
        .unwrap();
    let items = json!([
        {"id":"a","type":"agentMessage","text":"I'll inspect the project files first","phase":"commentary"},
        {"id":"b","type":"commandExecution","command":"ls"},
        {"id":"c","type":"agentMessage","text":"# The paper\n\nThe argument.","phase":"final_answer"},
    ]);
    w.project_codex_event(
        &binding.id,
        &NormalizedEvent::TurnCompleted {
            epoch: 1,
            thread_id: "thread-1".into(),
            turn_id: "turn-1".into(),
            status: "completed".into(),
            turn: json!({"id":"turn-1","status":"completed","items":items}),
        },
    )
    .unwrap();
    let draft = crate::workbench::tasks::outcome(&w, &session, "task-draft")
        .unwrap()
        .unwrap();

    let fixture = include_str!("../fixtures/review-chain.json");
    assert!(!fixture.contains("\"/text\""));
    let chain: Chain = serde_json::from_str(fixture).unwrap();
    let Action::Snapshot {
        input, filename, ..
    } = &chain.steps[1].action
    else {
        panic!("The template saves the draft second");
    };
    let saved = input.resolve(&json!({"draft":draft}), &json!({})).unwrap();
    let s = Store::open(&temp.path().join("tasks")).unwrap();
    let artifact = adapters::snapshot(&s, &Scope::default(), saved, filename).unwrap();
    assert_eq!(
        std::fs::read_to_string(adapters::artifact_path(&s, &artifact).unwrap()).unwrap(),
        "# The paper\n\nThe argument."
    );
}

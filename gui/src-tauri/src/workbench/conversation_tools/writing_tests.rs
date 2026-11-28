//! The tools that read the researcher's own earlier papers from About you.

use super::*;
use crate::researcher::{self, SampleUpdate};
use crate::workbench::{
    research,
    store::{CreateSessionRequest, UpdateSessionRequest},
};

/// An unfiled conversation on the Writing profile, mid-turn, with one paper
/// marked as an example and one not.
struct Turn {
    _directory: tempfile::TempDir,
    store: Store,
    session: String,
    folder: std::path::PathBuf,
    example: String,
    private: String,
}

fn turn() -> Turn {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open_at(&directory.path().join("workbench")).unwrap();
    let folder = researcher::folder_beside(store.root_path()).unwrap();
    let add = |name: &str, text: &str, example: bool| {
        let path = directory.path().join(name);
        std::fs::write(&path, text).unwrap();
        let sample = researcher::add_sample(&folder, &path).unwrap();
        researcher::update_sample(
            &folder,
            SampleUpdate {
                id: sample.id.clone(),
                title: name.into(),
                year: Some(2024),
                kind: "paper".into(),
                use_as_example: example,
            },
        )
        .unwrap();
        sample.id
    };
    let example = add("example.md", "We study firms. Entry falls.", true);
    let private = add("private.md", "An unpublished draft.", false);
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Draft".into(),
            operation_id: "create".into(),
        })
        .unwrap()
        .record;
    store
        .update_session(UpdateSessionRequest {
            session_id: session.id.clone(),
            expected_revision: session.revision,
            operation_id: "profile".into(),
            title: None,
            draft: None,
            overrides: None,
            archived: None,
            preset_id: Some("writing".into()),
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
    let prepared = research::prepare_turn(&store, &session.id).unwrap();
    let binding = store.bind_session(&session.id, "test", "thread").unwrap();
    store
        .record_turn_submission(&binding.id, "submission", "turn")
        .unwrap();
    store
        .attach_turn_snapshots(
            &binding.id,
            "submission",
            &prepared.config_snapshot_id,
            &prepared.context_snapshot_id,
        )
        .unwrap();
    Turn {
        _directory: directory,
        store,
        session: session.id,
        folder,
        example,
        private,
    }
}

fn call(id: &str, tool: &str, arguments: Value) -> Value {
    json!({"threadId":"thread","turnId":"turn","callId":id,"tool":tool,"arguments":arguments})
}

#[test]
fn the_tools_are_ours_and_admitted_while_the_turn_offers_them() {
    assert!(owns("writing_samples") && owns("writing_sample_read"));
    let turn = turn();
    for (id, tool) in [("list", "writing_samples"), ("read", "writing_sample_read")] {
        let arguments = json!({"id": turn.example});
        assert!(matches!(
            prepare(&turn.store, &call(id, tool, arguments)).unwrap(),
            Admission::Run(_)
        ));
    }
    // Turning About you off for this conversation withdraws them.
    let session = turn.store.session_snapshot(&turn.session).unwrap().session;
    turn.store
        .update_session(UpdateSessionRequest {
            session_id: session.id,
            expected_revision: session.revision,
            operation_id: "about-you-off".into(),
            title: None,
            draft: None,
            overrides: Some(json!({"aboutYou": false})),
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
    let error = match prepare(&turn.store, &call("after", "writing_samples", json!({}))) {
        Err(error) => error,
        Ok(_) => panic!("a withdrawn tool was admitted"),
    };
    assert!(error.message.contains("unavailable"), "{}", error.message);
}

#[test]
fn only_examples_can_be_listed_or_read() {
    let turn = turn();
    let listed = writing_tool(&turn.store, "writing_samples", &json!({})).unwrap();
    let samples = listed["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0]["id"], json!(turn.example));
    assert_eq!(samples[0]["words"], json!(5));
    assert!(listed["use"].as_str().unwrap().contains("not sources"));

    let read = writing_tool(
        &turn.store,
        "writing_sample_read",
        &json!({"id": turn.example, "start": 16, "length": 11}),
    )
    .unwrap();
    assert_eq!(read["text"], "Entry falls");
    assert_eq!(read["title"], "example.md");
    assert_eq!(
        (read["start"].clone(), read["end"].clone()),
        (json!(16), json!(27))
    );

    for id in [turn.private.as_str(), "../secret", ""] {
        let error =
            writing_tool(&turn.store, "writing_sample_read", &json!({"id": id})).unwrap_err();
        assert!(
            error.message.contains("style examples"),
            "{id}: {}",
            error.message
        );
    }
    assert!(writing_tool(&turn.store, "writing_sample_read", &json!({})).is_err());

    // Unmarking the example takes it away from the tools at once.
    researcher::update_sample(
        &turn.folder,
        SampleUpdate {
            id: turn.example.clone(),
            title: "example.md".into(),
            year: None,
            kind: "paper".into(),
            use_as_example: false,
        },
    )
    .unwrap();
    let listed = writing_tool(&turn.store, "writing_samples", &json!({})).unwrap();
    assert_eq!(listed["samples"], json!([]));
}

#[test]
fn a_store_outside_a_data_folder_has_no_writing_to_read() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open_at(directory.path()).unwrap();
    let error = writing_tool(&store, "writing_samples", &json!({})).unwrap_err();
    assert!(error.message.contains("unavailable"), "{}", error.message);
}

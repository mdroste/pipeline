use super::*;

#[test]
fn legacy_reference_resolves_document_owner_without_advancing_reference() {
    let f = test_support::fixture();
    let path = f.root.join("manuscript.txt");
    fs::write(&path, "Original argument").unwrap();
    let import = |paper_id, operation: &str| {
        research::import_paper(
            &f.store,
            research::ImportPaperRequest {
                workspace_id: f.ws.clone(),
                paper_id,
                title: "Manuscript".into(),
                role: "manuscript".into(),
                document_type: Some("paper".into()),
                path: path.to_string_lossy().into_owned(),
                operation_id: operation.into(),
            },
        )
        .unwrap()
    };
    let first = import(None, "first");
    let reference = first.revision.unwrap().id;
    put(
        &f.store,
        &f.ws,
        &format!("home_{}", f.ws),
        "home",
        0,
        &json!({
            "manuscriptRevisionId": reference, "baselineExecutionId": null
        }),
    )
    .unwrap();
    fs::write(&path, "New working argument").unwrap();
    let next = import(Some(first.paper.id.clone()), "next");
    let home = home(&f.store, &f.ws).unwrap();
    assert_eq!(home.settings.body["primaryDocumentId"], first.paper.id);
    assert_eq!(home.settings.body["manuscriptRevisionId"], reference);
    assert_eq!(
        selected_manuscript(&f.store, &f.ws).unwrap(),
        Some(reference)
    );
    assert_eq!(
        home.papers[0].revision.as_ref().unwrap().id,
        next.revision.unwrap().id
    );
}

#[test]
fn explicit_primary_clear_does_not_reselect_the_reference_owner() {
    let f = test_support::fixture();
    put(
        &f.store,
        &f.ws,
        &format!("home_{}", f.ws),
        "home",
        0,
        &json!({
            "primaryDocumentId": null, "manuscriptRevisionId": null, "baselineExecutionId": null
        }),
    )
    .unwrap();
    assert!(home(&f.store, &f.ws).unwrap().settings.body["primaryDocumentId"].is_null());
}

#[test]
fn an_archived_primary_document_does_not_block_other_preferences() {
    let f = test_support::fixture();
    let path = f.root.join("manuscript.txt");
    fs::write(&path, "Argument").unwrap();
    let paper = research::import_paper(
        &f.store,
        research::ImportPaperRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            title: "Manuscript".into(),
            role: "manuscript".into(),
            document_type: Some("paper".into()),
            path: path.to_string_lossy().into_owned(),
            operation_id: "import".into(),
        },
    )
    .unwrap()
    .paper;
    let chosen = save_home(
        &f.store,
        &f.ws,
        0,
        ProjectHomeSettings {
            primary_document_id: Some(paper.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    research::update_document(
        &f.store,
        research::UpdateDocumentRequest {
            workspace_id: f.ws.clone(),
            paper_id: paper.id.clone(),
            expected_updated_at: paper.updated_at.clone(),
            title: paper.title.clone(),
            document_type: "paper".into(),
            pinned: false,
            archived: true,
            current_revision_id: None,
            operation_id: "archive".into(),
        },
    )
    .unwrap();
    // The settings form resends the stale reference with an unrelated change.
    let mut settings: ProjectHomeSettings = serde_json::from_value(chosen.body.clone()).unwrap();
    settings.venue = "Journal of Studies".into();
    let saved = save_home(&f.store, &f.ws, chosen.revision, settings).unwrap();
    assert!(saved.body["primaryDocumentId"].is_null());
    assert_eq!(saved.body["venue"], "Journal of Studies");
    // Choosing the archived document anew is still refused.
    let mut settings: ProjectHomeSettings = serde_json::from_value(saved.body.clone()).unwrap();
    settings.primary_document_id = Some(paper.id.clone());
    assert!(save_home(&f.store, &f.ws, saved.revision, settings).is_err());
}

use super::*;

fn add(
    f: &Fixture,
    paper_id: Option<String>,
    kind: Option<&str>,
    text: &str,
    operation: &str,
) -> PaperWithRevision {
    let path = f.root.join("slides.tex");
    fs::write(&path, text).unwrap();
    import_paper(
        &f.store,
        ImportPaperRequest {
            workspace_id: f.workspace_id.clone(),
            paper_id,
            title: "Seminar".into(),
            role: "manuscript".into(),
            document_type: kind.map(str::to_owned),
            path: path.to_string_lossy().into_owned(),
            operation_id: operation.into(),
        },
    )
    .unwrap()
}
fn edit(f: &Fixture, p: &Paper) -> UpdateDocumentRequest {
    UpdateDocumentRequest {
        workspace_id: f.workspace_id.clone(),
        paper_id: p.id.clone(),
        expected_updated_at: p.updated_at.clone(),
        title: "Conference presentation".into(),
        document_type: "slides".into(),
        pinned: true,
        archived: false,
        current_revision_id: None,
        operation_id: "edit-document".into(),
    }
}
#[test]
fn document_types_survive_new_and_reimported_versions() {
    let f = fixture();
    let first = add(&f, None, Some("slides"), "Version one", "first");
    assert_eq!(first.paper.document_type, "slides");
    assert_eq!(first.paper.role, "other");
    let second = add(
        &f,
        Some(first.paper.id.clone()),
        None,
        "Version two",
        "second",
    );
    assert_eq!(second.paper.document_type, "slides");
    let other = add(
        &f,
        None,
        Some("paper"),
        "Same project, another document",
        "other",
    );
    let history = document_versions(&f.store, &f.workspace_id, &first.paper.id).unwrap();
    assert_eq!(history.len(), 2);
    assert!(history.iter().all(|r| r.paper_id == first.paper.id));
    let restored = add(
        &f,
        Some(first.paper.id.clone()),
        None,
        "Version one",
        "restore",
    );
    assert_eq!(restored.revision.unwrap().id, first.revision.unwrap().id);
    assert_eq!(restored.paper.document_type, "slides");
    assert_eq!(
        document_versions(&f.store, &f.workspace_id, &other.paper.id)
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn metadata_edits_preserve_versions_and_reject_stale_or_cross_document_writes() {
    let f = fixture();
    let first = add(&f, None, Some("paper"), "Version one", "first");
    let second = add(
        &f,
        Some(first.paper.id.clone()),
        None,
        "Version two",
        "second",
    );
    let updated = update_document(&f.store, edit(&f, &second.paper)).unwrap();
    assert_eq!(updated.paper.id, first.paper.id);
    assert_eq!(updated.paper.document_type, "slides");
    assert_eq!(updated.paper.role, "other");
    assert!(updated.paper.pinned);
    assert_eq!(
        updated.paper.current_revision_id,
        second.paper.current_revision_id
    );
    assert!(update_document(&f.store, edit(&f, &second.paper)).is_err());
    let other = add(&f, None, Some("slides"), "Another deck", "other");
    let mut request = edit(&f, &updated.paper);
    request.current_revision_id = other.paper.current_revision_id;
    request.operation_id = "wrong-document".into();
    assert!(update_document(&f.store, request).is_err());
    let mut request = edit(&f, &updated.paper);
    request.current_revision_id = first.paper.current_revision_id.clone();
    request.archived = true;
    request.operation_id = "archive-and-select".into();
    let archived = update_document(&f.store, request).unwrap();
    assert!(archived.paper.archived);
    assert_eq!(
        archived.paper.current_revision_id,
        first.paper.current_revision_id
    );
    assert_eq!(
        document_versions(&f.store, &f.workspace_id, &first.paper.id)
            .unwrap()
            .len(),
        2
    );
    assert!(document_versions(&f.store, "another_workspace", &first.paper.id).is_err());
}
#[test]
fn invalid_types_are_rejected_before_importing() {
    let f = fixture();
    let path = f.root.join("document.txt");
    fs::write(&path, "Draft").unwrap();
    let result = import_paper(
        &f.store,
        ImportPaperRequest {
            workspace_id: f.workspace_id.clone(),
            paper_id: None,
            title: "Draft".into(),
            role: "other".into(),
            document_type: Some("unknown".into()),
            path: path.to_string_lossy().into_owned(),
            operation_id: "bad-kind".into(),
        },
    );
    assert!(result.is_err());
    assert!(list_papers(&f.store, &f.workspace_id).unwrap().is_empty());
}

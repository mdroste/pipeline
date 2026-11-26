use super::*;

#[test]
fn paper_list_keeps_the_bounded_page_and_only_its_own_current_revisions() {
    let f = fixture();
    let mut connection = f.store.connection().unwrap();
    let tx = connection.transaction().unwrap();
    for index in 0..502 {
        let id = format!("paper_{index:04}");
        let revision = format!("revision_{index:04}");
        tx.execute("INSERT INTO papers(id,workspace_id,title,role,current_revision_id,created_at,updated_at,document_type,pinned,archived) VALUES(?1,?2,?1,'other',?3,'t',?1,'slides',1,1)", params![id,f.workspace_id,revision]).unwrap();
        tx.execute("INSERT INTO paper_revisions(id,paper_id,input_kind,entrypoint,dependency_manifest_json,content_hash,extraction_json,capture_complete,captured_at) VALUES(?1,?2,'text','draft.txt','{\"files\":[]}',?2,'{}',1,'t')",params![revision,id]).unwrap();
    }
    // Null, missing, and another paper's revision remain an absent revision.
    tx.execute(
        "UPDATE papers SET current_revision_id=NULL WHERE id='paper_0501'",
        [],
    )
    .unwrap();
    tx.execute(
        "UPDATE papers SET current_revision_id='missing' WHERE id='paper_0500'",
        [],
    )
    .unwrap();
    tx.execute(
        "UPDATE papers SET current_revision_id='revision_0498' WHERE id='paper_0499'",
        [],
    )
    .unwrap();
    tx.commit().unwrap();
    let listed = list_papers(&f.store, &f.workspace_id).unwrap();
    assert_eq!(listed.len(), 500);
    assert_eq!(listed[0].paper.id, "paper_0501");
    assert_eq!(listed[499].paper.id, "paper_0002");
    for item in &listed[..3] {
        assert!(item.revision.is_none());
    }
    for item in &listed[3..] {
        assert_eq!(item.revision.as_ref().unwrap().paper_id, item.paper.id);
        assert!(item.paper.archived && item.paper.pinned);
        assert_eq!(item.paper.document_type, "slides");
    }
    for item in &listed[..5] {
        let single =
            papers::paper_with_revision(&f.store, &f.workspace_id, &item.paper.id, None).unwrap();
        assert_eq!(
            serde_json::to_value(item).unwrap(),
            serde_json::to_value(single).unwrap()
        );
    }
    assert!(list_papers(&f.store, "other_workspace").unwrap().is_empty());
}

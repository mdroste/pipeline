//! Finding 4 (docs/bug-triage-2026-10.md): imported response_/zotero_ record
//! ids carry the workspace, and the unscoped ids written by builds up to 1.0.2
//! are still found in their own workspace.
use super::*;
fn second_workspace(f: &Fixture) -> String {
    let root = f._temp.path().join("other");
    fs::create_dir(&root).unwrap();
    f.store
        .create_workspace(crate::workbench::store::CreateWorkspaceRequest {
            name: "Other".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create-other".into(),
        })
        .unwrap()
        .record
        .id
}
fn zotero_preview() -> ZoteroPreview {
    ZoteroPreview {
        server_id: Some("server-1".into()),
        collection: "ABC123".into(),
        start: 0,
        has_more: false,
        items: vec![
            json!({"key":"ITEM1","version":7,"library":{"id":1,"type":"user"},"data":{"title":"Shared paper","DOI":"10.1/x"}}),
        ],
    }
}
fn package() -> FindingPackage {
    FindingPackage {
        version: 1,
        run_id: "run-1".into(),
        source_step_id: "final".into(),
        findings: vec![crate::models::Finding {
            id: "issue-1".into(),
            title: "Appendix".into(),
            body: "Missing derivation".into(),
            ..Default::default()
        }],
    }
}
#[test]
fn same_item_imported_into_two_workspaces_yields_independent_records() {
    let f = fixture();
    let other = second_workspace(&f);
    let a = import_zotero(&f.store, &f.ws, zotero_preview(), &["ITEM1".into()]).unwrap();
    let b = import_zotero(&f.store, &other, zotero_preview(), &["ITEM1".into()]).unwrap();
    let (a_id, b_id) = (a[0]["id"].as_str().unwrap(), b[0]["id"].as_str().unwrap());
    assert_ne!(a_id, b_id);
    assert!(a_id.starts_with("zotero_") && b_id.starts_with("zotero_"));
    assert_eq!(
        serde_json::to_value(record(&f.store, &f.ws, a_id, "bibliography").unwrap()).unwrap(),
        a[0]
    );
    assert!(record(&f.store, &f.ws, b_id, "bibliography").is_err());
    assert_eq!(records(&f.store, &f.ws, "bibliography").unwrap().len(), 1);
    assert_eq!(records(&f.store, &other, "bibliography").unwrap().len(), 1);
    assert_eq!(research::list_sources(&f.store, &other).unwrap().len(), 1);
    let ra = import_findings(&f.store, &f.ws, package(), &["issue-1".into()]).unwrap();
    let rb = import_findings(&f.store, &other, package(), &["issue-1".into()]).unwrap();
    assert_ne!(ra[0]["id"], rb[0]["id"]);
    assert_eq!(records(&f.store, &f.ws, "response").unwrap().len(), 1);
    assert_eq!(records(&f.store, &other, "response").unwrap().len(), 1);
    import_zotero(&f.store, &f.ws, zotero_preview(), &["ITEM1".into()]).unwrap();
    import_findings(&f.store, &f.ws, package(), &["issue-1".into()]).unwrap();
    assert_eq!(records(&f.store, &f.ws, "bibliography").unwrap().len(), 1);
    assert_eq!(records(&f.store, &f.ws, "response").unwrap().len(), 1);
}
#[test]
fn legacy_unscoped_ids_are_reused_instead_of_duplicated() {
    let f = fixture();
    let preview = zotero_preview();
    let item = &preview.items[0];
    let legacy_zotero = format!(
        "zotero_{}",
        hash(
            format!(
                "server-1\0{}\0ITEM1\07\0{}",
                item["library"],
                hash(&serde_json::to_vec(item).unwrap())
            )
            .as_bytes()
        )
    );
    put(
        &f.store,
        &f.ws,
        &legacy_zotero,
        "bibliography",
        0,
        &json!({"origin":"zotero_local_preview","note":"written by 1.0.2"}),
    )
    .unwrap();
    let legacy_response = format!("response_{}", hash(b"workflow\0run-1\0final\0issue-1"));
    put(
        &f.store,
        &f.ws,
        &legacy_response,
        "response",
        0,
        &ResponseRecord {
            source: json!({"kind":"workflow"}),
            decision: ResponseDecision {
                disposition: "deferred".into(),
                ..Default::default()
            },
            flags: vec![],
        },
    )
    .unwrap();
    let bib = import_zotero(&f.store, &f.ws, preview, &["ITEM1".into()]).unwrap();
    assert_eq!(bib[0]["id"], legacy_zotero);
    assert_eq!(bib[0]["body"]["note"], "written by 1.0.2");
    assert_eq!(records(&f.store, &f.ws, "bibliography").unwrap().len(), 1);
    assert!(research::list_sources(&f.store, &f.ws).unwrap().is_empty());
    let resp = import_findings(&f.store, &f.ws, package(), &["issue-1".into()]).unwrap();
    assert_eq!(resp[0]["id"], legacy_response);
    assert_eq!(resp[0]["revision"], 2);
    assert_eq!(resp[0]["body"]["decision"]["disposition"], "deferred");
    assert_eq!(resp[0]["body"]["source"]["findingId"], "issue-1");
    assert_eq!(records(&f.store, &f.ws, "response").unwrap().len(), 1);
}

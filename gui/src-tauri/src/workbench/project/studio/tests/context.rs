use super::*;

#[test]
fn source_context_preserves_exact_bytes_and_refuses_changed_files() {
    let f = fixture();
    fs::write(f.root.join("main.tex"), "α beta").unwrap();
    let request = SourceContextRequest {
        path: "main.tex".into(),
        checkpoint_id: None,
        expected_hash: Some(hash("α beta".as_bytes())),
        selection: Some(SourcePassage {
            quote: "α".into(),
            start: 0,
            end: 2,
            line: 1,
            page: None,
            draft: false,
        }),
    };
    let snapshot = capture_context(&f.store, &f.ws, request.clone(), "context-one").unwrap();
    assert_eq!(snapshot.body["text"], "α");
    fs::write(f.root.join("main.tex"), "changed").unwrap();
    assert!(capture_context(&f.store, &f.ws, request, "context-two").is_err());
    let kept = crate::workbench::desk::record(&f.store, &f.ws, &snapshot.id).unwrap();
    assert_eq!(kept.body["text"], "α");
}

#[test]
fn unsaved_context_is_a_labeled_snapshot_without_writing_the_file() {
    let f = fixture();
    fs::write(f.root.join("main.tex"), "saved").unwrap();
    let snapshot = capture_context(
        &f.store,
        &f.ws,
        SourceContextRequest {
            path: "main.tex".into(),
            checkpoint_id: None,
            expected_hash: Some(hash(b"saved")),
            selection: Some(SourcePassage {
                quote: "unsaved edit".into(),
                start: 0,
                end: 12,
                line: 1,
                page: None,
                draft: true,
            }),
        },
        "draft-context",
    )
    .unwrap();
    assert_eq!(snapshot.body["basis"], "unsaved researcher selection");
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "saved"
    );
}

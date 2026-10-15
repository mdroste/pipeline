//! Finding 13 (docs/bug-triage-2026-10.md): paths that task capture ignores
//! cannot be edited or checkpointed, so acceptance never unlinks them.
use super::*;

#[test]
fn ignored_paths_cannot_be_edited_or_captured_into_a_deletion() {
    let f = fixture();
    fs::create_dir_all(f.root.join("target")).unwrap();
    fs::write(f.root.join("target/notes.md"), "orig").unwrap();
    let expected = match editor_file(&f.store, &f.ws, None, "target/notes.md") {
        Ok(opened) => opened.hash,
        Err(_) => hash(b"orig"),
    };
    assert!(save_text(
        &f.store,
        &f.ws,
        None,
        "target/notes.md",
        &expected,
        "edited"
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(f.root.join("target/notes.md")).unwrap(),
        "orig"
    );
    let task = tasks::create_task(
        &f.store,
        &f.ws,
        "Edit target/notes.md".into(),
        None,
        vec!["target/notes.md".into()],
        vec![],
    )
    .unwrap();
    assert!(tasks::checkpoint(
        &f.store,
        &f.ws,
        &task.id,
        vec!["target/notes.md".into()],
        "copy"
    )
    .is_err());
}

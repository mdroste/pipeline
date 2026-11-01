//! What a paper import leaves selected, stored, and reported.
use super::*;

fn import(
    fixture: &Fixture,
    paper_id: Option<String>,
    title: &str,
    path: &Path,
    operation: &str,
) -> PaperWithRevision {
    import_paper(
        &fixture.store,
        ImportPaperRequest {
            workspace_id: fixture.workspace_id.clone(),
            paper_id,
            title: title.into(),
            role: "manuscript".into(),
            path: path.to_string_lossy().into_owned(),
            operation_id: operation.into(),
        },
    )
    .unwrap()
}

#[test]
fn importing_an_earlier_revision_again_selects_it_and_stales_later_evidence() {
    let fixture = fixture();
    let path = fixture.root.join("draft.tex");
    fs::write(&path, "Draft A: output rises.\n").unwrap();
    let a = import(&fixture, None, "Draft", &path, "draft-a");
    let paper = a.paper.id.clone();
    let revision_a = a.revision.unwrap().id;
    fs::write(&path, "Draft B: output falls.\n").unwrap();
    let b = import(&fixture, Some(paper.clone()), "Draft", &path, "draft-b");
    let revision_b = b.revision.unwrap().id;
    let claim = propose_claim(
        &fixture.store,
        ProposeClaimRequest {
            workspace_id: fixture.workspace_id.clone(),
            paper_id: Some(paper.clone()),
            claim: "Output falls.".into(),
            kind: "mechanism".into(),
            origin: "user".into(),
            operation_id: "draft-claim".into(),
        },
        false,
    )
    .unwrap();
    propose_evidence(
        &fixture.store,
        ProposeEvidenceRequest {
            workspace_id: fixture.workspace_id.clone(),
            claim_version_id: claim.version_id,
            target_type: "paper_revision".into(),
            target_id: revision_b.clone(),
            locator: None,
            relation: "supports".into(),
            assessor: "model:test".into(),
            operation_id: "draft-evidence".into(),
        },
        true,
    )
    .unwrap();
    let ledger = research_ledger(&fixture.store, &fixture.workspace_id).unwrap();
    assert_eq!(ledger.evidence[0].freshness, "current");

    fs::write(&path, "Draft A: output rises.\n").unwrap();
    let again = import(
        &fixture,
        Some(paper.clone()),
        "Draft, corrected",
        &path,
        "draft-a2",
    );
    assert_eq!(again.revision.unwrap().id, revision_a);
    assert_eq!(
        again.paper.current_revision_id.as_deref(),
        Some(revision_a.as_str())
    );
    assert_eq!(again.paper.title, "Draft, corrected");
    let papers = list_papers(&fixture.store, &fixture.workspace_id).unwrap();
    assert_eq!(
        papers[0].revision.as_ref().unwrap().id,
        revision_a,
        "later turns read the selected revision"
    );
    let ledger = research_ledger(&fixture.store, &fixture.workspace_id).unwrap();
    assert_eq!(ledger.evidence[0].freshness, "stale");
    assert_eq!(
        ledger.evidence[0].stale_reason.as_deref(),
        Some("Another paper revision was selected")
    );

    // A retried request changes nothing, even after another revision was chosen.
    fs::write(&path, "Draft B: output falls.\n").unwrap();
    let chosen = import(&fixture, Some(paper.clone()), "Draft B", &path, "draft-b2");
    fs::write(&path, "Draft A: output rises.\n").unwrap();
    let retried = import(&fixture, Some(paper), "Draft, corrected", &path, "draft-a2");
    assert_eq!(
        retried.paper.current_revision_id,
        chosen.paper.current_revision_id
    );
    assert_eq!(retried.paper.title, "Draft B");
}

#[test]
fn folder_import_reads_latin1_and_reports_what_it_leaves_out() {
    let fixture = fixture();
    let tree = fixture.root.join("old-paper");
    fs::create_dir(&tree).unwrap();
    fs::write(tree.join("a-main.tex"), "\\input{b-section}\n").unwrap();
    fs::write(
        tree.join("b-section.tex"),
        b"Caf\xe9 prices rose by 2.5%.\n",
    )
    .unwrap();
    fs::write(tree.join("c-binary.txt"), b"\xff\xfe\x00t\x00").unwrap();
    let paper = import(&fixture, None, "Old", &tree, "old-tree");
    let revision = paper.revision.unwrap();
    assert_eq!(revision.extraction["status"], "partial");
    let notes = revision.extraction["qualityNotes"].as_array().unwrap();
    assert_eq!(
        notes[1],
        "Read as Latin-1 because they are not UTF-8: b-section.tex."
    );
    assert_eq!(notes[2], "Left out of this text: c-binary.txt (not text).");
    let hit = paper_search(
        &fixture.store,
        &fixture.workspace_id,
        &revision.id,
        "Café prices",
        5,
    )
    .unwrap();
    assert_eq!(hit.len(), 1);
    assert!(hit[0].excerpt.contains("Café prices rose by 2.5%."));

    let clean = fixture.root.join("clean");
    fs::create_dir(&clean).unwrap();
    fs::write(clean.join("main.tex"), "Plain text.\n").unwrap();
    let revision = import(&fixture, None, "Clean", &clean, "clean-tree")
        .revision
        .unwrap();
    assert_eq!(revision.extraction["status"], "complete");
    assert_eq!(
        revision.extraction["qualityNotes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn folder_import_past_the_text_limit_names_the_files_it_stopped_at() {
    let fixture = fixture();
    let tree = fixture.root.join("large-paper");
    fs::create_dir(&tree).unwrap();
    fs::write(tree.join("a.tex"), "x".repeat(MAX_TEXT_BYTES - 1000)).unwrap();
    fs::write(tree.join("b.tex"), "y".repeat(2000)).unwrap();
    fs::write(tree.join("c.tex"), "Short closing section.").unwrap();
    let revision = import(&fixture, None, "Large", &tree, "large-tree")
        .revision
        .unwrap();
    assert_eq!(revision.extraction["status"], "partial");
    assert_eq!(
        revision.extraction["qualityNotes"][1],
        "Left out of this text: b.tex (over the 16 MiB text limit)."
    );
    let hit = paper_search(
        &fixture.store,
        &fixture.workspace_id,
        &revision.id,
        "closing section",
        5,
    )
    .unwrap();
    assert_eq!(hit.len(), 1, "a smaller later file still fits");
}

#[test]
fn a_truncated_text_blob_is_repaired_by_the_next_import() {
    let fixture = fixture();
    let path = fixture.root.join("paper.tex");
    let full = "Output rises with productivity in every sector.\n";
    fs::write(&path, full).unwrap();
    let blob = fixture
        .store
        .root_path()
        .join("blobs")
        .join(format!("{}.txt", hash_bytes(full.as_bytes())));
    fs::write(&blob, &full.as_bytes()[..11]).unwrap();
    let revision = import(&fixture, None, "Paper", &path, "repair")
        .revision
        .unwrap();
    assert_eq!(fs::read_to_string(&blob).unwrap(), full);
    let hit = paper_search(
        &fixture.store,
        &fixture.workspace_id,
        &revision.id,
        "every sector",
        5,
    )
    .unwrap();
    assert_eq!(hit.len(), 1);
}

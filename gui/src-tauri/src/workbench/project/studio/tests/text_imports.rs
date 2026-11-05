//! Previews and imports that read a document's words: a missing or unreadable text
//! is an error that says why, blank BibTeX fields import, line endings and the
//! comment limit don't change what a report split shows, and lookups see every
//! entry however long the bibliography is.
use super::*;

fn bytes_paper(f: &Fixture, name: &str, bytes: &[u8]) -> research::PaperWithRevision {
    fs::write(f.root.join(name), bytes).unwrap();
    research::import_paper(
        &f.store,
        research::ImportPaperRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            title: name.into(),
            role: "other".into(),
            path: f.root.join(name).to_string_lossy().into_owned(),
            operation_id: id("paper").unwrap(),
        },
    )
    .unwrap()
}
fn revision_of(p: research::PaperWithRevision) -> String {
    p.revision.unwrap().id
}
/// A bibliography's selected entries, imported the way the preview offers them.
fn import_all(f: &Fixture, revision: &str) -> WorkbenchResult<Value> {
    let text = exact_text(&f.store, &f.ws, revision).unwrap();
    let ids = parse_bibtex(&text)
        .unwrap()
        .into_iter()
        .map(|e| e.id)
        .collect::<Vec<_>>();
    import_bibliography(&f.store, &f.ws, revision, &ids)
}

#[test]
fn a_latin1_bibliography_is_read_and_says_how() {
    let f = fixture();
    let p = bytes_paper(
        &f,
        "refs.bib",
        b"@article{gr\xfcn, title={Gr\xfcnfeld investment}}\n",
    );
    let revision = p.revision.unwrap();
    assert_eq!(revision.extraction["method"], "latin1");
    let preview = preview_bibliography(&f.store, &f.ws, &revision.id).unwrap();
    assert_eq!(preview["entries"][0]["key"], "grün");
    assert_eq!(
        preview["entries"][0]["fields"]["title"],
        "Grünfeld investment"
    );
}

#[test]
fn a_file_that_is_not_text_says_so_where_the_preview_was_silent() {
    let f = fixture();
    let revision = revision_of(bytes_paper(&f, "binary.bib", b"\x00\xff\x00\xfe"));
    for error in [
        preview_bibliography(&f.store, &f.ws, &revision).unwrap_err(),
        preview_report(&f.store, &f.ws, &revision).unwrap_err(),
        citation_navigation(&f.store, &f.ws, &revision).unwrap_err(),
    ] {
        assert!(
            error.message.contains("not text in a known encoding"),
            "{}",
            error.message
        );
    }
}

#[test]
fn a_document_with_only_blank_text_is_refused_by_name() {
    let f = fixture();
    // What pdftotext writes for a scanned PDF with no text layer.
    let revision = revision_of(bytes_paper(&f, "scan.txt", b"\x0c\x0c\n\x0c"));
    let error = preview_report(&f.store, &f.ws, &revision).unwrap_err();
    assert!(error.message.contains("no readable text"), "{error:?}");
}

#[test]
fn blank_bibtex_fields_count_as_absent() {
    let f = fixture();
    let revision = revision_of(paper(
        &f,
        "refs.bib",
        "@article{full, title={Whole}, year={2020}}\n@article{blank, title={ }, url={}, doi={}, year={}}\n",
    ));
    let imported = import_all(&f, &revision).unwrap();
    assert_eq!(imported.as_array().unwrap().len(), 2);
    let sources = research::list_sources(&f.store, &f.ws).unwrap();
    let blank = sources
        .iter()
        .find(|s| s.citation_key.as_deref() == Some("blank"))
        .unwrap();
    assert_eq!(blank.title, "blank");
    assert!(blank.identifiers.get("doi").is_none());
}

#[test]
fn a_refused_entry_is_named_and_the_retry_keeps_what_came_before() {
    let f = fixture();
    let long = "x".repeat(1100);
    let revision = revision_of(paper(
        &f,
        "refs.bib",
        &format!("@article{{first, title={{One}}}}\n@article{{toolong, title={{{long}}}}}\n"),
    ));
    let error = import_all(&f, &revision).unwrap_err();
    assert!(error.message.starts_with("Citation toolong: "), "{error:?}");
    assert!(error.message.contains("importing again keeps"), "{error:?}");
    assert_eq!(records(&f.store, &f.ws, "bibliography").unwrap().len(), 1);
}

#[test]
fn a_windows_report_splits_into_comments_with_exact_locators() {
    let f = fixture();
    let revision = revision_of(paper(
        &f,
        "report.txt",
        "First comment.\r\nStill first.\r\n\r\nSecond comment.\r\n\r\nThird.\r\n",
    ));
    let preview = preview_report(&f.store, &f.ws, &revision).unwrap();
    let texts = preview
        .comments
        .iter()
        .map(|c| c.text.trim_end().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        texts,
        [
            "First comment.\r\nStill first.",
            "Second comment.",
            "Third."
        ]
    );
    assert_eq!(preview.paragraphs, 3);
    let imported = import_report(&f.store, &f.ws, &revision, preview.comments).unwrap();
    assert_eq!(imported.as_array().unwrap().len(), 3);
}

#[test]
fn unix_reports_keep_the_locators_they_were_imported_with() {
    let f = fixture();
    let revision = revision_of(paper(&f, "report.md", "One.\n\n\nTwo.\n\nThree."));
    let preview = preview_report(&f.store, &f.ws, &revision).unwrap();
    let spans = preview
        .comments
        .iter()
        .map(|c| (c.start, c.end))
        .collect::<Vec<_>>();
    assert_eq!(spans, [(0, 4), (6, 11), (13, 19)]);
}

#[test]
fn a_report_past_the_comment_limit_says_how_many_it_holds() {
    let f = fixture();
    let text = (1..=503)
        .map(|n| format!("Comment {n}."))
        .collect::<Vec<_>>()
        .join("\n\n");
    let revision = revision_of(paper(&f, "report.md", &text));
    let preview = preview_report(&f.store, &f.ws, &revision).unwrap();
    assert_eq!(preview.comments.len(), 500);
    assert_eq!(preview.paragraphs, 503);
    assert_eq!(preview.comments[499].text, "Comment 500.");
}

#[test]
fn lookups_see_entries_past_the_newest_list() {
    let f = fixture();
    let text = (0..520)
        .map(|n| format!("@article{{key{n}, title={{Paper {n}}}}}\n"))
        .collect::<String>();
    let bib = revision_of(paper(&f, "refs.bib", &text));
    import_all(&f, &bib).unwrap();
    let conn = f.store.connection().unwrap();
    // The oldest entry by the list's order, whichever one that was.
    let oldest: String = conn
        .query_row(
            "SELECT json_extract(body_json,'$.entry.key') FROM project_records WHERE kind='bibliography' ORDER BY updated_at,id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);
    assert_eq!(records(&f.store, &f.ws, "bibliography").unwrap().len(), 500);
    assert!(!records(&f.store, &f.ws, "bibliography")
        .unwrap()
        .iter()
        .any(|r| r.body["entry"]["key"] == oldest.as_str()));

    let preview = preview_bibliography(&f.store, &f.ws, &bib).unwrap();
    assert_eq!(preview["existingKeys"].as_array().unwrap().len(), 520);

    let manuscript = revision_of(paper(
        &f,
        "main.tex",
        &format!("As shown \\cite{{{oldest}, nobody}}."),
    ));
    let cited = citation_navigation(&f.store, &f.ws, &manuscript).unwrap();
    assert_eq!(
        cited["citations"][0]["sources"].as_array().unwrap().len(),
        1
    );
    assert_eq!(cited["citations"][1]["sources"], json!([]));
}

#[test]
fn impact_says_it_is_incomplete_when_a_read_limit_left_links_unread() {
    let f = fixture();
    let insert = |from: usize, to: usize| {
        let c = f.store.connection().unwrap();
        for n in from..to {
            c.execute(
                "INSERT INTO project_records VALUES(?1,?2,'experiment',1,'{}','2026-09-07')",
                params![format!("experiment_{n}"), f.ws],
            )
            .unwrap();
        }
    };
    insert(0, 999);
    let impact = crate::workbench::project::relations::impact;
    assert!(impact(&f.store, &f.ws).unwrap().complete);
    insert(999, 1000);
    assert!(!impact(&f.store, &f.ws).unwrap().complete);
}

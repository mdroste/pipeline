use super::*;
use std::collections::BTreeMap;

const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx(paragraphs: &[&str]) -> Vec<u8> {
    let body: String = paragraphs
        .iter()
        .map(|text| format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"))
        .collect();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, content) in [
        ("[Content_Types].xml", r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_string()),
        ("_rels/.rels", r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_string()),
        ("word/_rels/document.xml.rels", r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#.to_string()),
        ("word/document.xml", format!(r#"<?xml version="1.0"?><w:document xmlns:w="{NS}"><w:body>{body}</w:body></w:document>"#)),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn pdf(line: &str) -> Vec<u8> {
    let stream = format!("BT /F1 12 Tf 72 720 Td ({line}) Tj ET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut pdf = "%PDF-1.4\n".to_string();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = pdf.len();
    pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    ));
    pdf.into_bytes()
}

struct Fixture {
    _root: tempfile::TempDir,
    run_dir: PathBuf,
    original: PathBuf,
    manifest: RunManifest,
}

fn fixture(name: &str, bytes: &[u8], paper_hash: &str, quote: &str) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("manuscript").join(name);
    std::fs::create_dir_all(original.parent().unwrap()).unwrap();
    std::fs::write(&original, bytes).unwrap();
    let run_dir = root.path().join("run");
    std::fs::create_dir_all(run_dir.join("context")).unwrap();
    let report = serde_json::json!({
        "report_date": "2026-10-28",
        "paper_hash": paper_hash,
        "quality": {"schema_version": 1, "status": "done"},
        "products": {"schema_version": 1, "findings": {"schema_version": 1, "findings": [
            {"id": "a", "title": "Anchored finding", "category": "Correctness", "priority": "high",
             "body": "**Problem.** The estimate is biased.",
             "evidence": [{"page": 1, "quote": quote, "verification_status": "verified"}]},
            {"id": "b", "title": "Unanchored finding", "category": "Exposition", "priority": "low",
             "body": "Clarify notation.", "evidence": [{"description": "Section 2"}]}
        ]}}
    });
    std::fs::write(run_dir.join("report.json"), report.to_string()).unwrap();
    std::fs::write(run_dir.join("report.md"), "# Referee Report\n\nBody").unwrap();
    std::fs::write(
        run_dir.join("context/document_bundle.json"),
        serde_json::json!({"extraction": {"source_path": original, "paper_hash": paper_hash}})
            .to_string(),
    )
    .unwrap();
    let manifest: RunManifest = serde_json::from_value(serde_json::json!({
        "run_id": "abc_1", "created": "2026-10-28", "input_path": original,
        "input_mode": "document", "profile_id": "paper", "profile_name": "Paper Review",
        "provider": "claude", "artifacts": [], "status": "done"
    }))
    .unwrap();
    Fixture {
        _root: root,
        run_dir,
        original,
        manifest,
    }
}

fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(
                std::fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else {
            files.insert(path.clone(), std::fs::read(&path).unwrap());
        }
    }
    files
}

const PARAGRAPHS: &[&str] = &[
    "Introduction",
    "We find that the estimate is biased upward in small samples.",
];
const QUOTE: &str = "the estimate is biased upward";

fn docx_fixture() -> Fixture {
    let bytes = docx(PARAGRAPHS);
    let hash = short_hash(docx_text(&bytes).unwrap().as_bytes());
    fixture("draft.docx", &bytes, &hash, QUOTE)
}

#[test]
fn every_format_reads_the_run_without_modifying_it_or_the_manuscript() {
    let fixture = docx_fixture();
    let before = snapshot(&fixture.run_dir);
    let original_before = std::fs::read(&fixture.original).unwrap();
    let run = load_run(&fixture.run_dir, fixture.manifest.clone()).unwrap();
    assert_eq!(run.export.meta.title, "draft.docx");
    assert_eq!(run.export.meta.workflow, "Paper Review");
    assert_eq!(run.export.meta.status, "Completed");

    let html = generate(&run, ReportFormat::Html).unwrap();
    assert_eq!(html.suggested_name, "draft review.html");
    assert!(String::from_utf8(html.bytes)
        .unwrap()
        .contains("Anchored finding"));
    let word = generate(&run, ReportFormat::Docx).unwrap();
    assert!(docx_text(&word.bytes)
        .unwrap()
        .contains("Unanchored finding"));
    let commented = generate(&run, ReportFormat::DocxComments).unwrap();
    assert_eq!((commented.anchored, commented.unanchored), (1, 1));
    assert_eq!(commented.suggested_name, "draft with review comments.docx");
    assert_eq!(commented.protected, vec![fixture.original.clone()]);
    assert_eq!(
        docx_text(&commented.bytes).unwrap(),
        docx_text(&original_before).unwrap()
    );

    assert_eq!(snapshot(&fixture.run_dir), before);
    assert_eq!(std::fs::read(&fixture.original).unwrap(), original_before);
}

#[test]
fn annotation_requires_the_reviewed_version_and_matching_input_type() {
    let fixture = docx_fixture();
    let run = load_run(&fixture.run_dir, fixture.manifest.clone()).unwrap();
    let error = generate(&run, ReportFormat::AnnotatedPdf).err().unwrap();
    assert!(error.contains("was not a PDF"), "{error}");

    std::fs::write(
        &fixture.original,
        docx(&["A revised draft with other words."]),
    )
    .unwrap();
    let error = generate(&run, ReportFormat::DocxComments).err().unwrap();
    assert!(
        error.contains("has changed since this Review ran"),
        "{error}"
    );

    // A copy captured inside the run takes precedence over the moved original.
    std::fs::create_dir_all(fixture.run_dir.join("source-snapshot")).unwrap();
    std::fs::write(
        fixture.run_dir.join("source-snapshot/draft.docx"),
        docx(PARAGRAPHS),
    )
    .unwrap();
    std::fs::remove_file(&fixture.original).unwrap();
    let commented = generate(&run, ReportFormat::DocxComments).unwrap();
    assert_eq!(commented.anchored, 1);
}

#[test]
fn running_reviews_are_not_exported() {
    let fixture = docx_fixture();
    let mut manifest = fixture.manifest.clone();
    manifest.status = "running".to_string();
    assert!(load_run(&fixture.run_dir, manifest)
        .err()
        .unwrap()
        .contains("Wait for this Review to finish"));
}

#[test]
fn annotated_pdf_uses_the_verified_original() {
    if crate::deps::resolve_command("pdftotext").is_none() {
        eprintln!("skipping: pdftotext is not installed");
        return;
    }
    let bytes = pdf("We find that the estimate is biased upward in small samples.");
    let fixture = fixture("paper.pdf", &bytes, &short_hash(&bytes), QUOTE);
    let before = snapshot(&fixture.run_dir);
    let run = load_run(&fixture.run_dir, fixture.manifest.clone()).unwrap();
    let annotated = generate(&run, ReportFormat::AnnotatedPdf).unwrap();
    assert_eq!((annotated.anchored, annotated.unanchored), (1, 1));
    assert!(annotated.bytes.starts_with(&bytes));
    assert_eq!(annotated.suggested_name, "paper with review highlights.pdf");
    assert_eq!(snapshot(&fixture.run_dir), before);
    assert_eq!(std::fs::read(&fixture.original).unwrap(), bytes);

    std::fs::write(&fixture.original, pdf("Different text")).unwrap();
    assert!(generate(&run, ReportFormat::AnnotatedPdf)
        .err()
        .unwrap()
        .contains("has changed"));
}

#[test]
fn saving_never_replaces_the_reviewed_manuscript() {
    let fixture = docx_fixture();
    let protected = vec![fixture.original.clone()];
    let error = check_destination(&fixture.original, &protected).unwrap_err();
    assert!(error.contains("never replaces the reviewed manuscript"));
    let sibling = fixture
        .original
        .with_file_name("draft with review comments.docx");
    let target = check_destination(&sibling, &protected).unwrap();
    write_atomically(&target, b"copy").unwrap();
    assert_eq!(std::fs::read(&sibling).unwrap(), b"copy");
    #[cfg(unix)]
    {
        let link = fixture.original.with_file_name("link.docx");
        std::os::unix::fs::symlink(&fixture.original, &link).unwrap();
        assert!(check_destination(&link, &protected).is_err());
    }
}

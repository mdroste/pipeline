//! Bibliography discovery through the path a run takes: stage the selected
//! source, extract it, build the bundle, then collect.
use super::*;
use crate::document_bundle::{self, DocumentNode, DocumentProvenance};
use crate::models::ExtractionResult;
use serde_json::json;

const UNDEFINED: &str = "references-undefined-citations";

struct Prepared {
    _private: tempfile::TempDir,
    source: String,
    extraction: ExtractionResult,
    bundle: DocumentBundle,
}

impl Prepared {
    fn collect(&self) -> (Vec<Reference>, LocalChecks) {
        collect(Some(&self.bundle), &self.extraction.text, &self.source)
    }

    async fn run(&self) -> Value {
        run(
            Some(&self.bundle),
            &self.extraction.text,
            &self.source,
            &json!({}),
        )
        .await
        .unwrap()
    }
}

async fn prepare(selected: &Path) -> Prepared {
    let private = tempfile::tempdir().unwrap();
    let staged =
        crate::pipeline::extract::stage_selected_source(selected, "document", private.path())
            .unwrap();
    let source = staged.source_path.unwrap().to_string_lossy().into_owned();
    let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let extraction = crate::pipeline::extract::extract(
        &app,
        &source,
        &crate::pipeline_config::ExtractionConfig::default(),
    )
    .await
    .unwrap();
    let run_dir = private.path().join("run");
    std::fs::create_dir_all(&run_dir).unwrap();
    let bundle = document_bundle::build(&extraction, &run_dir)
        .unwrap()
        .bundle;
    Prepared {
        _private: private,
        source,
        extraction,
        bundle,
    }
}

fn project(files: &[(&str, &[u8])]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = directory.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    directory
}

fn tex(body: &str) -> Vec<u8> {
    format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n")
        .into_bytes()
}

fn finding_ids(output: &Value) -> Vec<String> {
    output["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| finding["id"].as_str().unwrap().to_string())
        .collect()
}

fn word_paragraph(style: Option<&str>, text: &str) -> String {
    let style = style
        .map(|style| format!(r#"<w:pPr><w:pStyle w:val="{style}"/></w:pPr>"#))
        .unwrap_or_default();
    format!(r#"<w:p w:rsidR="1">{style}<w:r><w:t>{text}</w:t></w:r></w:p>"#)
}

fn word_file(directory: &Path, body: &str, footnotes: Option<&str>) -> PathBuf {
    use std::io::Write as _;
    const WORD: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let path = directory.join("paper.docx");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    archive.start_file("word/document.xml", options).unwrap();
    write!(
        archive,
        r#"<w:document xmlns:w="{WORD}"><w:body>{body}</w:body></w:document>"#
    )
    .unwrap();
    if let Some(notes) = footnotes {
        archive.start_file("word/footnotes.xml", options).unwrap();
        write!(
            archive,
            r#"<w:footnotes xmlns:w="{WORD}"><w:footnote w:id="2">{notes}</w:footnote></w:footnotes>"#
        )
        .unwrap();
    }
    archive.finish().unwrap();
    path
}

const ENTRIES: [&str; 3] = [
    "Card, David. 1994. Minimum wages and employment. American Economic Review.",
    "Neumark, David. 2000. A comment on minimum wages. American Economic Review.",
    "Autor, David. 2013. The China syndrome. American Economic Review.",
];

#[tokio::test]
async fn a_word_document_with_real_tables_and_notes_yields_its_references() {
    let directory = tempfile::tempdir().unwrap();
    let body = [
        word_paragraph(Some("Heading1"), "Introduction"),
        word_paragraph(None, "We study minimum wages."),
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Variable</w:t></w:r></w:p></w:tc></w:tr></w:tbl>".into(),
        word_paragraph(Some("Heading1"), "References"),
        word_paragraph(None, ENTRIES[0]),
        word_paragraph(None, ENTRIES[1]),
        word_paragraph(None, ENTRIES[2]),
    ]
    .concat();
    let note = word_paragraph(None, "An acknowledgment that is plainly not a reference.");
    let prepared = prepare(&word_file(directory.path(), &body, Some(&note))).await;
    assert_eq!(prepared.bundle.source_kind, "docx");

    let (references, checks) = prepared.collect();
    let texts: Vec<&str> = references.iter().map(|item| item.text.as_str()).collect();
    assert_eq!(texts, ENTRIES);
    assert_eq!(references[0].first_author.as_deref(), Some("Card"));
    assert!(checks.limitations.is_empty(), "{:?}", checks.limitations);
    let output = prepared.run().await;
    assert_eq!(output["reference_count"], 3);
    assert!(finding_ids(&output).is_empty());
}

#[tokio::test]
async fn page_marked_markdown_keeps_entries_on_the_heading_page() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("paper.md");
    std::fs::write(
        &path,
        format!(
            "<!-- PAGE 1 -->\n\n# Minimum wages\n\nWe study wages.\n\n\
             <!-- PAGE 2 -->\n\n## References\n\n{}\n\n{}\n\n\
             <!-- PAGE 3 -->\n{}\n",
            ENTRIES[0], ENTRIES[1], ENTRIES[2]
        ),
    )
    .unwrap();
    let prepared = prepare(&path).await;
    assert!(prepared
        .bundle
        .nodes
        .iter()
        .any(|node| node.kind == "section" && node.text == "References"));

    let (references, _) = prepared.collect();
    let texts: Vec<&str> = references.iter().map(|item| item.text.as_str()).collect();
    assert_eq!(texts, ENTRIES, "page markers stay out of the entries");
}

#[test]
fn pdftotext_output_with_one_text_node_yields_its_references() {
    let run_dir = tempfile::tempdir().unwrap();
    let extraction = ExtractionResult {
        text: format!(
            "Body of the paper.\n\n\u{c}REFERENCES\n{}\n{}\n{}\n",
            ENTRIES[0], ENTRIES[1], ENTRIES[2]
        ),
        method: "pdftotext".into(),
        source_path: "paper.pdf".into(),
        paper_hash: "0123456789abcdef".into(),
        quality_notes: Vec::new(),
    };
    let bundle = document_bundle::build(&extraction, run_dir.path())
        .unwrap()
        .bundle;
    let (references, _) = collect(Some(&bundle), &extraction.text, "paper.pdf");
    assert_eq!(references.len(), 3);
}

#[tokio::test]
async fn a_document_with_no_bibliography_says_so() {
    let directory = tempfile::tempdir().unwrap();
    let body = word_paragraph(None, "A short note with no references at all.");
    let prepared = prepare(&word_file(directory.path(), &body, None)).await;
    let output = prepared.run().await;
    assert_eq!(output["reference_count"], 0);
    assert_eq!(finding_ids(&output), ["references-limitation-1"]);
    let finding = &output["findings"][0];
    assert_eq!(finding["severity"], "info");
    assert!(finding["problem"]
        .as_str()
        .unwrap()
        .starts_with("No bibliography was found"));
    let parsed = crate::findings::parse_value(&output, "refs", "References").unwrap();
    assert_eq!(parsed.findings.len(), 1);
    assert_eq!(parsed.findings[0].priority, "low");
}

fn paragraph_node(bundle: &DocumentBundle, kind: &str, text: &str) -> DocumentNode {
    let order = bundle.nodes.len() as u32 + 1;
    DocumentNode {
        id: format!("{kind}-{order}"),
        kind: kind.into(),
        order,
        page: Some(1),
        parent_id: None,
        label: None,
        number: None,
        text: text.into(),
        asset_ids: Vec::new(),
        representations: Vec::new(),
        provenance: DocumentProvenance {
            origin_id: "origin-primary".into(),
            method: "paddleocr-vl-full".into(),
            confidence: None,
        },
    }
}

#[test]
fn paragraph_nodes_are_used_when_they_hold_the_bibliography() {
    let run_dir = tempfile::tempdir().unwrap();
    let extraction = ExtractionResult {
        text: String::new(),
        method: "paddleocr-vl-full".into(),
        source_path: "paper.pdf".into(),
        paper_hash: "0123456789abcdef".into(),
        quality_notes: Vec::new(),
    };
    let mut bundle = document_bundle::build(&extraction, run_dir.path())
        .unwrap()
        .bundle;
    for (kind, text) in [
        ("section", "References"),
        ("text_block", ENTRIES[0]),
        ("text_block", ENTRIES[1]),
        ("text_block", ENTRIES[2]),
    ] {
        let node = paragraph_node(&bundle, kind, text);
        bundle.nodes.push(node);
    }
    let (references, _) = collect(Some(&bundle), "", "paper.pdf");
    assert_eq!(references.len(), 3);
    assert_eq!(references[0].node_id, "text_block-2");

    // Paragraph nodes without a bibliography do not hide the extracted text.
    bundle.nodes.retain(|node| node.kind != "section");
    let text = format!("Body.\n\n# References\n\n{}", ENTRIES.join("\n\n"));
    let (references, _) = collect(Some(&bundle), &text, "paper.pdf");
    assert_eq!(references.len(), 3);
    assert_eq!(references[0].node_id, "");
}

#[test]
fn headings_keep_their_meaning_under_markdown_markers() {
    for heading in [
        "# References",
        "## 7. References",
        "**References**",
        "__Bibliography:__",
        "### VII. REFERENCES ###",
    ] {
        let text = format!("{heading}\n\n{}", ENTRIES.join("\n\n"));
        let entries = bibliography::entries(&bibliography::text_blocks(&text));
        assert_eq!(entries.len(), 3, "{heading}");
    }
}

#[test]
fn bibitem_keys_read_the_forms_bibliography_styles_write() {
    let keys = bibtex::bibitem_keys(concat!(
        "\\begin{thebibliography}{9}\n",
        "\\bibitem{plain}\n",
        "\\bibitem[Card and Krueger(1994)]{natbib}\n",
        "\\bibitem[{Acemoglu et~al.(2001)Acemoglu, Johnson, and\n  Robinson}]{wrapped}\n",
        "\\bibitem[{[1]}]{bracketed}\n",
        "\\bibitem[Long]{%\n  split}\n",
        "% \\bibitem{commented}\n",
        "\\setlength{\\bibitemsep}{2pt}\n",
        "\\end{thebibliography}\n",
    ));
    assert_eq!(keys, ["plain", "natbib", "wrapped", "bracketed", "split"]);
    let biber = "\\datalist[entry]{nty/global//global/global}\n  \\entry{card1994}{article}{}\n    \\name{author}{1}{}{}\n  \\endentry\n";
    assert_eq!(bibtex::compiled_keys(biber), ["card1994"]);
}

#[tokio::test]
async fn an_embedded_bibliography_defines_its_keys() {
    let source = project(&[(
        "paper.tex",
        &tex(concat!(
            "As \\citet{card} and \\citep{neumark} show, and \\cite{forgotten} too.\n",
            "\\begin{thebibliography}{2}\n",
            "\\bibitem[Card(1994)]{card} D. Card. Minimum wages.\n",
            "\\bibitem{neumark} D. Neumark. A comment.\n",
            "\\end{thebibliography}",
        )),
    )]);
    let prepared = prepare(&source.path().join("paper.tex")).await;
    let (_, checks) = prepared.collect();
    assert_eq!(checks.compiled_entries, 2);
    assert_eq!(checks.undefined_citations, ["forgotten"]);
    let output = prepared.run().await;
    let finding = output["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["id"] == UNDEFINED)
        .unwrap();
    assert_eq!(
        finding["title"],
        "1 citation keys have no bibliography entry"
    );
    let limits = output["local_checks"]["limitations"].to_string();
    assert!(limits.contains("used only to confirm"), "{limits}");
}

#[tokio::test]
async fn labels_inside_an_embedded_bibliography_are_not_citations() {
    let source = project(&[(
        "paper.tex",
        &tex(concat!(
            "As \\cite{card} shows.\n",
            "\\begin{thebibliography}{1}\n",
            "\\bibitem[\\protect\\citeauthoryear{Card}{1994}]{card} D. Card.\n",
            "\\end{thebibliography}",
        )),
    )]);
    let prepared = prepare(&source.path().join("paper.tex")).await;
    let (_, checks) = prepared.collect();
    assert!(
        checks.undefined_citations.is_empty(),
        "{:?}",
        checks.undefined_citations
    );
}

#[tokio::test]
async fn a_compiled_bibliography_is_kept_in_the_captured_source() {
    let body = "\\cite{card} and \\cite{forgotten}.\n\\bibliography{refs}";
    let bbl = b"\\begin{thebibliography}{1}\n\\bibitem{card} D. Card.\n\\end{thebibliography}\n";
    let source = project(&[("paper.tex", &tex(body)), ("paper.bbl", bbl)]);
    let prepared = prepare(&source.path().join("paper.tex")).await;
    assert!(Path::new(&prepared.source).join("paper.bbl").is_file());

    let (_, checks) = prepared.collect();
    assert_eq!(checks.compiled_entries, 1);
    assert_eq!(checks.undefined_citations, ["forgotten"]);
    assert_eq!(
        checks.skipped_bib_files,
        [SkippedBibFile {
            file: "refs.bib".into(),
            reason: "not found".into()
        }]
    );
}

#[tokio::test]
async fn a_biber_compiled_bibliography_defines_its_keys() {
    let body = "\\autocite{card}.\n\\printbibliography";
    let bbl = b"\\refsection{0}\n  \\entry{card}{article}{}\n  \\endentry\n\\endrefsection\n";
    let source = project(&[("main.tex", &tex(body)), ("main.bbl", bbl)]);
    let prepared = prepare(source.path()).await;
    let (_, checks) = prepared.collect();
    assert!(checks.undefined_citations.is_empty());
    assert_eq!(checks.compiled_entries, 1);
}

#[tokio::test]
async fn a_latin1_bibliography_is_read() {
    let body = "\\cite{munoz}.\n\\bibliography{refs}";
    let bib =
        b"@article{munoz, author={Mu\xf1oz, Jos\xe9}, title={A study of wages}, year={1994}}\n";
    let source = project(&[("paper.tex", &tex(body)), ("refs.bib", bib)]);
    let prepared = prepare(&source.path().join("paper.tex")).await;
    let (references, checks) = prepared.collect();
    assert!(checks.undefined_citations.is_empty());
    assert!(checks.skipped_bib_files.is_empty());
    assert_eq!(references[0].first_author.as_deref(), Some("Muñoz"));
}

#[tokio::test]
async fn an_oversized_bibliography_is_a_limitation_and_not_undefined_keys() {
    let body = "\\cite{huge}.\n\\bibliography{refs}";
    let bib = "@misc{huge, title={A very large exported library entry}}\n".repeat(90_000);
    assert!(bib.len() as u64 > MAX_BIB_BYTES);
    let source = project(&[("paper.tex", &tex(body)), ("refs.bib", bib.as_bytes())]);
    let prepared = prepare(&source.path().join("paper.tex")).await;
    let (_, checks) = prepared.collect();
    assert!(checks.undefined_citations.is_empty());
    assert_eq!(
        checks.skipped_bib_files,
        [SkippedBibFile {
            file: "refs.bib".into(),
            reason: "larger than the 4 MB limit".into()
        }]
    );
    let output = prepared.run().await;
    assert_eq!(finding_ids(&output), ["references-limitation-1"]);
    assert_eq!(
        output["findings"][0]["problem"],
        "No bibliography could be read for this LaTeX source: refs.bib (larger than the 4 MB limit). Its citations could not be compared with one, so undefined citations are not reported."
    );
}

#[tokio::test]
async fn no_bibliography_at_all_is_a_limitation_and_not_undefined_keys() {
    let source = project(&[("paper.tex", &tex("\\cite{card} and \\cite{neumark}."))]);
    let prepared = prepare(&source.path().join("paper.tex")).await;
    let output = prepared.run().await;
    assert_eq!(finding_ids(&output), ["references-limitation-1"]);
    assert!(output["findings"][0]["problem"]
        .as_str()
        .unwrap()
        .starts_with("No bibliography file was found"));
}

fn numbered_entries(count: usize) -> String {
    (0..count)
        .map(|index| {
            format!("@misc{{key{index}, author={{Card, David}}, title={{Work {index}}}}}\n")
        })
        .collect()
}

#[tokio::test]
async fn a_large_master_bibliography_is_read_past_its_first_two_thousand_entries() {
    let body = "\\cite{key2400} and \\cite{forgotten}.\n\\bibliography{refs}";
    let bib = numbered_entries(2_500);
    let source = project(&[("paper.tex", &tex(body)), ("refs.bib", bib.as_bytes())]);
    let prepared = prepare(&source.path().join("paper.tex")).await;
    let (references, checks) = prepared.collect();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].key, "key2400");
    assert_eq!(checks.uncited_entries, 2_499);
    assert_eq!(checks.undefined_citations, ["forgotten"]);
}

#[tokio::test]
async fn reaching_the_entry_limit_withholds_undefined_keys() {
    let body = "\\cite{key0} and \\cite{key100000}.\n\\bibliography{refs}";
    let bib: String = (0..=100_000)
        .map(|index| format!("@misc{{key{index},t={{T}}}}\n"))
        .collect();
    assert!(bib.len() as u64 <= MAX_BIB_BYTES);
    let source = project(&[("paper.tex", &tex(body)), ("refs.bib", bib.as_bytes())]);
    let prepared = prepare(&source.path().join("paper.tex")).await;
    let (references, checks) = prepared.collect();
    assert_eq!(references.len(), 1);
    assert!(checks.undefined_citations.is_empty());
    assert_eq!(
        checks.skipped_bib_files[0].reason,
        "reading stopped after 100000 entries"
    );
    assert!(checks.limitations[0].starts_with("Part of the bibliography could not be read"));
}

#[tokio::test]
async fn a_missing_second_file_withholds_undefined_keys() {
    let body = "\\cite{card} and \\cite{elsewhere}.\n\\bibliography{refs,other}";
    let bib = b"@article{card, author={Card, David}, title={Minimum wages}}\n";
    let source = project(&[("paper.tex", &tex(body)), ("refs.bib", bib)]);
    let prepared = prepare(&source.path().join("paper.tex")).await;
    let (references, checks) = prepared.collect();
    assert_eq!(references.len(), 1);
    assert!(checks.undefined_citations.is_empty());
    assert!(checks.limitations[0].contains("other.bib (not found)"));
}

#[tokio::test]
async fn an_unfinished_entry_withholds_undefined_keys_after_it() {
    let body = "\\cite{card} and \\cite{after}.\n\\bibliography{refs}";
    let bib = b"@article{card, title={Fine}}\n@article{broken, title={Open\n@article{after, title={Lost}}\n";
    let source = project(&[("paper.tex", &tex(body)), ("refs.bib", bib)]);
    let prepared = prepare(&source.path().join("paper.tex")).await;
    let (_, checks) = prepared.collect();
    assert!(checks.undefined_citations.is_empty());
    assert_eq!(
        checks.skipped_bib_files[0].reason,
        "reading stopped at an unfinished entry"
    );
}

#[tokio::test]
async fn a_key_missing_from_a_complete_bibliography_is_still_reported() {
    // A commented-out file must not make the file in use look incomplete, and
    // a file reached with `..` is read from the captured project.
    let body = "\\cite{card} and \\cite{forgotten}.\n% \\bibliography{old}\n\\bibliography{../shared/refs}";
    let bib = b"@article{card, author={Card, David}, title={Minimum wages}}\n";
    let parent = project(&[("paper/paper.tex", &tex(body)), ("shared/refs.bib", bib)]);
    let prepared = prepare(&parent.path().join("paper/paper.tex")).await;
    let (references, checks) = prepared.collect();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].source_file, "_external/up1/shared/refs.bib");
    assert_eq!(checks.undefined_citations, ["forgotten"]);
    assert!(checks.skipped_bib_files.is_empty());
    let output = prepared.run().await;
    assert_eq!(finding_ids(&output), [UNDEFINED]);
}

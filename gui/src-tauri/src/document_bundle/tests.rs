use super::*;

#[test]
fn docx_entry_limit_uses_bytes_actually_read() {
    let mut exact = std::io::Cursor::new(b"12345678");
    assert_eq!(
        read_docx_entry_limited(&mut exact, 8, "test entry").unwrap(),
        b"12345678"
    );

    let mut oversized = std::io::Cursor::new(b"123456789");
    let error = read_docx_entry_limited(&mut oversized, 8, "test entry").unwrap_err();
    assert!(error.contains("decompressed safety limit"), "{error}");
}

#[test]
fn docx_media_failure_publishes_no_partial_files() {
    use std::io::Write as _;
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("oversized.docx");
    let file = fs::File::create(&path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    archive.start_file("word/media/first.png", options).unwrap();
    archive.write_all(b"12345678").unwrap();
    archive
        .start_file("word/media/second.png", options)
        .unwrap();
    archive.write_all(b"abcdefgh").unwrap();
    archive.finish().unwrap();

    let run = tempfile::tempdir().unwrap();
    let error = copy_docx_media_with_limits(&path, run.path(), 16, 12, 10, 100).unwrap_err();
    assert!(error.contains("cumulative"), "{error}");
    assert!(!run.path().join("artifacts/document/figures").exists());
    assert!(fs::read_dir(run.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".pipeline-docx-media-")));
}

#[test]
fn docx_archive_entry_count_is_bounded_before_media_scan() {
    use std::io::Write as _;
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("many-entries.docx");
    let file = fs::File::create(&path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for index in 0..3 {
        archive
            .start_file(format!("metadata/{index}.xml"), options)
            .unwrap();
        archive.write_all(b"x").unwrap();
    }
    archive.finish().unwrap();

    let run = tempfile::tempdir().unwrap();
    let error = copy_docx_media_with_limits(&path, run.path(), 16, 64, 10, 2).unwrap_err();
    assert!(error.contains("contains 3 entries"), "{error}");
    assert!(!run.path().read_dir().unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".pipeline-docx-media-")));
}

#[test]
fn jpeg_dimension_parser_reads_start_of_frame() {
    let bytes = [
        0xff, 0xd8, // SOI
        0xff, 0xc0, // baseline SOF
        0x00, 0x11, // segment length
        0x08, // precision
        0x04, 0xb0, // 1200px high
        0x03, 0xf0, // 1008px wide
        0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x00, 0x03, 0x11, 0x00,
    ];
    assert_eq!(jpeg_dimensions(&bytes), (Some(1008), Some(1200)));
}

fn extraction(text: &str) -> ExtractionResult {
    ExtractionResult {
        text: text.to_string(),
        method: "llm".to_string(),
        source_path: "/tmp/paper.pdf".to_string(),
        paper_hash: "0123456789abcdef".to_string(),
        quality_notes: Vec::new(),
    }
}

#[test]
fn builds_page_and_structural_nodes() {
    let temp = tempfile::tempdir().unwrap();
    let build = build(
        &extraction(
            "<!-- PAGE 1 -->\n# Introduction\n\nText\n\n$$y=x$$\n\nFigure 1: Impulse response",
        ),
        temp.path(),
    )
    .unwrap();
    assert_eq!(build.bundle.schema_version, "1.0");
    assert!(build
        .bundle
        .nodes
        .iter()
        .any(|node| node.kind == "page_text"));
    assert!(build.bundle.nodes.iter().any(|node| node.kind == "section"));
    assert!(build
        .bundle
        .nodes
        .iter()
        .any(|node| node.kind == "equation"));
    assert!(build.bundle.nodes.iter().any(|node| node.kind == "figure"));
}

#[test]
fn rejects_unsafe_asset_paths() {
    let mut bundle = build(&extraction("Text"), tempfile::tempdir().unwrap().path())
        .unwrap()
        .bundle;
    bundle.assets.push(DocumentAsset {
        id: "bad".to_string(),
        kind: "figure".to_string(),
        label: "Bad".to_string(),
        rel_path: "../outside.png".to_string(),
        media_type: "image/png".to_string(),
        page: None,
        width: None,
        height: None,
        provenance: DocumentProvenance {
            origin_id: "origin-primary".to_string(),
            method: "test".to_string(),
            confidence: None,
        },
    });
    assert!(bundle.validate().is_err());
}

#[test]
fn markdown_view_contains_stable_node_markers() {
    let bundle = build(
        &extraction("A complete paragraph."),
        tempfile::tempdir().unwrap().path(),
    )
    .unwrap()
    .bundle;
    let markdown = bundle.to_markdown();
    assert!(markdown.contains("DOCUMENT_NODE page_text-00001"));
    assert!(markdown.contains("A complete paragraph."));
}

#[test]
fn readable_projection_can_use_exact_text_without_page_node_duplication() {
    let mut bundle = build(
        &extraction("Compatibility text"),
        tempfile::tempdir().unwrap().path(),
    )
    .unwrap()
    .bundle;
    bundle.nodes.retain(|node| node.kind != "page_text");

    let markdown = bundle.to_markdown_with_text("<!-- PAGE 1 -->\nExact native text");
    assert!(markdown.contains("Exact native text"));
    assert!(!markdown.contains("DOCUMENT_NODE page_text"));
}

#[test]
fn canonical_bundle_serialization_enforces_artifact_limit() {
    let mut bundle = build(
        &extraction("Compatibility text"),
        tempfile::tempdir().unwrap().path(),
    )
    .unwrap()
    .bundle;
    bundle.nodes[0].text = "x".repeat(MAX_DOCUMENT_BUNDLE_BYTES);

    assert!(bundle
        .to_json_pretty()
        .unwrap_err()
        .contains("durable artifact limit"));
    assert!(bundle
        .to_jsonl()
        .unwrap_err()
        .contains("durable artifact limit"));
}

#[test]
fn model_index_enforces_direct_provider_limit() {
    let mut bundle = build(
        &extraction("Compatibility text"),
        tempfile::tempdir().unwrap().path(),
    )
    .unwrap()
    .bundle;
    bundle.nodes[0].kind = "equation".to_string();
    bundle.nodes[0].text = "x".repeat(MAX_MODEL_DOCUMENT_INDEX_BYTES);

    assert!(bundle
        .to_model_index_pretty()
        .unwrap_err()
        .contains("model-readable limit"));
}

#[test]
fn model_index_omits_duplicate_prose_but_keeps_semantic_content() {
    let bundle = build(
        &extraction("<!-- PAGE 1 -->\nA paragraph.\n\n$$y=x$$"),
        tempfile::tempdir().unwrap().path(),
    )
    .unwrap()
    .bundle;
    let index: serde_json::Value =
        serde_json::from_str(&bundle.to_model_index_pretty().unwrap()).unwrap();
    let nodes = index["nodes"].as_array().unwrap();
    let equation = nodes
        .iter()
        .find(|node| node["kind"] == "equation")
        .unwrap();

    assert!(!nodes.iter().any(|node| node["kind"] == "page_text"));
    assert_eq!(equation["text"], "$$y=x$$");
    assert_eq!(index["format"], "pipeline.document-index.v1");
}

#[test]
fn paddle_structure_preserves_roles_exact_markdown_and_page_evidence() {
    let mut bundle = build(
        &extraction("<!-- PAGE 2 -->\nA complete paragraph."),
        tempfile::tempdir().unwrap().path(),
    )
    .unwrap()
    .bundle;
    let structure = crate::pipeline::extract::PaddleStructure {
        schema_version: 1,
        parser: "paddleocr-vl-fast".to_string(),
        parser_version: "1.6".to_string(),
        settings: serde_json::Value::Null,
        quality_notes: Vec::new(),
        pages: vec![crate::pipeline::extract::PaddleStructuredPage {
            number: 2,
            markdown: String::new(),
            source_text_chars: None,
            width: None,
            height: None,
            blocks: vec![
                crate::pipeline::extract::PaddleStructuredBlock {
                    block_id: "paddle-page-0002-block-0001".to_string(),
                    role: "page_header".to_string(),
                    block_label: String::new(),
                    markdown: "Running title".to_string(),
                    text: "Running title".to_string(),
                    boundary: Some("top".to_string()),
                    note_marker: None,
                    order: Some(1),
                    bbox: Vec::new(),
                    polygon: Vec::new(),
                    confidence: None,
                    asset_files: Vec::new(),
                    raw: serde_json::Value::Null,
                },
                crate::pipeline::extract::PaddleStructuredBlock {
                    block_id: "paddle-page-0002-block-0003".to_string(),
                    role: "possible_footnote".to_string(),
                    block_label: String::new(),
                    markdown: "2 A qualification with *source emphasis*.".to_string(),
                    text: "2 A qualification with source emphasis.".to_string(),
                    boundary: Some("bottom".to_string()),
                    note_marker: Some("2".to_string()),
                    order: Some(2),
                    bbox: Vec::new(),
                    polygon: Vec::new(),
                    confidence: None,
                    asset_files: Vec::new(),
                    raw: serde_json::Value::Null,
                },
            ],
        }],
    };
    add_paddle_structured_nodes(&mut bundle, &structure);

    let header = bundle
        .nodes
        .iter()
        .find(|node| node.kind == "page_header")
        .unwrap();
    assert_eq!(header.text, "Running title");
    assert_eq!(header.provenance.confidence, Some(0.8));
    let possible = bundle
        .nodes
        .iter()
        .find(|node| node.kind == "possible_footnote")
        .unwrap();
    assert_eq!(possible.number.as_deref(), Some("2"));
    assert_eq!(possible.provenance.confidence, Some(0.6));
    let representation = possible
        .representations
        .iter()
        .find(|representation| representation.format == "paddle_block")
        .unwrap();
    assert_eq!(
        representation.content["markdown"],
        "2 A qualification with *source emphasis*."
    );
}

#[test]
fn full_paddle_structure_preserves_layout_semantics_and_merges_formula_numbers() {
    let mut bundle = build(
        &extraction("<!-- PAGE 1 -->\n# Results\n\nA table follows."),
        tempfile::tempdir().unwrap().path(),
    )
    .unwrap()
    .bundle;
    let block =
        |id: &str, label: &str, markdown: &str, order: u32, confidence: f32, bbox: [f64; 4]| {
            crate::pipeline::extract::PaddleStructuredBlock {
                block_id: id.to_string(),
                role: label.to_string(),
                block_label: label.to_string(),
                markdown: markdown.to_string(),
                text: markdown.trim_start_matches('#').trim().to_string(),
                boundary: None,
                note_marker: None,
                order: Some(order),
                bbox: bbox.to_vec(),
                polygon: Vec::new(),
                confidence: Some(confidence),
                asset_files: Vec::new(),
                raw: serde_json::json!({ "block_order": order }),
            }
        };
    let structure = crate::pipeline::extract::PaddleStructure {
        schema_version: 2,
        parser: "paddleocr-vl-full".to_string(),
        parser_version: "3.7.0".to_string(),
        settings: serde_json::json!({ "merge_tables": true, "relevel_titles": true }),
        quality_notes: Vec::new(),
        pages: vec![crate::pipeline::extract::PaddleStructuredPage {
            number: 1,
            markdown: "# Results\n\n## Baseline\n\n| x | y |".to_string(),
            source_text_chars: None,
            width: Some(1200),
            height: Some(1600),
            blocks: vec![
                block(
                    "title",
                    "doc_title",
                    "# Results",
                    1,
                    0.98,
                    [50.0, 20.0, 800.0, 60.0],
                ),
                block(
                    "subtitle",
                    "paragraph_title",
                    "## Baseline",
                    2,
                    0.94,
                    [50.0, 80.0, 600.0, 110.0],
                ),
                block(
                    "table",
                    "table",
                    "| x | y |\n|---|---|\n|1|2|",
                    3,
                    0.91,
                    [50.0, 140.0, 900.0, 300.0],
                ),
                block(
                    "formula",
                    "formula",
                    r"$$y=\beta x$$",
                    4,
                    0.89,
                    [250.0, 340.0, 750.0, 380.0],
                ),
                block(
                    "formula-number",
                    "formula_number",
                    "(1)",
                    5,
                    0.96,
                    [1050.0, 342.0, 1100.0, 378.0],
                ),
                block(
                    "caption",
                    "vision_footnote",
                    "Figure 1. Fit",
                    6,
                    0.87,
                    [50.0, 420.0, 600.0, 450.0],
                ),
                block(
                    "orphan-number",
                    "formula_number",
                    "(99)",
                    7,
                    0.82,
                    [1050.0, 500.0, 1100.0, 530.0],
                ),
                block(
                    "prose",
                    "text",
                    "Not an equation",
                    8,
                    0.90,
                    [50.0, 500.0, 700.0, 535.0],
                ),
                block(
                    "leading-number",
                    "formula_number",
                    r"$[A.2]$",
                    9,
                    0.95,
                    [1050.0, 602.0, 1100.0, 638.0],
                ),
                block(
                    "formula-2",
                    "display_formula",
                    r"$$z=\gamma x$$",
                    10,
                    0.88,
                    [250.0, 600.0, 750.0, 640.0],
                ),
            ],
        }],
    };
    add_paddle_structured_nodes(&mut bundle, &structure);

    let title = bundle
        .nodes
        .iter()
        .find(|node| node.text == "Results" && node.provenance.method == "paddleocr-vl-full")
        .unwrap();
    let subtitle = bundle
        .nodes
        .iter()
        .find(|node| node.text == "Baseline" && node.provenance.method == "paddleocr-vl-full")
        .unwrap();
    let table = bundle
        .nodes
        .iter()
        .find(|node| node.kind == "table")
        .unwrap();
    let equations: Vec<&DocumentNode> = bundle
        .nodes
        .iter()
        .filter(|node| node.kind == "equation" && node.provenance.method == "paddleocr-vl-full")
        .collect();
    let equation = equations[0];
    let caption = bundle
        .nodes
        .iter()
        .find(|node| node.text == "Figure 1. Fit")
        .unwrap();
    assert_eq!(title.kind, "section");
    assert_eq!(subtitle.parent_id.as_deref(), Some(title.id.as_str()));
    assert_eq!(table.parent_id.as_deref(), Some(subtitle.id.as_str()));
    assert_eq!(equations.len(), 2);
    assert_eq!(equations[0].number.as_deref(), Some("1"));
    assert_eq!(equations[1].number.as_deref(), Some("A.2"));
    assert_eq!(equations[0].representations.len(), 2);
    assert_eq!(equations[1].representations.len(), 2);
    assert_eq!(
        equations[0].representations[1].content["block_label"],
        "formula_number"
    );
    assert!(bundle.nodes.iter().any(|node| {
        node.kind == "text_block"
            && node.text == "(99)"
            && node.provenance.method == "paddleocr-vl-full"
    }));
    assert_eq!(equation.provenance.method, "paddleocr-vl-full");
    assert_eq!(caption.kind, "caption");
    assert_eq!(table.provenance.confidence, Some(0.91));
    assert_eq!(table.representations[0].content["bbox"][0], 50.0);
    assert_eq!(
        table.representations[0].content["parser_settings"]["merge_tables"],
        true
    );
}

#[test]
fn paddle_formula_numbers_require_unambiguous_page_geometry() {
    let mut bundle = build(
        &extraction("<!-- PAGE 1 -->\nEquations."),
        tempfile::tempdir().unwrap().path(),
    )
    .unwrap()
    .bundle;
    let block =
        |id: &str, label: &str, markdown: &str, order: u32, confidence: f32, bbox: [f64; 4]| {
            crate::pipeline::extract::PaddleStructuredBlock {
                block_id: id.to_string(),
                role: label.to_string(),
                block_label: label.to_string(),
                markdown: markdown.to_string(),
                text: markdown.to_string(),
                boundary: None,
                note_marker: None,
                order: Some(order),
                bbox: bbox.to_vec(),
                polygon: Vec::new(),
                confidence: Some(confidence),
                asset_files: Vec::new(),
                raw: serde_json::Value::Null,
            }
        };
    let structure = crate::pipeline::extract::PaddleStructure {
        schema_version: 2,
        parser: "paddleocr-vl-full".to_string(),
        parser_version: "3.7.0".to_string(),
        settings: serde_json::Value::Null,
        quality_notes: Vec::new(),
        pages: vec![crate::pipeline::extract::PaddleStructuredPage {
            number: 1,
            markdown: String::new(),
            source_text_chars: None,
            width: Some(1200),
            height: Some(1600),
            blocks: vec![
                // Reading-order-leading number for the left column.
                block(
                    "left-number",
                    "formula_number",
                    "(1)",
                    1,
                    0.96,
                    [480.0, 100.0, 520.0, 130.0],
                ),
                block(
                    "left-equation",
                    "formula",
                    "$$a=1$$",
                    2,
                    0.94,
                    [50.0, 98.0, 440.0, 132.0],
                ),
                block(
                    "right-equation",
                    "formula",
                    "$$b=2$$",
                    3,
                    0.93,
                    [650.0, 98.0, 1040.0, 132.0],
                ),
                // Reading-order-trailing number for the right column.
                block(
                    "right-number",
                    "formula_number",
                    "(2)",
                    4,
                    0.95,
                    [1070.0, 100.0, 1110.0, 130.0],
                ),
                block(
                    "orphan",
                    "formula_number",
                    "(77)",
                    5,
                    0.90,
                    [1070.0, 250.0, 1110.0, 280.0],
                ),
                block(
                    "intervening-prose",
                    "text",
                    "An unrelated number follows.",
                    6,
                    0.90,
                    [50.0, 245.0, 600.0, 285.0],
                ),
                block(
                    "low-confidence-equation",
                    "formula",
                    "$$c=3$$",
                    7,
                    0.92,
                    [650.0, 300.0, 1040.0, 332.0],
                ),
                block(
                    "low-confidence-number",
                    "formula_number",
                    "(8)",
                    8,
                    0.20,
                    [1070.0, 301.0, 1110.0, 331.0],
                ),
                block(
                    "ambiguous-left",
                    "formula",
                    "$$d=4$$",
                    9,
                    0.92,
                    [50.0, 400.0, 400.0, 432.0],
                ),
                block(
                    "ambiguous-number",
                    "formula_number",
                    "(9)",
                    10,
                    0.95,
                    [575.0, 401.0, 625.0, 431.0],
                ),
                block(
                    "ambiguous-right",
                    "formula",
                    "$$e=5$$",
                    11,
                    0.92,
                    [800.0, 400.0, 1150.0, 432.0],
                ),
            ],
        }],
    };

    add_paddle_structured_nodes(&mut bundle, &structure);

    let equation = |text: &str| {
        bundle
            .nodes
            .iter()
            .find(|node| node.kind == "equation" && node.text == text)
            .unwrap()
    };
    assert_eq!(equation("$$a=1$$").number.as_deref(), Some("1"));
    assert_eq!(equation("$$b=2$$").number.as_deref(), Some("2"));
    for text in ["$$c=3$$", "$$d=4$$", "$$e=5$$"] {
        assert_eq!(equation(text).number.as_deref(), None, "{text}");
    }
    for number in ["(77)", "(8)", "(9)"] {
        assert!(bundle.nodes.iter().any(|node| {
            node.kind == "text_block"
                && node.text == number
                && node.provenance.method == "paddleocr-vl-full"
        }));
    }
}

#[test]
fn docx_preserves_tables_equations_and_media() {
    use std::io::Write as _;
    let source_dir = tempfile::tempdir().unwrap();
    let path = source_dir.path().join("paper.docx");
    let file = fs::File::create(&path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    archive.start_file("word/document.xml", options).unwrap();
    archive
            .write_all(
                br#"<?xml version="1.0" encoding="UTF-8"?>
                <w:document xmlns:w="w" xmlns:m="m"><w:body>
                  <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Introduction</w:t></w:r></w:p>
                  <w:p><w:r><w:t>Aggregate output falls.</w:t></w:r><m:oMath><m:r><m:t>y_t=x_t</m:t></m:r></m:oMath></w:p>
                  <w:tbl><w:tr><w:tc><w:p><w:r><w:t>Variable</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Estimate</w:t></w:r></w:p></w:tc></w:tr>
                  <w:tr><w:tc><w:p><w:r><w:t>Output</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>-0.2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
                </w:body></w:document>"#,
            )
            .unwrap();
    archive
        .start_file("word/media/image1.png", options)
        .unwrap();
    let mut png = vec![0u8; 24];
    png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    png[16..20].copy_from_slice(&640u32.to_be_bytes());
    png[20..24].copy_from_slice(&480u32.to_be_bytes());
    archive.write_all(&png).unwrap();
    archive.finish().unwrap();

    let text = extract_docx_text(&path).unwrap();
    assert!(text.contains("# Introduction"));
    assert!(text.contains("| Variable | Estimate |"));

    let run = tempfile::tempdir().unwrap();
    let build = build(
        &ExtractionResult {
            text,
            method: "docx".to_string(),
            source_path: path.to_string_lossy().to_string(),
            paper_hash: "0123456789abcdef".to_string(),
            quality_notes: Vec::new(),
        },
        run.path(),
    )
    .unwrap();
    assert!(build.bundle.nodes.iter().any(|node| {
        node.kind == "equation"
            && node
                .representations
                .iter()
                .any(|representation| representation.format == "omml")
    }));
    assert!(build.bundle.nodes.iter().any(|node| {
        node.kind == "table"
            && node
                .representations
                .iter()
                .any(|representation| representation.format == "grid")
    }));
    assert_eq!(build.bundle.assets.len(), 1);
    assert_eq!(build.bundle.assets[0].width, Some(640));
}

#[test]
fn docx_appends_substantive_footnotes_and_endnotes() {
    use std::io::Write as _;
    let source_dir = tempfile::tempdir().unwrap();
    let path = source_dir.path().join("paper.docx");
    let file = fs::File::create(&path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    archive.start_file("word/document.xml", options).unwrap();
    archive
        .write_all(
            br#"<w:document xmlns:w="w"><w:body>
              <w:p><w:r><w:t>Body text.</w:t></w:r></w:p>
            </w:body></w:document>"#,
        )
        .unwrap();
    archive.start_file("word/footnotes.xml", options).unwrap();
    archive
        .write_all(
            br#"<w:footnotes xmlns:w="w">
              <w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:t>sep</w:t></w:r></w:p></w:footnote>
              <w:footnote w:type="continuationSeparator" w:id="0"><w:p/></w:footnote>
              <w:footnote w:id="2"><w:p><w:r><w:t>See the appendix.</w:t></w:r></w:p></w:footnote>
              <w:footnote w:id="3"><w:p><w:r><w:t>Second</w:t></w:r></w:p><w:p><w:r><w:t>note.</w:t></w:r></w:p></w:footnote>
            </w:footnotes>"#,
        )
        .unwrap();
    // Endnotes part with separator stubs only: no section is emitted.
    archive.start_file("word/endnotes.xml", options).unwrap();
    archive
        .write_all(
            br#"<w:endnotes xmlns:w="w">
              <w:endnote w:type="separator" w:id="-1"><w:p/></w:endnote>
            </w:endnotes>"#,
        )
        .unwrap();
    archive.finish().unwrap();

    let text = extract_docx_text(&path).unwrap();
    assert!(text.contains("Body text."));
    assert!(text.contains("## Footnotes"));
    assert!(text.contains("1. See the appendix."));
    assert!(text.contains("2. Second note."));
    assert!(!text.contains("sep"), "separator stubs are not notes");
    assert!(!text.contains("## Endnotes"));
}

//! Word extraction over real OOXML structure, through the zip reader.
use super::*;

const NAMESPACES: &str = concat!(
    r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
    r#"xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" "#,
    r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" "#,
    r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
    r#"xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" "#,
    r#"xmlns:v="urn:schemas-microsoft-com:vml" "#,
    r#"xmlns:w16se="http://schemas.microsoft.com/office/word/2015/wordml/symex""#,
);

fn paragraph(text: &str) -> String {
    format!(r#"<w:p w:rsidR="00A1" w:rsidRDefault="00B2"><w:r><w:t>{text}</w:t></w:r></w:p>"#)
}

fn cell(content: &str) -> String {
    format!(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr>{content}</w:tc>"#)
}

fn table(rows: &[Vec<String>]) -> String {
    let rows: String = rows
        .iter()
        .map(|cells| format!(r#"<w:tr w:rsidR="00A1">{}</w:tr>"#, cells.concat()))
        .collect();
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>{rows}</w:tbl>"#
    )
}

/// What Word writes for a text box anchored in the middle of a paragraph: the
/// same box as a drawing and, for older readers, as a VML shape.
fn text_box(text: &str) -> String {
    let content = format!("<w:txbxContent>{}</w:txbxContent>", paragraph(text));
    format!(
        concat!(
            r#"<w:r><w:rPr><w:noProof/></w:rPr><mc:AlternateContent><mc:Choice Requires="wps">"#,
            r#"<w:drawing><wp:anchor><wps:wsp><wps:txbx>{content}</wps:txbx></wps:wsp></wp:anchor></w:drawing>"#,
            r#"</mc:Choice><mc:Fallback><w:pict><v:shape><v:textbox>{content}</v:textbox></v:shape></w:pict>"#,
            r#"</mc:Fallback></mc:AlternateContent></w:r>"#,
        ),
        content = content
    )
}

fn document(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NAMESPACES}><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr></w:body></w:document>"#
    )
}

fn write_docx(directory: &Path, parts: &[(&str, String)]) -> PathBuf {
    use std::io::Write as _;
    let path = directory.join("paper.docx");
    let mut archive = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for (name, content) in parts {
        archive.start_file(*name, options).unwrap();
        archive.write_all(content.as_bytes()).unwrap();
    }
    archive.finish().unwrap();
    path
}

fn extract(body: &str) -> String {
    extract_parts(&[("word/document.xml", document(body))])
}

fn extract_parts(parts: &[(&str, String)]) -> String {
    let directory = tempfile::tempdir().unwrap();
    extract_docx_text(&write_docx(directory.path(), parts)).unwrap()
}

fn occurrences(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

#[test]
fn a_table_ending_in_an_empty_cell_keeps_the_paragraph_after_it() {
    let body = [
        paragraph("Before table."),
        table(&[
            vec![cell(&paragraph("Variable")), cell(&paragraph("Estimate"))],
            vec![cell(&paragraph("Output")), cell(r#"<w:p w:rsidR="1"/>"#)],
        ]),
        paragraph("The first paragraph after the table."),
        paragraph("Later paragraph."),
    ]
    .concat();
    let text = extract(&body);
    assert_eq!(
        text.split("## Extracted tables").next().unwrap(),
        "Before table.\n\nVariable\n\nEstimate\n\nOutput\n\n\
         The first paragraph after the table.\n\nLater paragraph.\n\n"
    );
    assert!(text.contains("| Output |  |"), "{text}");
}

#[test]
fn an_empty_cell_inside_a_row_keeps_the_next_cell_in_the_running_text() {
    let body = table(&[vec![
        cell(&paragraph("First")),
        cell(r#"<w:p w:rsidR="1"/>"#),
        cell(&paragraph("Third")),
        cell("<w:p/>"),
        cell(&paragraph("Fifth")),
    ]]);
    let text = extract(&body);
    assert_eq!(
        text.split("## Extracted tables").next().unwrap(),
        "First\n\nThird\n\nFifth\n\n"
    );
    assert!(text.contains("| First |  | Third |  | Fifth |"), "{text}");
}

#[test]
fn a_text_box_keeps_its_paragraph_whole_and_appears_once() {
    let body = [
        paragraph("Earlier paragraph."),
        format!(
            r#"<w:p w:rsidR="1"><w:r><w:t xml:space="preserve">Text before the anchor </w:t></w:r>{}<w:r><w:t>and text after it.</w:t></w:r></w:p>"#,
            text_box("Figure 1: The box caption.")
        ),
        paragraph("Later paragraph."),
    ]
    .concat();
    assert_eq!(
        extract(&body),
        "Earlier paragraph.\n\nText before the anchor and text after it.\n\n\
         Figure 1: The box caption.\n\nLater paragraph.\n\n"
    );
}

#[test]
fn a_nested_paragraph_cannot_end_a_heading_or_a_table_cell() {
    let body = [
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>Results</w:t></w:r>{}<w:r><w:t xml:space="preserve"> in detail</w:t></w:r></w:p>"#,
            text_box("Box in a heading.")
        ),
        table(&[vec![cell(&format!(
            r#"<w:p><w:r><w:t>Cell</w:t></w:r>{}</w:p>"#,
            text_box("Box in a cell.")
        ))]]),
        paragraph("After the table."),
    ]
    .concat();
    let text = extract(&body);
    assert_eq!(
        text.split("## Extracted tables").next().unwrap(),
        "## Results in detail\n\nBox in a heading.\n\nCell\n\nBox in a cell.\n\nAfter the table.\n\n"
    );
    assert_eq!(occurrences(&text, "Box in a cell."), 2, "body and table");
}

#[test]
fn an_alternate_block_with_no_text_in_its_choice_reads_the_fallback() {
    let body = concat!(
        r#"<w:p><w:r><w:t xml:space="preserve">Before </w:t></w:r><w:r><mc:AlternateContent>"#,
        r#"<mc:Choice Requires="w16se"><w16se:symEx w16se:font="Segoe UI Emoji" w16se:char="1F4C8"/></mc:Choice>"#,
        r#"<mc:Fallback><w:t>📈</w:t></mc:Fallback></mc:AlternateContent></w:r>"#,
        r#"<w:r><w:t xml:space="preserve"> after.</w:t></w:r></w:p>"#,
    );
    assert_eq!(extract(body), "Before 📈 after.\n\n");
}

#[test]
fn moved_away_and_deleted_text_is_not_read() {
    let body = concat!(
        r#"<w:p><w:pPr><w:rPr><w:del w:id="1" w:author="A"/><w:moveFrom w:id="2" w:author="A"/></w:rPr></w:pPr>"#,
        r#"<w:r><w:t xml:space="preserve">Kept </w:t></w:r>"#,
        r#"<w:moveFrom w:id="3" w:author="A"><w:r><w:t>old place </w:t></w:r></w:moveFrom>"#,
        r#"<w:del w:id="4" w:author="A"><w:r><w:delText>struck </w:delText></w:r></w:del>"#,
        r#"<w:ins w:id="5" w:author="A"><w:r><w:t xml:space="preserve">inserted </w:t></w:r></w:ins>"#,
        r#"<w:moveTo w:id="6" w:author="A"><w:r><w:t>new place.</w:t></w:r></w:moveTo></w:p>"#,
        r#"<w:moveFrom w:id="7" w:author="A"><w:p><w:r><w:t>Whole moved paragraph.</w:t></w:r></w:p></w:moveFrom>"#,
    );
    assert_eq!(extract(body), "Kept inserted new place.\n\n");
}

#[test]
fn notes_use_the_same_reader_as_the_body() {
    let footnotes = format!(
        concat!(
            r#"<w:footnotes {namespaces}>"#,
            r#"<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>"#,
            r#"<w:footnote w:type="continuationSeparator" w:id="0"><w:p w:rsidR="1"/></w:footnote>"#,
            r#"<w:footnote w:id="1"/>"#,
            r#"<w:footnote w:id="2"><w:p w:rsidR="1"/><w:p><w:r><w:t>After an empty line.</w:t></w:r></w:p></w:footnote>"#,
            r#"<w:footnote w:id="3"><w:p><w:r><w:t xml:space="preserve">Before the box </w:t></w:r>{text_box}<w:r><w:t>and after it.</w:t></w:r></w:p></w:footnote>"#,
            r#"<w:footnote w:id="4"><w:p><w:moveFrom w:id="9"><w:r><w:t>moved </w:t></w:r></w:moveFrom><w:r><w:t>Last note.</w:t></w:r></w:p></w:footnote>"#,
            r#"</w:footnotes>"#,
        ),
        namespaces = NAMESPACES,
        text_box = text_box("Box in a note."),
    );
    let text = extract_parts(&[
        ("word/document.xml", document(&paragraph("Body."))),
        ("word/footnotes.xml", footnotes),
    ]);
    assert_eq!(
        text,
        "Body.\n\n## Footnotes\n\n1. After an empty line.\n\
         2. Before the box and after it. Box in a note.\n3. Last note.\n\n"
    );
}

#[test]
fn an_unreadable_notes_part_costs_only_its_section() {
    let text = extract_parts(&[
        ("word/document.xml", document(&paragraph("Body."))),
        (
            "word/endnotes.xml",
            r#"<w:endnotes xmlns:w="w"><w:endnote w:id="1"><w:p></w:endnote></w:endnotes>"#.into(),
        ),
    ]);
    assert!(text.starts_with("Body.\n\n"), "{text}");
    assert!(text.contains("Endnotes were omitted"), "{text}");
}

#[test]
fn malformed_document_markup_is_still_refused() {
    let directory = tempfile::tempdir().unwrap();
    for (xml, message) in [
        (
            "<w:document><w:body><w:p></w:body></w:document>",
            "Invalid DOCX XML",
        ),
        ("<w:document><w:body>", "unclosed"),
        ("<!DOCTYPE x><w:document/>", "DTD"),
    ] {
        let path = write_docx(directory.path(), &[("word/document.xml", xml.to_string())]);
        let error = extract_docx_text(&path).unwrap_err();
        assert!(error.contains(message), "{error}");
    }
    let deep = format!("{}{}", "<w:x>".repeat(600), "</w:x>".repeat(600));
    let path = write_docx(directory.path(), &[("word/document.xml", deep)]);
    assert!(extract_docx_text(&path).unwrap_err().contains("limits"));
}

/// The markup Word writes around ordinary prose. Extraction of this document
/// must not move a byte, because `paper_hash` is computed from it.
fn ordinary_document() -> (String, String) {
    let body = concat!(
        r#"<w:p w14:paraId="1A" w:rsidR="00A1" xmlns:w14="w14"><w:pPr><w:pStyle w:val="Title"/><w:jc w:val="center"/></w:pPr><w:bookmarkStart w:id="0" w:name="_GoBack"/><w:r><w:t>Minimum Wages &amp; Employment</w:t></w:r><w:bookmarkEnd w:id="0"/></w:p>"#,
        r#"<w:p w:rsidR="00A1"/>"#,
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Introduction</w:t></w:r></w:p>"#,
        r#"<w:p w:rsidR="00B2"><w:pPr><w:tabs><w:tab w:val="left" w:pos="720"/><w:tab w:val="right" w:pos="9360"/></w:tabs><w:rPr><w:lang w:val="en-US"/></w:rPr></w:pPr>"#,
        r#"<w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">We estimate </w:t></w:r><w:proofErr w:type="spellStart"/><w:r><w:t>the</w:t></w:r><w:proofErr w:type="spellEnd"/><w:r><w:t xml:space="preserve"> effect</w:t></w:r>"#,
        r#"<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="2"/></w:r>"#,
        r#"<w:r><w:t xml:space="preserve"> using </w:t></w:r><w:hyperlink r:id="rId5" xmlns:r="r"><w:r><w:t>public data</w:t></w:r></w:hyperlink><w:r><w:t>.</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:t>Left</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t>Right</w:t></w:r><w:r><w:br/></w:r><w:r><w:t>Next line</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> SEQ Table \* ARABIC </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Table 1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
        r#"<w:sdt><w:sdtPr><w:alias w:val="Abstract"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Inside a content control.</w:t></w:r></w:p></w:sdtContent></w:sdt>"#,
        r#"<w:p><w:ins w:id="7" w:author="A"><w:r><w:t>An accepted insertion.</w:t></w:r></w:ins></w:p>"#,
        r#"<w:p><w:r><w:t xml:space="preserve">Inline </w:t></w:r><m:oMath><m:r><m:t>y=x</m:t></m:r></m:oMath><w:r><w:t xml:space="preserve"> math.</w:t></w:r></w:p>"#,
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/></w:tblPr><w:tblGrid><w:gridCol w:w="4680"/><w:gridCol w:w="4680"/></w:tblGrid>"#,
        r#"<w:tr w:rsidR="1"><w:trPr><w:tblHeader/></w:trPr><w:tc><w:tcPr><w:tcW w:w="4680" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Variable</w:t></w:r></w:p></w:tc>"#,
        r#"<w:tc><w:tcPr><w:tcW w:w="4680" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Estimate</w:t></w:r></w:p></w:tc></w:tr>"#,
        r#"<w:tr w:rsidR="2"><w:tc><w:tcPr/><w:p><w:r><w:t>Output</w:t></w:r></w:p><w:p><w:r><w:t>(log)</w:t></w:r></w:p></w:tc>"#,
        r#"<w:tc><w:tcPr/><w:p><w:r><w:t>-0.2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        r#"<w:p w:rsidR="00C3"><w:pPr><w:pStyle w:val="heading3"/></w:pPr><w:r><w:t>A lower-case heading style</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1Char"/></w:pPr><w:r><w:t>Not a heading.</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:t xml:space="preserve">   Surrounding spaces are trimmed.   </w:t></w:r></w:p>"#,
        r#"<w:p w:rsidR="00D4"><w:r><w:t>Final paragraph with entities: &lt;5% &#x3B1; &#945;.</w:t></w:r></w:p>"#,
    );
    let notes = format!(
        concat!(
            r#"<w:footnotes {namespaces}><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>"#,
            r#"<w:footnote w:id="2"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> A note </w:t></w:r></w:p><w:p><w:r><w:t>over two paragraphs.</w:t></w:r></w:p></w:footnote></w:footnotes>"#,
        ),
        namespaces = NAMESPACES
    );
    (document(body), notes)
}

const ORDINARY_MARKDOWN: &str = "Minimum Wages & Employment\n\n\
# Introduction\n\n\
We estimate the effect using public data.\n\n\
Left\tRight\nNext line\n\n\
Table 1\n\n\
Inside a content control.\n\n\
An accepted insertion.\n\n\
Inline y=x math.\n\n\
Variable\n\n\
Estimate\n\n\
Output\n\n\
(log)\n\n\
-0.2\n\n\
### A lower-case heading style\n\n\
Not a heading.\n\n\
Surrounding spaces are trimmed.\n\n\
Final paragraph with entities: <5% α α.\n\n\
## Extracted tables\n\n\
### Table 1\n\n\
| Variable | Estimate |\n\
| --- | --- |\n\
| Output(log) | -0.2 |\n\n\
## Footnotes\n\n\
1. A note over two paragraphs.\n\n";

#[test]
fn an_ordinary_document_extracts_to_exactly_the_same_markdown() {
    let (body, notes) = ordinary_document();
    let directory = tempfile::tempdir().unwrap();
    let path = write_docx(
        directory.path(),
        &[("word/document.xml", body), ("word/footnotes.xml", notes)],
    );
    let text = extract_docx_text(&path).unwrap();
    assert_eq!(text, ORDINARY_MARKDOWN);
    let parsed = parse_docx(&path).unwrap();
    assert_eq!(parsed.tables.len(), 1);
    assert_eq!(parsed.tables[0][1], ["Output(log)", "-0.2"]);
    assert_eq!(parsed.equations.len(), 1);
}

#[test]
fn a_word_bundle_records_the_bytes_of_the_file_it_read() {
    use sha2::{Digest, Sha256};
    let directory = tempfile::tempdir().unwrap();
    let path = write_docx(
        directory.path(),
        &[("word/document.xml", document(&paragraph("Body.")))],
    );
    let run = tempfile::tempdir().unwrap();
    let bundle = build(
        &ExtractionResult {
            text: "Body.\n\n".into(),
            method: "docx".into(),
            source_path: path.to_string_lossy().into_owned(),
            paper_hash: "0123456789abcdef".into(),
            quality_notes: Vec::new(),
        },
        run.path(),
    )
    .unwrap()
    .bundle;
    let expected = format!("{:x}", Sha256::digest(fs::read(&path).unwrap()))[..16].to_string();
    assert_eq!(bundle.origins[0].sha256.as_deref(), Some(expected.as_str()));

    // An older bundle has no such field, and other inputs never write one.
    let mut older = serde_json::to_value(&bundle).unwrap();
    older["origins"][0]
        .as_object_mut()
        .unwrap()
        .remove("sha256");
    let older: DocumentBundle = serde_json::from_value(older).unwrap();
    assert_eq!(older.origins[0].sha256, None);
    assert!(!serde_json::to_string(&older).unwrap().contains("sha256"));
}

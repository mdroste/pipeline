use super::*;

const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const DATE: &str = "2026-10-28T00:00:00Z";
const MEDIA: &[u8] = b"\x89PNG\r\n\x1a\nnot really an image but binary \x00\x01\x02";

const BODY: &str = concat!(
    r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Introduction</w:t></w:r></w:p>"#,
    r#"<w:p><w:r w:rsidR="00A1"><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">We estimate that the </w:t></w:r>"#,
    r#"<w:r><w:rPr><w:b/></w:rPr><w:t>minimum wage</w:t></w:r>"#,
    r#"<w:r><w:t xml:space="preserve"> raises employment by 2% in R&amp;D-intensive firms.</w:t></w:r></w:p>"#,
    r#"<w:p><w:r><w:rPr><w:rPrChange w:id="41" w:author="A"><w:rPr/></w:rPrChange></w:rPr>"#,
    r#"<w:t>Table 2 reports the first stage, which is strong across specifications.</w:t></w:r></w:p>"#,
    r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>"#
);

fn original_docx(body: &str, existing_comments: Option<&str>) -> Vec<u8> {
    let mut rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    );
    let mut types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
    );
    if existing_comments.is_some() {
        rels.push_str(r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>"#);
        types.push_str(r#"<Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/>"#);
    }
    rels.push_str("</Relationships>");
    types.push_str("</Types>");
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{NS}" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><w:body>{body}</w:body></w:document>"#
    );
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut parts: Vec<(&str, Vec<u8>, bool)> = vec![
        ("[Content_Types].xml", types.into_bytes(), true),
        ("_rels/.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec(), true),
        ("word/document.xml", document.into_bytes(), true),
        ("word/_rels/document.xml.rels", rels.into_bytes(), true),
        ("word/styles.xml", format!(r#"<w:styles xmlns:w="{NS}"/>"#).into_bytes(), true),
        ("word/media/image1.png", MEDIA.to_vec(), false),
    ];
    if let Some(comments) = existing_comments {
        parts.push(("word/comments.xml", comments.as_bytes().to_vec(), true));
    }
    for (name, content, compress) in parts {
        writer
            .start_file(name, if compress { deflated } else { stored })
            .unwrap();
        writer.write_all(&content).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn quoted(number: usize, quote: &str) -> ExportFinding {
    finding(
        number,
        &format!("Finding {number}"),
        "high",
        vec![evidence(quote, None, "unverified")],
    )
}

fn findings() -> Vec<ExportFinding> {
    vec![
        // Starts and ends inside space-preserving runs; covers a bold run.
        quoted(1, "the minimum wage raises employment"),
        // Starts and ends inside one plain run without xml:space.
        quoted(2, "the first stage, which is strong"),
        // Typographic hyphen and curly quotes; entity inside the range.
        quoted(3, "“R&D‑intensive firms”"),
        // Crosses the heading/body paragraph boundary.
        quoted(4, "Introduction We estimate"),
        quoted(5, "a sentence that this manuscript never contains"),
        finding(
            6,
            "No quotation",
            "low",
            vec![evidence("", Some(3), "not_checkable")],
        ),
    ]
}

fn positions(xml: &str, needle: &str) -> Vec<usize> {
    xml.match_indices(needle).map(|(index, _)| index).collect()
}

#[test]
fn comments_are_anchored_across_split_runs_without_changing_text_or_other_parts() {
    let original = original_docx(BODY, None);
    let export = export_with(findings());
    let annotated = annotate_docx(&original, &export, DATE).unwrap();
    assert_eq!((annotated.anchored, annotated.unanchored), (4, 2));

    // The existing DOCX reader sees exactly the same manuscript text.
    assert_eq!(docx_text(&annotated.bytes), docx_text(&original));

    let document = part(&annotated.bytes, "word/document.xml");
    assert_well_formed(&document);
    assert_eq!(document.matches("<w:commentRangeStart ").count(), 5);
    assert_eq!(document.matches("<w:commentRangeEnd ").count(), 5);
    assert_eq!(document.matches("<w:commentReference ").count(), 5);
    // The tracked change's w:id=41 is the largest existing id.
    for id in 42..=46 {
        let start = positions(&document, &format!(r#"<w:commentRangeStart w:id="{id}"/>"#));
        let end = positions(&document, &format!(r#"<w:commentRangeEnd w:id="{id}"/>"#));
        assert_eq!((start.len(), end.len()), (1, 1), "comment {id}");
        assert!(start[0] < end[0], "comment {id} range is reversed");
    }
    // The italic run was split twice and keeps its formatting in each part;
    // the tracked formatting change stays with the original half only.
    assert_eq!(document.matches("<w:i/>").count(), 3);
    assert_eq!(document.matches("<w:b/>").count(), 1);
    assert_eq!(document.matches("<w:rPrChange ").count(), 1);
    assert!(document.contains(r#"<w:t xml:space="preserve">Table 2 reports </w:t>"#));
    assert!(document.contains(r#"<w:commentRangeStart w:id="43"/><w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">the </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>minimum wage</w:t></w:r>"#));
    assert!(document.contains(r#"<w:t xml:space="preserve">R&amp;D-intensive firms</w:t></w:r><w:commentRangeEnd w:id="45"/>"#));

    let comments = part(&annotated.bytes, "word/comments.xml");
    assert_well_formed(&comments);
    assert_eq!(comments.matches("<w:comment ").count(), 5);
    let summary = comments.split("</w:comment>").next().unwrap();
    assert!(summary.contains(r#"w:id="42""#));
    assert!(summary.contains("2 findings could not be attached"));
    assert!(summary.contains("#5 · High · Finding 5"));
    assert!(summary.contains("The quoted text was not found in this file."));
    assert!(summary.contains("#6 · Low · No quotation"));
    assert!(summary.contains("No quotation was given."));
    assert!(comments.contains("Problem: Problem for finding 1."));
    assert!(comments.contains("Evidence: Not checked"));
    assert!(comments.contains(r#"w:author="Pipeline Review" w:date="2026-10-28T00:00:00Z""#));

    let types = part(&annotated.bytes, "[Content_Types].xml");
    assert_well_formed(&types);
    assert_eq!(types.matches(r#"PartName="/word/comments.xml""#).count(), 1);
    let rels = part(&annotated.bytes, "word/_rels/document.xml.rels");
    assert!(rels.contains(r#"Target="comments.xml""#));
    assert!(rels.contains(r#"Target="styles.xml""#));

    for name in ["word/media/image1.png", "word/styles.xml", "_rels/.rels"] {
        assert_eq!(
            zip_raw(&annotated.bytes, name),
            zip_raw(&original, name),
            "{name}"
        );
    }
    let names: Vec<String> = zip_parts(&annotated.bytes)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names.last().map(String::as_str), Some("word/comments.xml"));
}

#[test]
fn existing_comments_are_kept_and_new_ids_do_not_collide() {
    let body = BODY.replacen(
        "<w:r><w:t>Introduction</w:t></w:r>",
        r#"<w:commentRangeStart w:id="7"/><w:r><w:t>Introduction</w:t></w:r><w:commentRangeEnd w:id="7"/><w:r><w:commentReference w:id="7"/></w:r>"#,
        1,
    );
    let existing = format!(
        r#"<?xml version="1.0"?><w:comments xmlns:w="{NS}"><w:comment w:id="7" w:author="Coauthor"><w:p><w:r><w:t>Keep me</w:t></w:r></w:p></w:comment></w:comments>"#
    );
    let original = original_docx(&body, Some(&existing));
    let export = export_with(vec![quoted(1, "the first stage, which is strong")]);
    let annotated = annotate_docx(&original, &export, DATE).unwrap();
    assert_eq!((annotated.anchored, annotated.unanchored), (1, 0));
    let comments = part(&annotated.bytes, "word/comments.xml");
    assert_well_formed(&comments);
    assert!(comments.contains("Keep me"));
    assert!(comments.contains(r#"<w:comment w:id="42""#), "{comments}");
    for name in ["[Content_Types].xml", "word/_rels/document.xml.rels"] {
        assert_eq!(
            zip_raw(&annotated.bytes, name),
            zip_raw(&original, name),
            "{name}"
        );
    }
    assert_eq!(docx_text(&annotated.bytes), docx_text(&original));
}

#[test]
fn every_finding_unanchored_still_produces_one_summary_comment() {
    let original = original_docx(BODY, None);
    let export = export_with(vec![quoted(1, "nothing like this appears anywhere")]);
    let annotated = annotate_docx(&original, &export, DATE).unwrap();
    assert_eq!((annotated.anchored, annotated.unanchored), (0, 1));
    let document = part(&annotated.bytes, "word/document.xml");
    assert!(document.contains(concat!(
        r#"<w:commentRangeStart w:id="42"/><w:r><w:t>Introduction</w:t></w:r>"#,
        r#"<w:commentRangeEnd w:id="42"/><w:r><w:commentReference w:id="42"/></w:r>"#
    )));
    assert_eq!(docx_text(&annotated.bytes), docx_text(&original));
}

#[test]
fn rejects_documents_it_cannot_annotate_safely() {
    let export = export_with(vec![quoted(1, "the first stage, which is strong")]);
    assert!(annotate_docx(b"not a zip", &export, DATE).is_err());
    let strict = original_docx(BODY, None);
    let strict = {
        let mut parts = zip_parts(&strict);
        for (name, content) in &mut parts {
            if name == "word/document.xml" {
                *content = String::from_utf8(content.clone())
                    .unwrap()
                    .replace(NS, "http://purl.oclc.org/ooxml/wordprocessingml/main")
                    .into_bytes();
            }
        }
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, content) in parts {
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    };
    assert!(annotate_docx(&strict, &export, DATE)
        .unwrap_err()
        .contains("Strict Open XML"));
    let empty = export_with(Vec::new());
    assert!(annotate_docx(&original_docx(BODY, None), &empty, DATE)
        .unwrap_err()
        .contains("no individual findings"));
}

#[test]
fn fresh_word_report_is_a_valid_package_with_escaped_content() {
    let mut export = export_with(findings());
    export.findings[0].title = "Bias in R&D <firms> \u{1}".to_string();
    export.findings[1].problem.clear();
    export.findings[1].body =
        "Intro paragraph.\n\n- first point\n- second with `code`\n\n| A | B |\n| - | - |\n| 1 | 2 |".to_string();
    let bytes = render_docx_report(&export).unwrap();
    for (name, content) in zip_parts(&bytes) {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            assert_well_formed(std::str::from_utf8(&content).unwrap());
        }
    }
    let text = docx_text(&bytes);
    assert!(text.contains("Review of Minimum Wages & Employment"));
    assert!(text.contains("#1. Bias in R&D <firms>"));
    assert!(!text.contains('\u{1}'));
    assert!(text.contains("“the minimum wage raises employment”"));
    assert!(text.contains("Not checked"));
    assert!(text.contains("Second with code") || text.contains("second with code"));
    assert!(text.contains("| A | B |"));
    assert!(text.contains("Quality and limitations"));
    assert!(text.contains("Equations were extracted as text."));

    let mut plain = export_with(Vec::new());
    plain.report_markdown = "# Referee Report\n\n## Overall\n\nA *careful* paper.".to_string();
    let text = docx_text(&render_docx_report(&plain).unwrap());
    assert!(text.contains("# Referee Report"));
    assert!(text.contains("A careful paper."));
}

#[test]
fn ranges_cross_hyperlinks_and_extend_around_equations() {
    let body = concat!(
        r#"<w:p><w:r><w:t xml:space="preserve">As shown by </w:t></w:r>"#,
        r#"<w:hyperlink w:anchor="ref1"><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>Card and Krueger</w:t></w:r></w:hyperlink>"#,
        r#"<w:r><w:t xml:space="preserve"> the effect </w:t></w:r><m:oMath><m:r><m:t>β</m:t></m:r></m:oMath>"#,
        r#"<w:r><w:t xml:space="preserve"> is positive in every sample.</w:t></w:r></w:p>"#
    );
    let original = original_docx(body, None);
    let export = export_with(vec![
        quoted(1, "Krueger the effect β is positive"),
        quoted(2, "As shown by Card and Krueger the effect β"),
    ]);
    let annotated = annotate_docx(&original, &export, DATE).unwrap();
    assert_eq!((annotated.anchored, annotated.unanchored), (2, 0));
    let document = part(&annotated.bytes, "word/document.xml");
    assert_well_formed(&document);
    assert_eq!(docx_text(&annotated.bytes), docx_text(&original));
    // The first range starts inside the hyperlink's run, which keeps its
    // underline on both halves; the second ends after the whole equation.
    assert!(document.contains(concat!(
        r#"<w:t xml:space="preserve">Card and </w:t></w:r><w:commentRangeStart w:id="1"/>"#,
        r#"<w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t xml:space="preserve">Krueger</w:t></w:r></w:hyperlink>"#
    )));
    assert!(document.contains(
        r#"</m:oMath><w:commentRangeEnd w:id="2"/><w:r><w:commentReference w:id="2"/></w:r>"#
    ));
}

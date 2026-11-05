use super::*;
use lopdf::Document;

const DATE: &str = "2026-10-28";

struct FixturePage<'a> {
    lines: &'a [&'a str],
    rotate: Option<i64>,
    existing_annotation: bool,
}

/// A classic-xref PDF with Helvetica text lines starting at (72, 720).
fn pdf_fixture(pages: &[FixturePage<'_>]) -> Vec<u8> {
    let font_id = 3 + pages.len() * 2;
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} /MediaBox [0 0 612 792] >>",
            (0..pages.len())
                .map(|index| format!("{} 0 R", 3 + index * 2))
                .collect::<Vec<_>>()
                .join(" "),
            pages.len()
        ),
    ];
    let mut extra = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let mut stream = String::from("BT /F1 12 Tf 14 TL 72 720 Td");
        for line in page.lines {
            stream.push_str(&format!(" ({line}) Tj T*"));
        }
        stream.push_str(" ET\n");
        let mut dictionary = format!(
            "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 {font_id} 0 R >> >> /Contents {} 0 R",
            4 + index * 2
        );
        if let Some(rotate) = page.rotate {
            dictionary.push_str(&format!(" /Rotate {rotate}"));
        }
        if page.existing_annotation {
            let array_id = font_id + 1 + extra.len();
            dictionary.push_str(&format!(" /Annots {array_id} 0 R"));
            extra.push(format!("[{} 0 R]", array_id + 1));
            extra.push(
                "<< /Type /Annot /Subtype /Text /Rect [500 700 520 720] /Contents (Coauthor note) >>"
                    .to_string(),
            );
        }
        dictionary.push_str(" >>");
        objects.push(dictionary);
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}endstream",
            stream.len()
        ));
    }
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
    objects.extend(extra);
    let mut pdf = "%PDF-1.4\n%\u{e2}\u{e3}\u{cf}\u{d3}\n".to_string();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    pdf.into_bytes()
}

fn word(text: &str, x: f32, line: usize) -> pdf_words::Word {
    let width = text.len() as f32 * 6.0;
    let top = 63.384 + 14.0 * (line as f32 - 1.0);
    pdf_words::Word {
        text: text.to_string(),
        x_min: x,
        y_min: top,
        x_max: x + width,
        y_max: top + 11.1,
        line,
    }
}

fn page_words(number: u32, lines: &[&str]) -> PageWords {
    let mut words = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let mut x = 72.0;
        for text in line.split_whitespace() {
            words.push(word(text, x, index + 1));
            x += text.len() as f32 * 6.0 + 3.3;
        }
    }
    PageWords { number, words }
}

fn annotations(document: &Document, page: u32) -> Vec<lopdf::Dictionary> {
    let id = document.get_pages()[&page];
    document
        .get_page_annotations(id)
        .unwrap()
        .into_iter()
        .cloned()
        .collect()
}

fn subtype(annotation: &lopdf::Dictionary) -> String {
    String::from_utf8_lossy(annotation.get(b"Subtype").unwrap().as_name().unwrap()).into_owned()
}

fn contents(annotation: &lopdf::Dictionary) -> String {
    lopdf::decode_text_string(annotation.get(b"Contents").unwrap()).unwrap()
}

fn rect(annotation: &lopdf::Dictionary) -> Vec<f32> {
    annotation
        .get(b"Rect")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_float().unwrap())
        .collect()
}

const PAGE_ONE: &[&str] = &[
    "The estimated effect is large (see Table 2).",
    "We find no evidence of pre-trends in",
    "the event-study specification.",
];
const PAGE_TWO: &[&str] = &["Robustness checks use a placebo cutoff at age 64."];

fn fixture_findings() -> Vec<ExportFinding> {
    vec![
        finding(
            1,
            "Effect size",
            "high",
            vec![evidence(
                "no evidence of pre‑trends in the event-study",
                Some(1),
                "verified",
            )],
        ),
        finding(
            2,
            "Wrong page",
            "medium",
            vec![evidence("placebo cutoff at age 64", Some(1), "unverified")],
        ),
        finding(
            3,
            "Missing quote",
            "low",
            vec![evidence(
                "this sentence is not in the paper",
                Some(2),
                "not_found",
            )],
        ),
        finding(4, "General", "", vec![]),
    ]
}

#[test]
fn annotations_are_an_incremental_update_that_keeps_original_bytes() {
    let original = pdf_fixture(&[
        FixturePage {
            lines: PAGE_ONE,
            rotate: None,
            existing_annotation: true,
        },
        FixturePage {
            lines: PAGE_TWO,
            rotate: None,
            existing_annotation: false,
        },
    ]);
    let words = vec![page_words(1, PAGE_ONE), page_words(2, PAGE_TWO)];
    let export = export_with(fixture_findings());
    let annotated = annotate_pdf(&original, &words, &export, DATE).unwrap();
    assert_eq!((annotated.anchored, annotated.unanchored), (2, 2));
    assert!(annotated.bytes.starts_with(&original));
    assert!(annotated.bytes.len() > original.len());

    let document = Document::load_mem(&annotated.bytes).unwrap();
    let prior_xref = std::str::from_utf8(&original)
        .unwrap()
        .rsplit("startxref\n")
        .next()
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_string();
    let update = String::from_utf8_lossy(&annotated.bytes[original.len()..]).into_owned();
    assert!(update.contains(&format!("/Prev {prior_xref}")), "{update}");
    assert!(!update.contains("%PDF-"));

    let first = annotations(&document, 1);
    let kinds: Vec<String> = first.iter().map(subtype).collect();
    assert_eq!(kinds, vec!["Text", "Highlight", "Popup", "Text", "Popup"]);
    assert_eq!(contents(&first[0]), "Coauthor note");
    let highlight = &first[1];
    assert!(contents(highlight)
        .starts_with("#1 · High · Effect size\n\nProblem: Problem for finding 1."));
    assert!(contents(highlight).contains("Evidence: Quote found · Page 1"));
    let quads = highlight.get(b"QuadPoints").unwrap().as_array().unwrap();
    assert_eq!(quads.len(), 16, "two lines, one quad each");
    // Lines 2 and 3 of the synthetic layout, converted to bottom-left origin.
    let bounds = rect(highlight);
    assert!(
        (bounds[1] - (792.0 - (63.384 + 28.0 + 11.1))).abs() < 0.05,
        "{bounds:?}"
    );
    assert!(
        (bounds[3] - (792.0 - (63.384 + 14.0))).abs() < 0.05,
        "{bounds:?}"
    );
    let popup_id = highlight.get(b"Popup").unwrap().as_reference().unwrap();
    let popup = document.get_dictionary(popup_id).unwrap();
    assert_eq!(subtype(popup), "Popup");
    let general = &first[3];
    assert!(contents(general).contains("No quotation was given for this finding."));

    let second = annotations(&document, 2);
    let kinds: Vec<String> = second.iter().map(subtype).collect();
    assert_eq!(kinds, vec!["Highlight", "Popup", "Text", "Popup"]);
    assert!(contents(&second[0])
        .contains("The finding cites page 1; Pipeline found the quotation on page 2."));
    assert!(contents(&second[2]).contains("could not find the quoted text"));
    assert!(contents(&second[2]).contains("this sentence is not in the paper"));
}

#[test]
fn modern_cross_reference_streams_are_extended_in_place() {
    let classic = pdf_fixture(&[FixturePage {
        lines: PAGE_ONE,
        rotate: None,
        existing_annotation: false,
    }]);
    let mut modern = Vec::new();
    Document::load_mem(&classic)
        .unwrap()
        .save_modern(&mut modern)
        .unwrap();
    assert!(!String::from_utf8_lossy(&modern).contains("\nxref\n"));
    let export = export_with(vec![fixture_findings().remove(0)]);
    let annotated = annotate_pdf(&modern, &[page_words(1, PAGE_ONE)], &export, DATE).unwrap();
    assert!(annotated.bytes.starts_with(&modern));
    let document = Document::load_mem(&annotated.bytes).unwrap();
    let kinds: Vec<String> = annotations(&document, 1).iter().map(subtype).collect();
    assert_eq!(kinds, vec!["Highlight", "Popup"]);
}

#[test]
fn rejects_unreadable_pdfs_and_reviews_without_findings() {
    let export = export_with(fixture_findings());
    assert!(annotate_pdf(b"%PDF-1.4 broken", &[], &export, DATE).is_err());
    let original = pdf_fixture(&[FixturePage {
        lines: PAGE_TWO,
        rotate: None,
        existing_annotation: false,
    }]);
    assert!(annotate_pdf(&original, &[], &export_with(Vec::new()), DATE)
        .unwrap_err()
        .contains("no individual findings"));
}

#[test]
fn poppler_word_boxes_place_highlights_on_upright_and_rotated_pages() {
    if crate::deps::resolve_command("pdftotext").is_none() {
        eprintln!("skipping: pdftotext is not installed");
        return;
    }
    let original = pdf_fixture(&[
        FixturePage {
            lines: PAGE_ONE,
            rotate: None,
            existing_annotation: false,
        },
        FixturePage {
            lines: PAGE_ONE,
            rotate: Some(90),
            existing_annotation: false,
        },
    ]);
    let words = pdftotext_bbox_layout(&original).unwrap();
    assert_eq!(words.len(), 2);
    let quote = "The estimated effect is large";
    let export = export_with(vec![
        finding(
            1,
            "Upright",
            "high",
            vec![evidence(quote, Some(1), "verified")],
        ),
        finding(
            2,
            "Rotated",
            "high",
            vec![evidence(quote, Some(2), "verified")],
        ),
    ]);
    let annotated = annotate_pdf(&original, &words, &export, DATE).unwrap();
    assert_eq!(annotated.anchored, 2);
    let document = Document::load_mem(&annotated.bytes).unwrap();
    for page in [1, 2] {
        let highlight = annotations(&document, page)
            .into_iter()
            .find(|annotation| subtype(annotation) == "Highlight")
            .unwrap();
        let bounds = rect(&highlight);
        // Helvetica 12 pt text drawn at baseline (72, 720).
        assert!((bounds[0] - 72.0).abs() < 1.0, "page {page}: {bounds:?}");
        assert!(
            bounds[1] > 714.0 && bounds[1] < 720.0,
            "page {page}: {bounds:?}"
        );
        assert!(
            bounds[3] > 726.0 && bounds[3] < 732.0,
            "page {page}: {bounds:?}"
        );
        assert!(
            bounds[2] > 200.0 && bounds[2] < 240.0,
            "page {page}: {bounds:?}"
        );
    }

    if let Some(pdfinfo) = crate::deps::resolve_command("pdfinfo") {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("annotated.pdf");
        std::fs::write(&path, &annotated.bytes).unwrap();
        let output = pdfinfo.command([path.as_os_str()]).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("Pages:"));
    }
    let reread = pdftotext_bbox_layout(&annotated.bytes).unwrap();
    assert_eq!(reread.len(), 2);
}

#[test]
fn out_of_range_cited_pages_fall_back_to_a_note_on_the_first_page() {
    let original = pdf_fixture(&[FixturePage {
        lines: PAGE_ONE,
        rotate: None,
        existing_annotation: false,
    }]);
    let export = export_with(vec![finding(
        1,
        "Bad page",
        "low",
        vec![evidence(
            "absent words that cannot match",
            Some(99),
            "unverified",
        )],
    )]);
    let annotated = annotate_pdf(&original, &[page_words(1, PAGE_ONE)], &export, DATE).unwrap();
    let document = Document::load_mem(&annotated.bytes).unwrap();
    let kinds: Vec<String> = annotations(&document, 1).iter().map(subtype).collect();
    assert_eq!(kinds, vec!["Text", "Popup"]);
}

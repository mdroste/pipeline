//! A fresh, minimal Word report for any Review input.

use super::docx_markdown::{Body, Run};
use super::docx_scan::WORD_MAIN_NS;
use super::model::{ExportFinding, ReportExport, Verification};
use super::text::escape_xml;
use std::io::Write;

const CONTENT_TYPES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    "\n",
    r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
    r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
    r#"<Default Extension="xml" ContentType="application/xml"/>"#,
    r#"<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
    r#"<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>"#,
    r#"<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>"#,
    "</Types>"
);

const PACKAGE_RELS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    "\n",
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>"#,
    r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>"#,
    "</Relationships>"
);

const DOCUMENT_RELS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    "\n",
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    "</Relationships>"
);

fn styles() -> String {
    let heading = |id: u32, size: u32, before: u32| {
        format!(
            concat!(
                r#"<w:style w:type="paragraph" w:styleId="Heading{id}"><w:name w:val="heading {id}"/>"#,
                r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:uiPriority w:val="9"/><w:qFormat/>"#,
                r#"<w:pPr><w:keepNext/><w:spacing w:before="{before}" w:after="120"/><w:outlineLvl w:val="{level}"/></w:pPr>"#,
                r#"<w:rPr><w:b/><w:color w:val="18364D"/><w:sz w:val="{size}"/><w:szCs w:val="{size}"/></w:rPr></w:style>"#
            ),
            id = id,
            level = id - 1,
            size = size,
            before = before
        )
    };
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            "\n",
            r#"<w:styles xmlns:w="{ns}"><w:docDefaults><w:rPrDefault><w:rPr>"#,
            r#"<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="Calibri" w:cs="Calibri"/>"#,
            r#"<w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="en-US"/></w:rPr></w:rPrDefault>"#,
            r#"<w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="276" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>"#,
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>"#,
            r#"<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/>"#,
            r#"<w:next w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="80"/></w:pPr>"#,
            r#"<w:rPr><w:b/><w:sz w:val="40"/><w:szCs w:val="40"/></w:rPr></w:style>"#,
            "{h1}{h2}{h3}",
            r#"<w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:basedOn w:val="Normal"/><w:qFormat/>"#,
            r#"<w:pPr><w:pBdr><w:left w:val="single" w:sz="12" w:space="8" w:color="9CABB4"/></w:pBdr>"#,
            r#"<w:ind w:left="360"/></w:pPr><w:rPr><w:i/><w:color w:val="4F5961"/></w:rPr></w:style>"#,
            r#"<w:style w:type="paragraph" w:styleId="Code"><w:name w:val="Code"/><w:basedOn w:val="Normal"/>"#,
            r#"<w:pPr><w:spacing w:after="0"/></w:pPr><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Consolas"/>"#,
            r#"<w:sz w:val="18"/><w:szCs w:val="18"/></w:rPr></w:style>"#,
            r#"<w:style w:type="paragraph" w:styleId="ReportDetails"><w:name w:val="Report Details"/><w:basedOn w:val="Normal"/>"#,
            r#"<w:pPr><w:spacing w:after="40"/></w:pPr><w:rPr><w:color w:val="66707A"/><w:sz w:val="18"/><w:szCs w:val="18"/></w:rPr></w:style>"#,
            r#"<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/><w:basedOn w:val="Normal"/>"#,
            r#"<w:qFormat/><w:pPr><w:spacing w:after="60"/></w:pPr></w:style>"#,
            "</w:styles>"
        ),
        ns = WORD_MAIN_NS,
        h1 = heading(1, 30, 360),
        h2 = heading(2, 26, 300),
        h3 = heading(3, 23, 240)
    )
}

fn severity_color(severity: &str) -> Option<&'static str> {
    match severity {
        "high" => Some("B42318"),
        "medium" => Some("B54708"),
        "low" => Some("175CD3"),
        _ => None,
    }
}

fn verification_color(verification: Verification) -> &'static str {
    match verification {
        Verification::Verified | Verification::VerifiedWithNormalization => "067647",
        Verification::PartiallyVerified => "B54708",
        Verification::NotFound => "B42318",
        Verification::NotCheckable | Verification::Unchecked => "66707A",
    }
}

fn write_finding(body: &mut Body, finding: &ExportFinding) {
    body.text(
        Some("Heading3"),
        &format!("#{}. {}", finding.number, finding.title),
    );
    let mut facts = vec![Run {
        text: format!("Severity: {}", finding.severity_label()),
        bold: true,
        color: severity_color(&finding.severity),
        ..Run::default()
    }];
    if !finding.confidence.is_empty() {
        facts.push(Run::plain(format!(" · Confidence: {}", finding.confidence)));
    }
    body.paragraph(Some("ReportDetails"), 0, &facts);
    if finding.problem.is_empty() {
        body.markdown(&finding.body, 4);
    } else {
        for (label, value) in [
            ("Problem.", &finding.problem),
            ("Consequence.", &finding.consequence),
            ("Recommended action.", &finding.recommended_action),
        ] {
            if !value.is_empty() {
                body.markdown(&format!("**{label}** {value}"), 4);
            }
        }
    }
    for evidence in &finding.evidence {
        if !evidence.quote.is_empty() {
            body.text(Some("Quote"), &format!("“{}”", evidence.quote));
        }
        let mut details = vec![Run {
            text: evidence.verification.label().to_string(),
            bold: true,
            color: Some(verification_color(evidence.verification)),
            ..Run::default()
        }];
        if !evidence.locator.is_empty() {
            details.push(Run::plain(format!(" · {}", evidence.locator)));
        }
        body.paragraph(Some("ReportDetails"), 0, &details);
    }
}

fn write_quality(body: &mut Body, export: &ReportExport) {
    let quality = &export.quality;
    body.text(Some("Heading1"), "Quality and limitations");
    if !export.meta.status.is_empty() {
        body.text(None, &format!("Final status: {}", export.meta.status));
    }
    if quality.schema_version > 0 {
        body.text(
            None,
            &format!(
                "Evidence: {} verified, {} partially verified, {} unverified, {} not checkable; {} item(s) for manual review.",
                quality.verified_evidence,
                quality.partially_verified_evidence,
                quality.unverified_evidence,
                quality.not_checkable_evidence,
                quality.manual_review_items
            ),
        );
    }
    if quality.limitations.is_empty() {
        body.text(None, "No limitations were recorded for this run.");
    } else {
        for limitation in &quality.limitations {
            body.paragraph(
                Some("ListParagraph"),
                1,
                &[Run::plain(format!("•\t{limitation}"))],
            );
        }
    }
}

fn document_body(export: &ReportExport) -> String {
    let mut body = Body::default();
    let meta = &export.meta;
    body.text(Some("Title"), &meta.title);
    let details = [
        ("Reviewed file", &meta.input_name),
        ("Review", &meta.workflow),
        ("Date", &meta.date),
        ("Status", &meta.status),
    ];
    for (label, value) in details {
        if !value.is_empty() {
            body.paragraph(
                Some("ReportDetails"),
                0,
                &[Run::bold(format!("{label}: ")), Run::plain(value.as_str())],
            );
        }
    }
    body.paragraph(
        Some("ReportDetails"),
        0,
        &[Run {
            text: "Generated by Pipeline. Findings are model-written. Each evidence label states whether Pipeline found the quotation in the reviewed document.".to_string(),
            italic: true,
            ..Run::default()
        }],
    );
    if export.findings.is_empty() {
        body.markdown(&export.report_markdown, 1);
        if !export
            .report_markdown
            .contains("## Quality and limitations")
        {
            write_quality(&mut body, export);
        }
    } else {
        body.text(Some("Heading1"), "Findings");
        let mut category = None;
        for finding in &export.findings {
            if category != Some(finding.category.as_str()) {
                category = Some(finding.category.as_str());
                if !finding.category.is_empty() {
                    body.text(Some("Heading2"), &finding.category);
                }
            }
            write_finding(&mut body, finding);
        }
        write_quality(&mut body, export);
    }
    body.into_xml()
}

/// Build a self-contained `.docx` containing the report and its findings.
pub fn render_docx_report(export: &ReportExport) -> Result<Vec<u8>, String> {
    let document = format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            "\n",
            r#"<w:document xmlns:w="{ns}"><w:body>{body}<w:sectPr>"#,
            r#"<w:pgSz w:w="12240" w:h="15840"/>"#,
            r#"<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#,
            "</w:sectPr></w:body></w:document>"
        ),
        ns = WORD_MAIN_NS,
        body = document_body(export)
    );
    let core = format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            "\n",
            r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" "#,
            r#"xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
            "<dc:title>{title}</dc:title><dc:creator>Pipeline</dc:creator></cp:coreProperties>"
        ),
        title = escape_xml(&export.meta.title)
    );
    let styles = styles();
    let parts: [(&str, &str); 6] = [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", PACKAGE_RELS),
        ("word/document.xml", &document),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/styles.xml", &styles),
        ("docProps/core.xml", &core),
    ];
    let failed = |error: zip::result::ZipError| format!("Could not build the Word file: {error}");
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, content) in parts {
        writer.start_file(name, options).map_err(failed)?;
        writer
            .write_all(content.as_bytes())
            .map_err(|error| format!("Could not build the Word file: {error}"))?;
    }
    Ok(writer.finish().map_err(failed)?.into_inner())
}

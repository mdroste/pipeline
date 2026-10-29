//! Word comments anchored in a copy of the reviewed `.docx`.
//!
//! Each finding becomes a comment on the passage its evidence quotes. Runs
//! are split only where a quotation starts or ends inside them, and the split
//! halves keep the run's formatting. Findings whose quotations cannot be
//! placed are listed in one summary comment on the first paragraph. Only
//! `word/document.xml`, the comments part, and (when they need a new entry)
//! the relationships and content-type parts change; every other package part
//! is copied byte-for-byte without recompression.

use super::docx_scan::{numeric_id, scan_document, CharSource, ScannedDocument, WORD_MAIN_NS};
use super::model::{evidence_line, ExportEvidence, ExportFinding, ReportExport};
use super::quote_match::{Haystack, Needle};
use super::text::{escape_xml, markdown_to_plain, truncate_chars};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::{Read, Write};

const MAIN_PART: &str = "word/document.xml";
const MAIN_RELS: &str = "word/_rels/document.xml.rels";
const CONTENT_TYPES: &str = "[Content_Types].xml";
const COMMENTS_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const COMMENTS_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml";
const RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const MAX_PART_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;

/// A commented copy of the reviewed document.
#[derive(Debug)]
pub struct DocxAnnotation {
    pub bytes: Vec<u8>,
    /// Findings with at least one comment on a quoted passage.
    pub anchored: usize,
    /// Findings listed in the summary comment instead.
    pub unanchored: usize,
}

struct PlannedComment {
    paragraphs: Vec<String>,
    /// Flat character range `[start, end)` in the scanned body.
    range: (usize, usize),
}

struct Edit {
    at: usize,
    /// Inserts at one offset: range ends, then range starts, then tag rewrites.
    order: u8,
    /// End of the replaced span; equals `at` for an insertion.
    until: usize,
    text: String,
}

/// Add one Word comment per placed finding to a copy of `original`.
/// `date` is an ISO 8601 timestamp recorded on each comment.
pub fn annotate_docx(
    original: &[u8],
    export: &ReportExport,
    date: &str,
) -> Result<DocxAnnotation, String> {
    if export.findings.is_empty() {
        return Err("This Review has no individual findings to turn into Word comments. Export it as Word or HTML instead.".into());
    }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(original))
        .map_err(|error| format!("The reviewed file is not a readable Word document: {error}"))?;
    if archive.len() > MAX_ENTRIES {
        return Err("The Word document contains too many parts to annotate safely".into());
    }
    let document_xml = read_part(&mut archive, MAIN_PART)?
        .ok_or("The Word document has no main text part (word/document.xml)")?;
    let content_types = read_part(&mut archive, CONTENT_TYPES)?
        .ok_or("The Word document has no content-type list")?;
    let main_rels = read_part(&mut archive, MAIN_RELS)?;
    let relationships = match main_rels.as_deref() {
        Some(xml) => parse_relationships(xml)?,
        None => Vec::new(),
    };
    let existing_comments_part = relationships
        .iter()
        .find(|relationship| relationship.kind == COMMENTS_REL_TYPE && !relationship.external)
        .map(|relationship| resolve_part_name(&relationship.target));
    let existing_comments = match existing_comments_part.as_deref() {
        Some(name) => read_part(&mut archive, name)?,
        None => None,
    };

    let scanned = scan_document(&document_xml)?;
    let mut next_id = scanned.max_id + 1;
    if let Some(xml) = existing_comments.as_deref() {
        next_id = next_id.max(comments_max_id(xml)? + 1);
    }

    let (planned, anchored, unanchored) = plan_comments(&scanned, &export.findings)?;
    let mut edits = Vec::new();
    let mut comments_xml = String::new();
    for comment in &planned {
        let id = next_id;
        next_id += 1;
        comment_edits(&scanned, &document_xml, comment.range, id, &mut edits)?;
        comments_xml.push_str(&comment_element(id, &comment.paragraphs, date));
    }
    let annotated_xml = apply_edits(&document_xml, edits)?;
    let rescanned = scan_document(&annotated_xml)
        .map_err(|error| format!("Pipeline produced an invalid Word document: {error}"))?;
    if rescanned.visible_text() != scanned.visible_text() {
        return Err("Adding comments would change the document's text; no file was written".into());
    }

    let mut replacements: Vec<(String, String)> = vec![(MAIN_PART.to_string(), annotated_xml)];
    let mut additions: Vec<(String, String)> = Vec::new();
    match (existing_comments_part, existing_comments) {
        (Some(name), Some(xml)) => replacements.push((name, append_comments(&xml, &comments_xml)?)),
        (Some(_), None) => {
            return Err("The Word document refers to a comments part that is missing".into())
        }
        (None, _) => {
            additions.push((
                "word/comments.xml".to_string(),
                new_comments_part(&comments_xml),
            ));
            let rels = add_comments_relationship(main_rels.as_deref(), &relationships)?;
            if main_rels.is_some() {
                replacements.push((MAIN_RELS.to_string(), rels));
            } else {
                additions.push((MAIN_RELS.to_string(), rels));
            }
            if let Some(updated) = add_comments_content_type(&content_types)? {
                replacements.push((CONTENT_TYPES.to_string(), updated));
            }
        }
    }
    let bytes = rewrite_package(&mut archive, &replacements, &additions)?;
    Ok(DocxAnnotation {
        bytes,
        anchored,
        unanchored,
    })
}

fn read_part<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Option<String>, String> {
    let entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Could not open {name} in the Word document: {error}"
            ))
        }
    };
    if entry.size() > MAX_PART_BYTES {
        return Err(format!("{name} is too large to annotate safely"));
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_PART_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read {name}: {error}"))?;
    if bytes.len() as u64 > MAX_PART_BYTES {
        return Err(format!("{name} is too large to annotate safely"));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{name} is not UTF-8 XML; Pipeline cannot annotate it"))
}

/// Decide which passage each finding comments on. Returns the comments in
/// creation order (summary first), and the anchored/unanchored counts.
fn plan_comments(
    scanned: &ScannedDocument,
    findings: &[ExportFinding],
) -> Result<(Vec<PlannedComment>, usize, usize), String> {
    let haystack = Haystack::new(&scanned.chars);
    let mut placed = Vec::new();
    let mut missing: Vec<&ExportFinding> = Vec::new();
    for finding in findings {
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for evidence in finding.evidence.iter().filter(|e| !e.quote.is_empty()) {
            let Some(found) = Needle::new(&evidence.quote).and_then(|n| haystack.find_best(&n))
            else {
                continue;
            };
            let range = (found.start, found.end);
            if ranges.contains(&range) {
                continue;
            }
            let paragraphs = if ranges.is_empty() {
                finding.note_paragraphs(Some(evidence))
            } else {
                vec![
                    format!("Also cited for {}", finding.heading_line()),
                    evidence_line(evidence),
                ]
            };
            ranges.push(range);
            placed.push(PlannedComment { paragraphs, range });
        }
        if ranges.is_empty() {
            missing.push(finding);
        }
    }
    let anchored = findings.len() - missing.len();
    let mut planned = Vec::with_capacity(placed.len() + 1);
    if !missing.is_empty() {
        let range = scanned
            .first_paragraph
            .map(|(first, last)| (first, last + 1))
            .ok_or("This Word document has no body text to attach comments to")?;
        planned.push(PlannedComment {
            paragraphs: summary_paragraphs(&missing),
            range,
        });
    }
    planned.extend(placed);
    Ok((planned, anchored, missing.len()))
}

fn summary_paragraphs(missing: &[&ExportFinding]) -> Vec<String> {
    let mut paragraphs = vec![format!(
        "Pipeline Review: {} finding{} could not be attached to a quoted passage in this document.",
        missing.len(),
        if missing.len() == 1 { "" } else { "s" }
    )];
    for finding in missing {
        let mut lines = vec![finding.heading_line()];
        let summary = if finding.problem.is_empty() {
            markdown_to_plain(&finding.body)
        } else {
            finding.problem.clone()
        };
        if !summary.is_empty() {
            lines.push(truncate_chars(&summary, 600));
        }
        let quoted: Vec<&ExportEvidence> = finding
            .evidence
            .iter()
            .filter(|e| !e.quote.is_empty())
            .collect();
        if quoted.is_empty() {
            lines.push("No quotation was given.".to_string());
        } else {
            lines.push("The quoted text was not found in this file.".to_string());
        }
        for evidence in finding.evidence.iter().take(5) {
            let mut line = evidence_line(evidence);
            if !evidence.quote.is_empty() {
                line.push_str(&format!(" · “{}”", truncate_chars(&evidence.quote, 160)));
            }
            lines.push(line);
        }
        paragraphs.push(lines.join("\n"));
    }
    paragraphs
}

fn start_mark(id: u64) -> String {
    format!(r#"<w:commentRangeStart w:id="{id}"/>"#)
}

fn end_mark(id: u64) -> String {
    format!(r#"<w:commentRangeEnd w:id="{id}"/><w:r><w:commentReference w:id="{id}"/></w:r>"#)
}

/// Copy a run's formatting for the far side of a split. Tracked formatting
/// changes carry revision ids, so they stay with the original half only.
fn run_properties(xml: &str, scanned: &ScannedDocument, run: usize) -> String {
    let Some((start, end)) = scanned.runs[run].properties else {
        return String::new();
    };
    let mut properties = xml[start..end].to_string();
    while let Some(begin) = properties.find("<w:rPrChange") {
        let rest = &properties[begin..];
        let close = rest
            .find("</w:rPrChange>")
            .map(|index| index + "</w:rPrChange>".len())
            .or_else(|| rest.find("/>").map(|index| index + 2));
        match close {
            Some(length) => properties.replace_range(begin..begin + length, ""),
            None => break,
        }
    }
    properties
}

fn split_run(xml: &str, scanned: &ScannedDocument, run: usize, mark: &str) -> String {
    format!("</w:r>{mark}<w:r>{}", run_properties(xml, scanned, run))
}

fn split_text(xml: &str, scanned: &ScannedDocument, run: usize, mark: &str) -> String {
    format!(
        r#"</w:t></w:r>{mark}<w:r>{}<w:t xml:space="preserve">"#,
        run_properties(xml, scanned, run)
    )
}

/// A split inside a `w:t` leaves spaces at the cut edge; make sure Word
/// keeps them by marking the original element as space-preserving.
fn preserve_space(xml: &str, scanned: &ScannedDocument, text: usize, edits: &mut Vec<Edit>) {
    let span = &scanned.texts[text];
    if span.preserves_space {
        return;
    }
    let tag = &xml[span.tag_start..span.tag_end];
    if tag.contains("xml:space") {
        return;
    }
    let rewritten = format!(r#"{} xml:space="preserve">"#, tag.trim_end_matches('>'));
    edits.push(Edit {
        at: span.tag_start,
        order: 2,
        until: span.tag_end,
        text: rewritten,
    });
}

fn comment_edits(
    scanned: &ScannedDocument,
    xml: &str,
    (start, end): (usize, usize),
    id: u64,
    edits: &mut Vec<Edit>,
) -> Result<(), String> {
    let invalid = || "A quoted passage could not be mapped back to the Word document".to_string();
    let insert = |at: usize, order: u8, text: String| Edit {
        at,
        order,
        until: at,
        text,
    };
    let last = end
        .checked_sub(1)
        .filter(|last| *last >= start)
        .ok_or_else(invalid)?;
    let mark = start_mark(id);
    match scanned.sources.get(start).copied().ok_or_else(invalid)? {
        CharSource::Text {
            run,
            text,
            raw_start,
            ..
        } => {
            if scanned.runs[run].first_char == Some(start) {
                edits.push(insert(scanned.runs[run].start, 1, mark));
            } else if raw_start == scanned.texts[text].tag_end {
                let at = scanned.texts[text].tag_start;
                edits.push(insert(at, 1, split_run(xml, scanned, run, &mark)));
            } else {
                edits.push(insert(raw_start, 1, split_text(xml, scanned, run, &mark)));
                preserve_space(xml, scanned, text, edits);
            }
        }
        CharSource::Element { run, start: at, .. } => {
            if scanned.runs[run].first_char == Some(start) {
                edits.push(insert(scanned.runs[run].start, 1, mark));
            } else {
                edits.push(insert(at, 1, split_run(xml, scanned, run, &mark)));
            }
        }
        CharSource::Math { math } => edits.push(insert(scanned.maths[math].0, 1, mark)),
        CharSource::Separator => return Err(invalid()),
    }
    let mark = end_mark(id);
    match scanned.sources.get(last).copied().ok_or_else(invalid)? {
        CharSource::Text {
            run, text, raw_end, ..
        } => {
            if scanned.runs[run].last_char == Some(last) {
                edits.push(insert(scanned.runs[run].end, 0, mark));
            } else if raw_end == scanned.texts[text].content_end {
                let at = scanned.texts[text].element_end;
                edits.push(insert(at, 0, split_run(xml, scanned, run, &mark)));
            } else {
                edits.push(insert(raw_end, 0, split_text(xml, scanned, run, &mark)));
                preserve_space(xml, scanned, text, edits);
            }
        }
        CharSource::Element { run, end: at, .. } => {
            if scanned.runs[run].last_char == Some(last) {
                edits.push(insert(scanned.runs[run].end, 0, mark));
            } else {
                edits.push(insert(at, 0, split_run(xml, scanned, run, &mark)));
            }
        }
        CharSource::Math { math } => edits.push(insert(scanned.maths[math].1, 0, mark)),
        CharSource::Separator => return Err(invalid()),
    }
    Ok(())
}

fn apply_edits(xml: &str, mut edits: Vec<Edit>) -> Result<String, String> {
    edits.sort_by_key(|edit| (edit.at, edit.order));
    edits.dedup_by(|later, earlier| {
        later.order == 2 && earlier.order == 2 && later.at == earlier.at
    });
    let extra: usize = edits.iter().map(|edit| edit.text.len()).sum();
    let mut output = String::with_capacity(xml.len() + extra);
    let mut cursor = 0usize;
    for edit in edits {
        if edit.at < cursor {
            return Err(
                "Comment positions overlap in an unexpected way; no file was written".into(),
            );
        }
        output.push_str(&xml[cursor..edit.at]);
        output.push_str(&edit.text);
        cursor = edit.until;
    }
    output.push_str(&xml[cursor..]);
    Ok(output)
}

fn comment_element(id: u64, paragraphs: &[String], date: &str) -> String {
    let mut xml = format!(
        r#"<w:comment w:id="{id}" w:author="{}" w:date="{}" w:initials="PR">"#,
        escape_xml(super::ANNOTATION_AUTHOR),
        escape_xml(date)
    );
    for (index, paragraph) in paragraphs.iter().enumerate() {
        xml.push_str("<w:p>");
        if index == 0 {
            xml.push_str("<w:r><w:annotationRef/></w:r><w:r><w:rPr><w:b/></w:rPr>");
        } else {
            xml.push_str("<w:r>");
        }
        for (line_number, line) in paragraph.split('\n').enumerate() {
            if line_number > 0 {
                xml.push_str("<w:br/>");
            }
            xml.push_str(r#"<w:t xml:space="preserve">"#);
            xml.push_str(&escape_xml(line));
            xml.push_str("</w:t>");
        }
        xml.push_str("</w:r></w:p>");
    }
    xml.push_str("</w:comment>");
    xml
}

fn new_comments_part(comments: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:comments xmlns:w=\"{WORD_MAIN_NS}\">{comments}</w:comments>"
    )
}

/// Largest comment id in an existing comments part, after checking that it
/// is a standard WordprocessingML comments list.
fn comments_max_id(xml: &str) -> Result<u64, String> {
    let mut reader = Reader::from_str(xml);
    let mut max_id = 0u64;
    let mut saw_root = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element) | Event::Empty(element)) => {
                if !saw_root {
                    saw_root = true;
                    let namespace = element.try_get_attribute("xmlns:w").ok().flatten();
                    if element.name().as_ref() != "w:comments"
                        || namespace.as_ref().map(|value| value.value.as_ref())
                            != Some(WORD_MAIN_NS)
                    {
                        return Err("The Word document's existing comments use a layout Pipeline cannot extend".into());
                    }
                }
                if let Some(id) = numeric_id(&element) {
                    max_id = max_id.max(id);
                }
            }
            Ok(Event::DocType(_)) => return Err("Word XML must not contain a DTD".into()),
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("The existing comments are not valid XML: {error}")),
            _ => {}
        }
    }
    Ok(max_id)
}

fn append_comments(existing: &str, comments: &str) -> Result<String, String> {
    if let Some(position) = existing.rfind("</w:comments>") {
        let mut updated = existing.to_string();
        updated.insert_str(position, comments);
        return Ok(updated);
    }
    let empty = regex::Regex::new(r"<w:comments(\s[^>]*)?/>").expect("comments regex is valid");
    let found = empty
        .find(existing)
        .ok_or("The Word document's existing comments part is incomplete")?;
    let tag = found.as_str();
    let open = format!("{}>", tag.trim_end_matches("/>"));
    Ok(format!(
        "{}{open}{comments}</w:comments>{}",
        &existing[..found.start()],
        &existing[found.end()..]
    ))
}

struct Relationship {
    id: String,
    kind: String,
    target: String,
    external: bool,
}

fn parse_relationships(xml: &str) -> Result<Vec<Relationship>, String> {
    let mut reader = Reader::from_str(xml);
    let mut relationships = Vec::new();
    let attribute = |element: &quick_xml::events::BytesStart<'_>, name: &str| {
        element
            .try_get_attribute(name)
            .ok()
            .flatten()
            .and_then(|value| {
                value
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .ok()
                    .map(|value| value.into_owned())
            })
            .unwrap_or_default()
    };
    loop {
        match reader.read_event() {
            Ok(Event::Start(element) | Event::Empty(element))
                if element.local_name().as_ref() == "Relationship" =>
            {
                relationships.push(Relationship {
                    id: attribute(&element, "Id"),
                    kind: attribute(&element, "Type"),
                    target: attribute(&element, "Target"),
                    external: attribute(&element, "TargetMode") == "External",
                });
            }
            Ok(Event::DocType(_)) => return Err("Word XML must not contain a DTD".into()),
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(format!(
                    "The Word document's relationships are not valid XML: {error}"
                ))
            }
            _ => {}
        }
    }
    Ok(relationships)
}

/// Resolve a relationship target from `word/document.xml` to a part name.
fn resolve_part_name(target: &str) -> String {
    let mut segments: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        vec!["word"]
    };
    for segment in target.trim_start_matches('/').split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

fn add_comments_relationship(
    existing: Option<&str>,
    relationships: &[Relationship],
) -> Result<String, String> {
    let mut id = "rIdPipelineComments".to_string();
    let mut suffix = 1;
    while relationships
        .iter()
        .any(|relationship| relationship.id == id)
    {
        suffix += 1;
        id = format!("rIdPipelineComments{suffix}");
    }
    let entry =
        format!(r#"<Relationship Id="{id}" Type="{COMMENTS_REL_TYPE}" Target="comments.xml"/>"#);
    let Some(existing) = existing else {
        return Ok(format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"{RELS_NS}\">{entry}</Relationships>"
        ));
    };
    let position = existing
        .rfind("</Relationships>")
        .ok_or("The Word document's relationship list is incomplete")?;
    let mut updated = existing.to_string();
    updated.insert_str(position, &entry);
    Ok(updated)
}

/// Add the comments content type when the package does not declare it.
fn add_comments_content_type(content_types: &str) -> Result<Option<String>, String> {
    let mut reader = Reader::from_str(content_types);
    loop {
        match reader.read_event() {
            Ok(Event::Start(element) | Event::Empty(element))
                if element.local_name().as_ref() == "Override" =>
            {
                let part = element
                    .try_get_attribute("PartName")
                    .ok()
                    .flatten()
                    .map(|value| value.value.to_ascii_lowercase());
                if part.as_deref() == Some("/word/comments.xml") {
                    return Ok(None);
                }
            }
            Ok(Event::DocType(_)) => return Err("Word XML must not contain a DTD".into()),
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("The content-type list is not valid XML: {error}")),
            _ => {}
        }
    }
    let position = content_types
        .rfind("</Types>")
        .ok_or("The Word document's content-type list is incomplete")?;
    let mut updated = content_types.to_string();
    updated.insert_str(
        position,
        &format!(
            r#"<Override PartName="/word/comments.xml" ContentType="{COMMENTS_CONTENT_TYPE}"/>"#
        ),
    );
    Ok(Some(updated))
}

fn rewrite_package<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    replacements: &[(String, String)],
    additions: &[(String, String)],
) -> Result<Vec<u8>, String> {
    let failed =
        |error: zip::result::ZipError| format!("Could not write the Word document: {error}");
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for index in 0..archive.len() {
        let (name, modified) = {
            let entry = archive.by_index_raw(index).map_err(failed)?;
            (
                entry
                    .name()
                    .map_err(|error| format!("Invalid DOCX entry name: {error}"))?
                    .into_owned(),
                entry.last_modified(),
            )
        };
        if let Some((_, content)) = replacements.iter().find(|(part, _)| *part == name) {
            let options = match modified {
                Some(time) => options.last_modified_time(time),
                None => options,
            };
            writer.start_file(name, options).map_err(failed)?;
            writer
                .write_all(content.as_bytes())
                .map_err(|error| format!("Could not write the Word document: {error}"))?;
        } else {
            writer
                .raw_copy_file(archive.by_index_raw(index).map_err(failed)?)
                .map_err(failed)?;
        }
    }
    for (name, content) in additions {
        writer.start_file(name.as_str(), options).map_err(failed)?;
        writer
            .write_all(content.as_bytes())
            .map_err(|error| format!("Could not write the Word document: {error}"))?;
    }
    Ok(writer.finish().map_err(failed)?.into_inner())
}

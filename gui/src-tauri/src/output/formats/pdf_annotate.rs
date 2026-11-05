//! Highlights and notes on a copy of the reviewed PDF.
//!
//! Annotations are appended as a PDF incremental update: the output starts
//! with the original file's exact bytes, followed by new annotation objects,
//! updated page dictionaries, and a new cross-reference section. Removing the
//! appended section restores the original, and signatures over the original
//! byte range remain intact.

use super::model::{ExportEvidence, ExportFinding, ReportExport};
use super::pdf_words::{LocatedQuote, PageSearch, PageWords};
use super::quote_match::{MatchKind, Needle};
use super::text::{one_line, truncate_chars, xml_char_allowed};
use lopdf::{dictionary, Dictionary, Document, IncrementalDocument, Object, ObjectId};
use std::collections::BTreeMap;

const MAX_NOTE_CHARS: usize = 4_000;
const MAX_PDF_BYTES: usize = 256 * 1024 * 1024;

/// An annotated copy of the reviewed PDF.
#[derive(Debug)]
pub struct PdfAnnotation {
    pub bytes: Vec<u8>,
    /// Findings with at least one highlighted quotation.
    pub anchored: usize,
    /// Findings shown only as page notes.
    pub unanchored: usize,
}

#[derive(Debug, Clone, Copy)]
struct PageBox {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    rotate: i64,
}

impl PageBox {
    fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    fn height(&self) -> f32 {
        self.y1 - self.y0
    }

    /// Convert a Poppler display rectangle (top-left origin, after rotation,
    /// relative to the media box) to a PDF user-space rectangle.
    fn user_rect(&self, (u0, v0, u1, v1): (f32, f32, f32, f32)) -> [f32; 4] {
        let (width, height) = (self.width(), self.height());
        let point = |u: f32, v: f32| -> (f32, f32) {
            let (x, y) = match self.rotate {
                90 => (v, u),
                180 => (width - u, v),
                270 => (width - v, height - u),
                _ => (u, height - v),
            };
            (self.x0 + x, self.y0 + y)
        };
        let (ax, ay) = point(u0, v0);
        let (bx, by) = point(u1, v1);
        [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)]
    }

    /// Width and height of the page as displayed (after `/Rotate`).
    fn display_size(&self) -> (f32, f32) {
        if matches!(self.rotate, 90 | 270) {
            (self.height(), self.width())
        } else {
            (self.width(), self.height())
        }
    }

    /// A closed popup by the displayed page's right margin, level with `top`.
    fn popup_rect(&self, top: f32) -> [f32; 4] {
        let (width, height) = self.display_size();
        let right = width - 12.0;
        let top = top.clamp(0.0, (height - 160.0).max(0.0));
        self.user_rect(((right - 240.0).max(0.0), top, right, top + 160.0))
    }

    /// The `slot`th note icon down the displayed page's top-left margin.
    fn note_rect(&self, slot: usize) -> [f32; 4] {
        let top = 12.0 + 26.0 * (slot % 20) as f32;
        self.user_rect((12.0, top, 32.0, top + 20.0))
    }
}

enum Placement {
    Highlight {
        page: u32,
        rects: Vec<[f32; 4]>,
        /// Displayed top edge of the first line, for the popup position.
        top: f32,
        contents: String,
        severity: String,
    },
    Note {
        page: u32,
        contents: String,
        severity: String,
    },
}

fn number(value: f32) -> Object {
    Object::Real((value * 100.0).round() / 100.0)
}

fn rect_object(rect: [f32; 4]) -> Object {
    Object::Array(rect.iter().map(|value| number(*value)).collect())
}

fn color(severity: &str) -> Object {
    let rgb = match severity {
        "high" => [1.0, 0.62, 0.6],
        "medium" => [1.0, 0.83, 0.42],
        "low" => [0.62, 0.8, 1.0],
        _ => [1.0, 0.95, 0.4],
    };
    Object::Array(rgb.iter().map(|value| Object::Real(*value)).collect())
}

/// PDF text string; control characters are dropped, line breaks kept.
fn text(value: &str) -> Object {
    let clean: String = value
        .chars()
        .filter(|character| xml_char_allowed(*character))
        .collect();
    lopdf::text_string(&truncate_chars(&clean, MAX_NOTE_CHARS))
}

fn pdf_date(date: &str) -> String {
    let digits: String = date.chars().filter(char::is_ascii_digit).take(14).collect();
    format!("D:{digits:0<14}Z")
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    document.dereference(object).ok().map(|(_, object)| object)
}

/// Read an inheritable page attribute such as `/MediaBox` or `/Rotate`.
fn inherited<'a>(document: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut current = document.get_dictionary(page).ok()?;
    for _ in 0..64 {
        if let Ok(value) = current.get(key) {
            return resolve(document, value);
        }
        let parent = current.get(b"Parent").ok()?.as_reference().ok()?;
        current = document.get_dictionary(parent).ok()?;
    }
    None
}

fn page_box(document: &Document, page: ObjectId) -> PageBox {
    let values: Vec<f32> = inherited(document, page, b"MediaBox")
        .and_then(|value| value.as_array().ok())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| resolve(document, value)?.as_float().ok())
                .collect()
        })
        .unwrap_or_default();
    let (x0, y0, x1, y1) = match values.as_slice() {
        [a, b, c, d] => (a.min(*c), b.min(*d), a.max(*c), b.max(*d)),
        _ => (0.0, 0.0, 612.0, 792.0),
    };
    let rotate = inherited(document, page, b"Rotate")
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    PageBox {
        x0,
        y0,
        x1,
        y1,
        rotate,
    }
}

fn highlight_note(finding: &ExportFinding, evidence: &ExportEvidence, found_on: u32) -> String {
    let mut paragraphs = finding.note_paragraphs(Some(evidence));
    if let Some(cited) = evidence.page.filter(|cited| *cited != found_on) {
        paragraphs.push(format!(
            "The finding cites page {cited}; Pipeline found the quotation on page {found_on}."
        ));
    }
    paragraphs.join("\n\n")
}

fn page_note(finding: &ExportFinding) -> String {
    let mut paragraphs = finding.note_paragraphs(None);
    let quoted: Vec<&ExportEvidence> = finding
        .evidence
        .iter()
        .filter(|e| !e.quote.is_empty())
        .collect();
    if quoted.is_empty() {
        paragraphs.push("No quotation was given for this finding.".to_string());
    } else {
        paragraphs
            .push("Pipeline could not find the quoted text in this PDF's text layer.".to_string());
        for evidence in quoted.into_iter().take(3) {
            paragraphs.push(format!(
                "{} · “{}”",
                super::model::evidence_line(evidence),
                truncate_chars(&one_line(&evidence.quote), 300)
            ));
        }
    }
    paragraphs.join("\n\n")
}

/// Search the cited page first, then every other page, preferring a
/// normalized match to a punctuation-blind one at each stage.
fn find_quote(
    searches: &BTreeMap<u32, PageSearch<'_>>,
    needle: &Needle,
    cited: Option<u32>,
) -> Option<(u32, LocatedQuote)> {
    let kinds = [MatchKind::Normalized, MatchKind::IgnoringPunctuation];
    if let Some(search) = cited.and_then(|page| searches.get(&page)) {
        for kind in kinds {
            if let Some(found) = search.locate(needle, kind) {
                return Some((cited?, found));
            }
        }
    }
    for kind in kinds {
        for (page, search) in searches {
            if Some(*page) == cited {
                continue;
            }
            if let Some(found) = search.locate(needle, kind) {
                return Some((*page, found));
            }
        }
    }
    None
}

fn plan(
    document: &Document,
    page_ids: &BTreeMap<u32, ObjectId>,
    pages: &[PageWords],
    findings: &[ExportFinding],
) -> (Vec<Placement>, usize) {
    let searches: BTreeMap<u32, PageSearch<'_>> = pages
        .iter()
        .filter(|page| page_ids.contains_key(&page.number))
        .map(|page| (page.number, page.searcher()))
        .collect();
    let mut placements = Vec::new();
    let mut anchored = 0usize;
    for finding in findings {
        let mut highlighted = false;
        for evidence in finding.evidence.iter().filter(|e| !e.quote.is_empty()) {
            let Some(needle) = Needle::new(&evidence.quote) else {
                continue;
            };
            let Some((page, found)) = find_quote(&searches, &needle, evidence.page) else {
                continue;
            };
            let page_box = page_box(document, page_ids[&page]);
            placements.push(Placement::Highlight {
                page,
                rects: found
                    .lines
                    .iter()
                    .map(|line| page_box.user_rect(*line))
                    .collect(),
                top: found
                    .lines
                    .iter()
                    .map(|line| line.1)
                    .fold(f32::MAX, f32::min),
                contents: highlight_note(finding, evidence, page),
                severity: finding.severity.clone(),
            });
            highlighted = true;
        }
        if highlighted {
            anchored += 1;
            continue;
        }
        let page = finding
            .evidence
            .iter()
            .filter_map(|evidence| evidence.page)
            .find(|page| page_ids.contains_key(page))
            .or_else(|| page_ids.keys().next().copied())
            .unwrap_or(1);
        placements.push(Placement::Note {
            page,
            contents: page_note(finding),
            severity: finding.severity.clone(),
        });
    }
    (placements, anchored)
}

struct Writer<'a> {
    document: &'a mut IncrementalDocument,
    date: String,
    serial: usize,
    added: BTreeMap<u32, Vec<ObjectId>>,
}

impl Writer<'_> {
    fn common(
        &mut self,
        subtype: &str,
        page: ObjectId,
        rect: [f32; 4],
        contents: &str,
        severity: &str,
    ) -> Dictionary {
        self.serial += 1;
        dictionary! {
            "Type" => "Annot",
            "Subtype" => Object::Name(subtype.as_bytes().to_vec()),
            "Rect" => rect_object(rect),
            "Contents" => text(contents),
            "T" => text(super::ANNOTATION_AUTHOR),
            "M" => Object::string_literal(self.date.clone()),
            "CreationDate" => Object::string_literal(self.date.clone()),
            "NM" => Object::string_literal(format!("pipeline-review-{}", self.serial)),
            "C" => color(severity),
            "P" => Object::Reference(page),
        }
    }

    fn push(
        &mut self,
        page_number: u32,
        page: ObjectId,
        mut annotation: Dictionary,
        popup_rect: [f32; 4],
    ) {
        let new = &mut self.document.new_document;
        let annotation_id = new.new_object_id();
        let popup_id = new.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Popup",
            "Rect" => rect_object(popup_rect),
            "Parent" => Object::Reference(annotation_id),
            "Open" => false,
            "P" => Object::Reference(page),
        });
        annotation.set("Popup", Object::Reference(popup_id));
        new.set_object(annotation_id, annotation);
        self.added
            .entry(page_number)
            .or_default()
            .extend([annotation_id, popup_id]);
    }
}

/// Attach the new annotation ids to each page's `/Annots`, copying the page
/// (and an indirect annotation array) into the incremental update.
fn attach(
    document: &mut IncrementalDocument,
    page: ObjectId,
    ids: &[ObjectId],
) -> Result<(), String> {
    let failed = |error: lopdf::Error| format!("Could not update a PDF page: {error}");
    document
        .opt_clone_object_to_new_document(page)
        .map_err(failed)?;
    let existing = document
        .new_document
        .get_dictionary(page)
        .map_err(failed)?
        .get(b"Annots")
        .ok()
        .cloned();
    let references = ids.iter().map(|id| Object::Reference(*id));
    match existing {
        Some(Object::Reference(array_id)) => {
            document
                .opt_clone_object_to_new_document(array_id)
                .map_err(failed)?;
            let array = document
                .new_document
                .get_object_mut(array_id)
                .map_err(failed)?
                .as_array_mut()
                .map_err(|_| "A PDF page has an unreadable annotation list".to_string())?;
            array.extend(references);
        }
        Some(Object::Array(mut array)) => {
            array.extend(references);
            let page = document
                .new_document
                .get_dictionary_mut(page)
                .map_err(failed)?;
            page.set("Annots", Object::Array(array));
        }
        _ => {
            let page = document
                .new_document
                .get_dictionary_mut(page)
                .map_err(failed)?;
            page.set("Annots", Object::Array(references.collect()));
        }
    }
    Ok(())
}

/// Add highlights and notes for every finding to a copy of `original`.
/// `pages` are Poppler word boxes for the same bytes; `date` is the ISO
/// date recorded on each annotation.
pub fn annotate_pdf(
    original: &[u8],
    pages: &[PageWords],
    export: &ReportExport,
    date: &str,
) -> Result<PdfAnnotation, String> {
    if export.findings.is_empty() {
        return Err("This Review has no individual findings to place on the PDF. Export it as Word or HTML instead.".into());
    }
    if original.len() > MAX_PDF_BYTES {
        return Err("The PDF is too large to annotate".into());
    }
    let mut document: IncrementalDocument =
        original.try_into().map_err(|error: lopdf::Error| {
            format!("Pipeline could not read this PDF's structure: {error}")
        })?;
    if document.get_prev_documents().is_encrypted() {
        return Err(
            "This PDF is encrypted or password-protected, so Pipeline cannot add highlights to it."
                .into(),
        );
    }
    let page_ids = document.get_prev_documents().get_pages();
    if page_ids.is_empty() {
        return Err("The PDF has no pages".into());
    }
    let (placements, anchored) = plan(
        document.get_prev_documents(),
        &page_ids,
        pages,
        &export.findings,
    );
    let boxes: BTreeMap<u32, PageBox> = page_ids
        .iter()
        .map(|(number, id)| (*number, page_box(document.get_prev_documents(), *id)))
        .collect();
    let mut writer = Writer {
        document: &mut document,
        date: pdf_date(date),
        serial: 0,
        added: BTreeMap::new(),
    };
    let mut notes_per_page: BTreeMap<u32, usize> = BTreeMap::new();
    for placement in &placements {
        match placement {
            Placement::Highlight {
                page,
                rects,
                top,
                contents,
                severity,
            } => {
                let page_id = page_ids[page];
                let page_box = boxes[page];
                let bounds =
                    rects
                        .iter()
                        .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |acc, rect| {
                            [
                                acc[0].min(rect[0]),
                                acc[1].min(rect[1]),
                                acc[2].max(rect[2]),
                                acc[3].max(rect[3]),
                            ]
                        });
                let mut annotation =
                    writer.common("Highlight", page_id, bounds, contents, severity);
                let quads: Vec<Object> = rects
                    .iter()
                    .flat_map(|[x0, y0, x1, y1]| [*x0, *y1, *x1, *y1, *x0, *y0, *x1, *y0])
                    .map(number)
                    .collect();
                annotation.set("QuadPoints", Object::Array(quads));
                annotation.set("F", 4);
                annotation.set("Subj", text("Review finding"));
                writer.push(*page, page_id, annotation, page_box.popup_rect(*top));
            }
            Placement::Note {
                page,
                contents,
                severity,
            } => {
                let page_id = page_ids[page];
                let page_box = boxes[page];
                let slot = notes_per_page.entry(*page).or_default();
                let rect = page_box.note_rect(*slot);
                let top = 12.0 + 26.0 * (*slot % 20) as f32;
                *slot += 1;
                let mut annotation = writer.common("Text", page_id, rect, contents, severity);
                annotation.set("Name", "Comment");
                annotation.set("Open", false);
                annotation.set("F", 28);
                annotation.set("Subj", text("Review finding"));
                writer.push(*page, page_id, annotation, page_box.popup_rect(top));
            }
        }
    }
    let added = std::mem::take(&mut writer.added);
    for (page, ids) in &added {
        attach(&mut document, page_ids[page], ids)?;
    }
    let mut bytes = Vec::with_capacity(original.len() + 64 * 1024);
    document
        .save_to(&mut bytes)
        .map_err(|error| format!("Could not write the annotated PDF: {error}"))?;
    verify(original, &bytes, added.values().map(Vec::len).sum())?;
    Ok(PdfAnnotation {
        bytes,
        anchored,
        unanchored: export.findings.len() - anchored,
    })
}

/// The update must leave the original bytes untouched and reopen cleanly.
fn verify(original: &[u8], annotated: &[u8], added: usize) -> Result<(), String> {
    if !annotated.starts_with(original) {
        return Err(
            "The annotated PDF does not preserve the original bytes; no file was written".into(),
        );
    }
    let reopened = Document::load_mem(annotated)
        .map_err(|error| format!("The annotated PDF could not be reopened: {error}"))?;
    let total: usize = reopened
        .get_pages()
        .values()
        .map(|page| {
            reopened
                .get_page_annotations(*page)
                .map(|a| a.len())
                .unwrap_or(0)
        })
        .sum();
    if total < added {
        return Err("The annotated PDF lost some annotations; no file was written".into());
    }
    Ok(())
}

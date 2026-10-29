//! Map the visible text of `word/document.xml` back to byte offsets.
//!
//! The scan yields one flat character sequence for the body (paragraphs
//! separated by a space) and, for each character, where it came from: a byte
//! range inside a `w:t`, a whole inline element such as `w:tab`, or an
//! equation. Comment anchoring splices markup at these offsets, so every byte
//! the scan does not point at is preserved exactly.
//!
//! Text boxes and alternate-content blocks are skipped: Word repeats their
//! text in fallback copies, and comment markup inside them is fragile.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

pub(super) const WORD_MAIN_NS: &str =
    "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CharSource {
    /// Paragraph boundary; never the first or last character of a match.
    Separator,
    /// One character of a splittable `w:t`, at `[raw_start, raw_end)`.
    Text {
        run: usize,
        text: usize,
        raw_start: usize,
        raw_end: usize,
    },
    /// An inline element or unsplittable text occupying `[start, end)`.
    Element {
        run: usize,
        start: usize,
        end: usize,
    },
    /// Text inside an equation; boundaries move to the whole equation.
    Math { math: usize },
}

#[derive(Debug, Clone, Default)]
pub(super) struct RunSpan {
    pub start: usize,
    pub end: usize,
    /// Byte range of the run's own `w:rPr`, copied when the run is split.
    pub properties: Option<(usize, usize)>,
    pub first_char: Option<usize>,
    pub last_char: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct TextSpan {
    pub tag_start: usize,
    pub tag_end: usize,
    pub content_end: usize,
    pub element_end: usize,
    pub preserves_space: bool,
}

#[derive(Debug, Default)]
pub(super) struct ScannedDocument {
    pub chars: Vec<char>,
    pub sources: Vec<CharSource>,
    pub runs: Vec<RunSpan>,
    pub texts: Vec<TextSpan>,
    pub maths: Vec<(usize, usize)>,
    /// First and last character of the first body paragraph with text.
    pub first_paragraph: Option<(usize, usize)>,
    /// Largest numeric `w:id` anywhere in the part.
    pub max_id: u64,
}

impl ScannedDocument {
    /// Visible text only (separators included), used to prove an edit left
    /// the document's words unchanged.
    pub fn visible_text(&self) -> String {
        self.chars.iter().collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    Paragraph { first_char: Option<usize> },
    Run(usize),
    Text(usize),
    RunProperties(usize),
    Math,
    Skipped,
    Other,
}

fn is_skipped_container(name: &str) -> bool {
    matches!(
        name,
        "w:txbxContent" | "mc:AlternateContent" | "w:del" | "w:moveFrom"
    )
}

fn is_math_root(name: &str) -> bool {
    matches!(name, "m:oMathPara" | "m:oMath")
}

pub(super) fn numeric_id(element: &BytesStart<'_>) -> Option<u64> {
    let attribute = element.try_get_attribute("w:id").ok().flatten()?;
    attribute.value.trim().parse().ok()
}

/// Decode a `w:t` body, keeping each character's raw byte range. Returns
/// `None` for CDATA, comments, or processing instructions inside the text,
/// which are left whole.
fn decode_text(raw: &str, base: usize) -> Option<Vec<(char, usize, usize)>> {
    if raw.contains("<!") || raw.contains("<?") {
        return None;
    }
    let mut decoded = Vec::with_capacity(raw.len());
    let mut index = 0usize;
    while index < raw.len() {
        let rest = &raw[index..];
        if rest.starts_with('&') {
            let end = rest.find(';')?;
            let entity = &rest[1..end];
            let character = match entity {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                _ => {
                    let value = if let Some(hex) = entity.strip_prefix("#x") {
                        u32::from_str_radix(hex, 16).ok()?
                    } else if let Some(hex) = entity.strip_prefix("#X") {
                        u32::from_str_radix(hex, 16).ok()?
                    } else {
                        entity.strip_prefix('#')?.parse().ok()?
                    };
                    char::from_u32(value)?
                }
            };
            decoded.push((character, base + index, base + index + end + 1));
            index += end + 1;
        } else {
            let character = rest.chars().next()?;
            decoded.push((character, base + index, base + index + character.len_utf8()));
            index += character.len_utf8();
        }
    }
    Some(decoded)
}

struct Scanner<'a> {
    xml: &'a str,
    document: ScannedDocument,
    stack: Vec<Frame>,
    skipped: usize,
    math_depth: usize,
    saw_body: bool,
}

impl Scanner<'_> {
    /// Innermost run that belongs to the innermost paragraph.
    fn current_run(&self) -> Option<usize> {
        for frame in self.stack.iter().rev() {
            match frame {
                Frame::Run(index) => return Some(*index),
                Frame::Paragraph { .. } => return None,
                _ => {}
            }
        }
        None
    }

    fn in_paragraph(&self) -> bool {
        self.skipped == 0
            && self
                .stack
                .iter()
                .any(|frame| matches!(frame, Frame::Paragraph { .. }))
    }

    fn parent(&self) -> Option<Frame> {
        self.stack.last().copied()
    }

    fn push_char(&mut self, character: char, source: CharSource) {
        let index = self.document.chars.len();
        self.document.chars.push(character);
        self.document.sources.push(source);
        let run = match source {
            CharSource::Text { run, .. } | CharSource::Element { run, .. } => Some(run),
            _ => None,
        };
        if let Some(run) = run {
            let span = &mut self.document.runs[run];
            span.first_char.get_or_insert(index);
            span.last_char = Some(index);
        }
        if let Some(Frame::Paragraph { first_char }) = self
            .stack
            .iter_mut()
            .rev()
            .find(|frame| matches!(frame, Frame::Paragraph { .. }))
        {
            first_char.get_or_insert(index);
        }
    }

    fn start(&mut self, element: &BytesStart<'_>, before: usize, after: usize, empty: bool) {
        if let Some(id) = numeric_id(element) {
            self.document.max_id = self.document.max_id.max(id);
        }
        let name = element.name();
        let name = name.as_ref();
        if name == "w:body" {
            self.saw_body = true;
        }
        let parent = self.parent();
        if empty {
            if let (Some(Frame::Run(run)), true) = (parent, self.in_paragraph()) {
                let character = match name {
                    "w:tab" | "w:ptab" => Some('\t'),
                    "w:br" | "w:cr" => Some('\n'),
                    "w:noBreakHyphen" => Some('-'),
                    _ => None,
                };
                if let Some(character) = character.filter(|_| self.math_depth == 0) {
                    let source = CharSource::Element {
                        run,
                        start: before,
                        end: after,
                    };
                    self.push_char(character, source);
                }
                if name == "w:rPr" {
                    self.document.runs[run].properties = Some((before, after));
                }
            }
            return;
        }
        let frame = if is_skipped_container(name) {
            self.skipped += 1;
            Frame::Skipped
        } else if is_math_root(name) && self.math_depth == 0 {
            self.math_depth = 1;
            self.document.maths.push((before, before));
            Frame::Math
        } else if is_math_root(name) {
            self.math_depth += 1;
            Frame::Math
        } else if name == "w:p" {
            Frame::Paragraph { first_char: None }
        } else if name == "w:r" && self.math_depth == 0 {
            self.document.runs.push(RunSpan {
                start: before,
                end: before,
                ..RunSpan::default()
            });
            Frame::Run(self.document.runs.len() - 1)
        } else if name == "w:rPr" && matches!(parent, Some(Frame::Run(_))) {
            let Some(Frame::Run(run)) = parent else {
                unreachable!()
            };
            Frame::RunProperties(run)
        } else if matches!(name, "w:t" | "m:t") {
            let preserves_space = element
                .try_get_attribute("xml:space")
                .ok()
                .flatten()
                .is_some_and(|value| value.value.as_ref() == "preserve");
            self.document.texts.push(TextSpan {
                tag_start: before,
                tag_end: after,
                preserves_space,
                ..TextSpan::default()
            });
            Frame::Text(self.document.texts.len() - 1)
        } else {
            Frame::Other
        };
        if let Frame::RunProperties(run) = frame {
            self.document.runs[run].properties = Some((before, before));
        }
        self.stack.push(frame);
    }

    fn end(&mut self, before: usize, after: usize) -> Result<(), String> {
        let frame = self
            .stack
            .pop()
            .ok_or("Word document has an unmatched closing tag")?;
        match frame {
            Frame::Skipped => self.skipped -= 1,
            Frame::Math => {
                self.math_depth -= 1;
                if self.math_depth == 0 {
                    if let Some(math) = self.document.maths.last_mut() {
                        math.1 = after;
                    }
                }
            }
            Frame::Run(run) => self.document.runs[run].end = after,
            Frame::RunProperties(run) => {
                if let Some(properties) = self.document.runs[run].properties.as_mut() {
                    properties.1 = after;
                }
            }
            Frame::Text(text) => {
                self.document.texts[text].content_end = before;
                self.document.texts[text].element_end = after;
                self.collect_text(text);
            }
            Frame::Paragraph { first_char } => {
                if self.skipped == 0 {
                    if let Some(first) = first_char {
                        let last = self.document.chars.len() - 1;
                        self.document.first_paragraph.get_or_insert((first, last));
                        self.document.chars.push(' ');
                        self.document.sources.push(CharSource::Separator);
                    }
                }
            }
            Frame::Other => {}
        }
        Ok(())
    }

    fn collect_text(&mut self, text: usize) {
        if !self.in_paragraph() {
            return;
        }
        let span = self.document.texts[text].clone();
        let raw = &self.xml[span.tag_end..span.content_end];
        if self.math_depth > 0 {
            let math = self.document.maths.len() - 1;
            let decoded = decode_text(raw, span.tag_end).unwrap_or_default();
            for (character, _, _) in decoded {
                self.push_char(character, CharSource::Math { math });
            }
            return;
        }
        let Some(run) = self.current_run() else {
            return;
        };
        match decode_text(raw, span.tag_end) {
            Some(decoded) => {
                for (character, raw_start, raw_end) in decoded {
                    let source = CharSource::Text {
                        run,
                        text,
                        raw_start,
                        raw_end,
                    };
                    self.push_char(character, source);
                }
            }
            None => {
                let source = CharSource::Element {
                    run,
                    start: span.tag_start,
                    end: span.element_end,
                };
                let visible = quick_xml::escape::unescape(raw)
                    .map(|value| value.into_owned())
                    .unwrap_or_default();
                for character in visible.chars() {
                    self.push_char(character, source);
                }
            }
        }
    }
}

/// Scan a WordprocessingML main part. Rejects a DTD, Strict Open XML, and
/// unbalanced markup.
pub(super) fn scan_document(xml: &str) -> Result<ScannedDocument, String> {
    let (body, base) = match xml.strip_prefix('\u{FEFF}') {
        Some(rest) => (rest, '\u{FEFF}'.len_utf8()),
        None => (xml, 0),
    };
    let mut reader = Reader::from_str(body);
    reader.config_mut().check_end_names = true;
    let mut scanner = Scanner {
        xml,
        document: ScannedDocument::default(),
        stack: Vec::new(),
        skipped: 0,
        math_depth: 0,
        saw_body: false,
    };
    let mut saw_root = false;
    loop {
        let before = reader.buffer_position() as usize + base;
        let event = reader
            .read_event()
            .map_err(|error| format!("The Word document's text part is not valid XML: {error}"))?;
        let after = reader.buffer_position() as usize + base;
        match event {
            Event::Start(element) | Event::Empty(element) if !saw_root => {
                saw_root = true;
                if element.name().as_ref() != "w:document" {
                    return Err("The Word document's main part has an unexpected root".into());
                }
                let namespace = element.try_get_attribute("xmlns:w").ok().flatten();
                if namespace.as_ref().map(|value| value.value.as_ref()) != Some(WORD_MAIN_NS) {
                    return Err("This Word file uses Strict Open XML or an unusual namespace layout, which Pipeline cannot annotate yet. Save it from Word as a standard .docx and run the Review again.".into());
                }
                scanner.stack.push(Frame::Other);
            }
            Event::Start(element) => scanner.start(&element, before, after, false),
            Event::Empty(element) => scanner.start(&element, before, after, true),
            Event::End(_) => scanner.end(before, after)?,
            Event::DocType(_) => return Err("Word XML must not contain a DTD".into()),
            Event::Eof => break,
            _ => {}
        }
        if scanner.stack.len() > 4_096 {
            return Err("The Word document is nested too deeply to annotate safely".into());
        }
    }
    if !scanner.stack.is_empty() || !saw_root {
        return Err("The Word document's text part is incomplete".into());
    }
    if !scanner.saw_body {
        return Err("The Word document has no body".into());
    }
    Ok(scanner.document)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(body: &str) -> String {
        format!(
            r#"<?xml version="1.0"?><w:document xmlns:w="{WORD_MAIN_NS}" xmlns:m="m"><w:body>{body}</w:body></w:document>"#
        )
    }

    #[test]
    fn maps_runs_entities_tabs_and_paragraph_separators() {
        let xml = doc(concat!(
            r#"<w:p><w:pPr><w:tabs><w:tab w:val="left" w:pos="720"/></w:tabs></w:pPr>"#,
            r#"<w:r><w:rPr><w:b/></w:rPr><w:t>A &amp; </w:t></w:r><w:r><w:tab/><w:t>B</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t xml:space="preserve">C</w:t></w:r><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></w:p>"#,
        ));
        let scanned = scan_document(&xml).unwrap();
        assert_eq!(scanned.visible_text(), "A & \tB Cx ");
        let CharSource::Text {
            raw_start, raw_end, ..
        } = scanned.sources[2]
        else {
            panic!("ampersand should map to its entity");
        };
        assert_eq!(&xml[raw_start..raw_end], "&amp;");
        assert!(matches!(scanned.sources[4], CharSource::Element { .. }));
        assert!(matches!(scanned.sources[8], CharSource::Math { math: 0 }));
        let (start, end) = scanned.maths[0];
        assert!(
            xml[start..end].starts_with("<m:oMath>") && xml[start..end].ends_with("</m:oMath>")
        );
        let (properties_start, properties_end) = scanned.runs[0].properties.unwrap();
        assert_eq!(
            &xml[properties_start..properties_end],
            "<w:rPr><w:b/></w:rPr>"
        );
        assert_eq!(scanned.first_paragraph, Some((0, 5)));
        assert!(scanned.texts[2].preserves_space);
    }

    #[test]
    fn skips_text_boxes_deleted_text_and_rejects_strict_or_dtd_documents() {
        let xml = doc(concat!(
            r#"<w:p><w:r><w:t>Keep</w:t></w:r><w:r><mc:AlternateContent><mc:Choice><w:txbxContent>"#,
            r#"<w:p><w:r><w:t>Box</w:t></w:r></w:p></w:txbxContent></mc:Choice></mc:AlternateContent></w:r>"#,
            r#"<w:del w:id="9"><w:r><w:delText>gone</w:delText></w:r></w:del></w:p>"#,
        ));
        let scanned = scan_document(&xml).unwrap();
        assert_eq!(scanned.visible_text(), "Keep ");
        assert_eq!(scanned.max_id, 9);
        let strict = xml.replace(
            WORD_MAIN_NS,
            "http://purl.oclc.org/ooxml/wordprocessingml/main",
        );
        assert!(scan_document(&strict).unwrap_err().contains("Strict"));
        let dtd = format!("<!DOCTYPE x>{xml}");
        assert!(scan_document(&dtd).is_err());
        assert!(scan_document(&xml.replace("</w:body>", "")).is_err());
    }
}

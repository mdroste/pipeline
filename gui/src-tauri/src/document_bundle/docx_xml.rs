//! XML extraction preserves run separators and complete math subtrees.
//!
//! `body` and `notes` read a whole Word part in one streaming pass, because
//! Word's paragraphs nest: a text box holds its own `w:p` inside the paragraph
//! it is anchored to, and an empty table cell ends in a self-closing `w:p`.
//! A finished nested paragraph waits for its enclosing paragraph, so the
//! enclosing sentence stays whole and the box text follows it. Deleted and
//! moved-away text is not read, and an `mc:AlternateContent` block yields one
//! branch: the first `mc:Choice` with text, otherwise `mc:Fallback`, which is
//! the only place Word writes the character of some symbols.
use quick_xml::{
    events::{BytesStart, Event},
    Reader, Writer,
};
use std::collections::BTreeMap;

/// Deepest element nesting a Word part may use, the same bound as `equations`.
const MAX_XML_DEPTH: usize = 512;
const NESTING_ERROR: &str = "DOCX XML exceeds nesting or namespace limits";
const MAX_NOTES: usize = 1_000;

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Paragraph {
    pub(super) text: String,
    /// Level 1 to 6 from a `HeadingN` paragraph style.
    pub(super) heading: Option<usize>,
}

/// Every paragraph of a document part in reading order, table cells included,
/// and each table as a grid of trimmed cell text.
#[derive(Debug, Default)]
pub(super) struct Body {
    pub(super) paragraphs: Vec<Paragraph>,
    pub(super) tables: Vec<Vec<Vec<String>>>,
}

#[derive(Default)]
struct OpenParagraph {
    paragraph: Paragraph,
    /// Paragraphs that finished inside this one, such as text box contents.
    nested: Vec<Paragraph>,
}

#[derive(Default)]
struct OpenTable {
    grid: Vec<Vec<String>>,
    row: Option<Vec<String>>,
    cell: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum Frame {
    Other,
    /// Deleted text, moved-away text, a separator note, or an unread branch.
    Hidden,
    Text,
    Paragraph,
    Alternate {
        read: bool,
    },
    /// A branch being read, with the count of visible text before it.
    Branch {
        before: usize,
    },
    Table,
    Row,
    Cell,
    Note,
}

#[derive(Default)]
struct Walk {
    stack: Vec<Frame>,
    hidden: usize,
    in_text: bool,
    /// Pieces of non-blank text read so far, to tell an empty branch.
    visible_count: usize,
    open_paragraphs: Vec<OpenParagraph>,
    open_tables: Vec<OpenTable>,
    paragraphs: Vec<Paragraph>,
    tables: Vec<Vec<Vec<String>>>,
    notes: Vec<String>,
    notes_seen: usize,
    finished: bool,
}

/// The heading level of a `w:pStyle` value such as `Heading2`.
fn heading_level(style: &[u8]) -> Option<usize> {
    let digits = style
        .strip_prefix(b"Heading")
        .or_else(|| style.strip_prefix(b"heading"))?;
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let level: usize = std::str::from_utf8(digits).ok()?.parse().unwrap_or(1);
    Some(level.clamp(1, 6))
}

/// Separator and continuation stubs (`w:type` other than `normal`) are layout
/// scaffolding, not notes.
fn is_substantive_note(element: &BytesStart<'_>) -> bool {
    element
        .try_get_attribute("w:type")
        .ok()
        .flatten()
        .is_none_or(|kind| kind.value.as_ref() == b"normal")
}

/// The characters an event inside `w:t` or `m:t` contributes.
fn character_data(event: &Event<'_>) -> Option<String> {
    match event {
        Event::Text(e) => e.decode().ok().map(|value| value.into_owned()),
        Event::CData(e) => e.decode().ok().map(|value| value.into_owned()),
        Event::GeneralRef(e) => {
            if let Ok(Some(c)) = e.resolve_char_ref() {
                return Some(c.to_string());
            }
            let encoded = format!("&{};", e.decode().ok()?);
            quick_xml::escape::unescape(&encoded)
                .ok()
                .map(|value| value.into_owned())
        }
        _ => None,
    }
}

impl Walk {
    fn visible(&mut self, value: &str) {
        if !value.trim().is_empty() {
            self.visible_count += 1;
        }
        if let Some(open) = self.open_paragraphs.last_mut() {
            open.paragraph.text.push_str(value);
        }
        if let Some(cell) = self
            .open_tables
            .last_mut()
            .and_then(|table| table.cell.as_mut())
        {
            cell.push_str(value);
        }
    }

    fn record_heading(&mut self, element: &BytesStart<'_>) {
        let Some(open) = self.open_paragraphs.last_mut() else {
            return;
        };
        if open.paragraph.heading.is_none() {
            open.paragraph.heading = element
                .try_get_attribute("w:val")
                .ok()
                .flatten()
                .and_then(|style| heading_level(&style.value));
        }
    }

    fn hide(&mut self) -> Frame {
        self.hidden += 1;
        Frame::Hidden
    }

    /// Read a branch unless an earlier one of its block already had text.
    fn branch(&mut self) -> Frame {
        match self.stack.last() {
            Some(Frame::Alternate { read: true }) => self.hide(),
            Some(Frame::Alternate { read: false }) => Frame::Branch {
                before: self.visible_count,
            },
            _ => Frame::Other,
        }
    }

    fn open_note(&mut self, element: &BytesStart<'_>, empty: bool) {
        if self.notes_seen == MAX_NOTES {
            self.finished = true;
            return;
        }
        self.notes_seen += 1;
        if empty {
            return;
        }
        let frame = if is_substantive_note(element) {
            // Text a note holds outside any paragraph still belongs to it.
            self.open_paragraphs.push(OpenParagraph::default());
            Frame::Note
        } else {
            self.hide()
        };
        self.stack.push(frame);
    }

    fn open(
        &mut self,
        element: &BytesStart<'_>,
        empty: bool,
        note: Option<&[u8]>,
    ) -> Result<(), String> {
        if !empty && self.stack.len() >= MAX_XML_DEPTH {
            return Err(NESTING_ERROR.into());
        }
        if self.hidden > 0 {
            if !empty {
                self.stack.push(Frame::Other);
            }
            return Ok(());
        }
        let name = element.name();
        let name = name.as_ref();
        if note == Some(name) {
            self.open_note(element, empty);
            return Ok(());
        }
        match name {
            b"w:tab" => self.visible("\t"),
            b"w:br" | b"w:cr" => self.visible("\n"),
            b"w:pStyle" => self.record_heading(element),
            _ => {}
        }
        let in_row = self
            .open_tables
            .last()
            .is_some_and(|table| table.row.is_some());
        if empty {
            if name == b"w:tc" && in_row {
                if let Some(row) = self.open_tables.last_mut().and_then(|t| t.row.as_mut()) {
                    row.push(String::new());
                }
            }
            return Ok(());
        }
        let frame = match name {
            b"w:del" | b"w:moveFrom" => self.hide(),
            b"mc:AlternateContent" => Frame::Alternate { read: false },
            b"mc:Choice" | b"mc:Fallback" => self.branch(),
            b"w:t" | b"m:t" => {
                self.in_text = true;
                Frame::Text
            }
            b"w:p" => {
                self.open_paragraphs.push(OpenParagraph::default());
                Frame::Paragraph
            }
            b"w:tbl" => {
                self.open_tables.push(OpenTable::default());
                Frame::Table
            }
            b"w:tr" if !self.open_tables.is_empty() => {
                if let Some(table) = self.open_tables.last_mut() {
                    table.row = Some(Vec::new());
                }
                Frame::Row
            }
            b"w:tc" if in_row => {
                if let Some(table) = self.open_tables.last_mut() {
                    table.cell = Some(String::new());
                }
                Frame::Cell
            }
            _ => Frame::Other,
        };
        self.stack.push(frame);
        Ok(())
    }

    fn close_paragraph(&mut self) {
        let Some(open) = self.open_paragraphs.pop() else {
            return;
        };
        let finished = std::iter::once(open.paragraph).chain(open.nested);
        match self.open_paragraphs.last_mut() {
            Some(parent) => parent.nested.extend(finished),
            None => self.paragraphs.extend(finished),
        }
    }

    fn close_note(&mut self) {
        let Some(open) = self.open_paragraphs.pop() else {
            return;
        };
        let text = std::iter::once(open.paragraph)
            .chain(open.nested)
            .map(|paragraph| paragraph.text.trim().to_string())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() {
            self.notes.push(text);
        }
    }

    fn close(&mut self) {
        match self.stack.pop() {
            Some(Frame::Hidden) => self.hidden -= 1,
            Some(Frame::Text) => self.in_text = false,
            Some(Frame::Paragraph) => self.close_paragraph(),
            Some(Frame::Note) => self.close_note(),
            Some(Frame::Branch { before }) if self.visible_count > before => {
                if let Some(Frame::Alternate { read }) = self.stack.last_mut() {
                    *read = true;
                }
            }
            Some(Frame::Cell) => {
                if let Some(table) = self.open_tables.last_mut() {
                    if let (Some(cell), Some(row)) = (table.cell.take(), table.row.as_mut()) {
                        row.push(cell.trim().to_string());
                    }
                }
            }
            Some(Frame::Row) => {
                if let Some(table) = self.open_tables.last_mut() {
                    if let Some(row) = table.row.take().filter(|row| !row.is_empty()) {
                        table.grid.push(row);
                    }
                }
            }
            Some(Frame::Table) => {
                if let Some(table) = self.open_tables.pop() {
                    if !table.grid.is_empty() {
                        self.tables.push(table.grid);
                    }
                }
            }
            _ => {}
        }
    }
}

fn walk(xml: &str, note: Option<&[u8]>) -> Result<Walk, String> {
    let mut reader = Reader::from_str(xml);
    let mut walk = Walk::default();
    while !walk.finished {
        let event = reader
            .read_event()
            .map_err(|e| format!("Invalid DOCX XML: {e}"))?;
        match &event {
            Event::Start(element) => walk.open(element, false, note)?,
            Event::Empty(element) => walk.open(element, true, note)?,
            Event::End(_) => walk.close(),
            Event::DocType(_) => return Err("DOCX XML must not contain a DTD".into()),
            Event::Eof if walk.stack.is_empty() => break,
            Event::Eof => return Err("DOCX XML contains an unclosed element".into()),
            other if walk.in_text => {
                if let Some(value) = character_data(other) {
                    walk.visible(&value);
                }
            }
            _ => {}
        }
    }
    Ok(walk)
}

/// Paragraphs and tables of `word/document.xml`.
pub(super) fn body(xml: &str) -> Result<Body, String> {
    let walk = walk(xml, None)?;
    Ok(Body {
        paragraphs: walk.paragraphs,
        tables: walk.tables,
    })
}

/// The substantive notes of a footnotes or endnotes part, each joined into one
/// line. `element` is the note's tag name, such as `w:footnote`.
pub(super) fn notes(xml: &str, element: &[u8]) -> Result<Vec<String>, String> {
    Ok(walk(xml, Some(element))?.notes)
}

pub(super) fn text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    let mut in_text = false;
    let mut output = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"w:t" | b"m:t" => in_text = true,
                b"w:tab" => output.push('\t'),
                b"w:br" | b"w:cr" => output.push('\n'),
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"w:tab" => output.push('\t'),
                b"w:br" | b"w:cr" => output.push('\n'),
                _ => {}
            },
            Ok(Event::End(e)) if matches!(e.name().as_ref(), b"w:t" | b"m:t") => in_text = false,
            Ok(Event::Eof) | Err(_) => break,
            Ok(event) if in_text => {
                if let Some(value) = character_data(&event) {
                    output.push_str(&value);
                }
            }
            _ => {}
        }
    }
    output
}

pub(super) fn equations(
    xml: &str,
    byte_limit: usize,
    count_limit: usize,
) -> Result<(Vec<String>, usize), String> {
    let mut reader = Reader::from_str(xml);
    let mut namespaces: Vec<BTreeMap<String, String>> = vec![BTreeMap::new()];
    let mut capture: Option<(usize, Writer<Vec<u8>>)> = None;
    let mut output = Vec::new();
    let mut omitted = 0;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("Invalid DOCX XML: {e}"))?;
        match &event {
            Event::Start(e) => {
                let mut scope = namespaces.last().cloned().unwrap_or_default();
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| format!("Invalid DOCX attribute: {e}"))?;
                    if attr.key.as_ref() == b"xmlns" || attr.key.as_ref().starts_with(b"xmlns:") {
                        scope.insert(
                            String::from_utf8_lossy(attr.key.as_ref()).into_owned(),
                            String::from_utf8_lossy(&attr.value).into_owned(),
                        );
                    }
                }
                if namespaces.len() >= 512
                    || scope.len() > 128
                    || scope
                        .iter()
                        .map(|(key, value)| key.len() + value.len())
                        .sum::<usize>()
                        > 64 * 1024
                {
                    return Err("DOCX XML exceeds nesting or namespace limits".into());
                }
                namespaces.push(scope);
                if capture.is_none() && matches!(e.name().as_ref(), b"m:oMath" | b"m:oMathPara") {
                    let mut start = e.to_owned();
                    let present: Vec<_> = start
                        .attributes()
                        .filter_map(Result::ok)
                        .map(|a| a.key.as_ref().to_vec())
                        .collect();
                    for (key, value) in namespaces.last().unwrap() {
                        if !present.iter().any(|k| k == key.as_bytes()) {
                            start.push_attribute((key.as_str(), value.as_str()));
                        }
                    }
                    let mut writer = Writer::new(Vec::new());
                    writer
                        .write_event(Event::Start(start))
                        .map_err(|e| e.to_string())?;
                    capture = Some((namespaces.len(), writer));
                    continue;
                }
            }
            Event::End(_) => {
                if let Some((depth, writer)) = capture.as_mut() {
                    writer
                        .write_event(event.clone())
                        .map_err(|e| e.to_string())?;
                    if *depth == namespaces.len() {
                        let (_, writer) = capture.take().unwrap();
                        let bytes = writer.into_inner();
                        if bytes.len() <= byte_limit && output.len() < count_limit {
                            output.push(String::from_utf8(bytes).map_err(|e| e.to_string())?);
                        } else {
                            omitted += 1;
                        }
                    }
                    namespaces.pop();
                    continue;
                }
                namespaces.pop();
            }
            Event::DocType(_) => return Err("DOCX XML must not contain a DTD".into()),
            Event::Eof => break,
            _ => {}
        }
        if let Some((_, writer)) = capture.as_mut() {
            writer.write_event(event).map_err(|e| e.to_string())?;
        }
    }
    if capture.is_some() || namespaces.len() != 1 {
        return Err("DOCX XML contains an unclosed element".into());
    }
    Ok((output, omitted))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_namespace_stack_growth() {
        let xml = format!("{}{}", "<w:p>".repeat(600), "</w:p>".repeat(600));
        assert!(equations(&xml, 1000, 10).unwrap_err().contains("limits"));
    }
    #[test]
    fn preserves_text_separators_and_numeric_entities() {
        assert_eq!(text("<w:p><w:r><w:t>before</w:t><w:tab/><w:t>after</w:t><w:br/><w:t>next &#x3B1; &#945; &amp;</w:t></w:r></w:p>"), "before\tafter\nnext α α &");
        assert_eq!(
            text("<w:r><w:t>con</w:t></w:r><w:r><w:t>tiguous</w:t></w:r>"),
            "contiguous"
        );
    }
    #[test]
    fn math_is_balanced_namespaced_and_never_truncated() {
        let xml = r#"<w:document xmlns:w="word" xmlns:m="math"><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath><m:oMath><m:r><m:t>y</m:t></m:r></m:oMath></m:oMathPara></w:document>"#;
        let (math, omitted) = equations(xml, 1000, 10).unwrap();
        assert_eq!(math.len(), 1);
        assert_eq!(omitted, 0);
        assert!(math[0].ends_with("</m:oMathPara>"));
        assert!(math[0].contains("xmlns:m=\"math\""));
        assert_eq!(equations(&math[0], 1000, 10).unwrap().0.len(), 1);
        assert_eq!(equations(xml, 5, 10).unwrap(), (vec![], 1));
    }
}

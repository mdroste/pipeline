//! Word boxes from Poppler's `pdftotext -bbox-layout`, and quotation search
//! over them.
//!
//! Poppler reports boxes in points from the top-left of the page as it is
//! displayed (after `/Rotate`), relative to the media box. Converting them to
//! PDF user space is the annotator's job; this module only parses and
//! searches.

use super::quote_match::{Haystack, MatchKind, Needle};
use quick_xml::events::Event;
use quick_xml::Reader;

/// Bounded Poppler output; a long paper's layout XHTML is a few megabytes.
const MAX_LAYOUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_WORDS: usize = 2_000_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
    /// Index of the layout line on its page.
    pub line: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PageWords {
    /// 1-based page number in the PDF.
    pub number: u32,
    pub words: Vec<Word>,
}

/// One located quotation: the words it covers, grouped by layout line, as
/// display-space rectangles `(x_min, y_min, x_max, y_max)`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct LocatedQuote {
    pub lines: Vec<(f32, f32, f32, f32)>,
    pub kind: MatchKind,
}

fn attribute_f32(element: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<f32> {
    let value = element.try_get_attribute(name).ok().flatten()?;
    std::str::from_utf8(&value.value).ok()?.trim().parse().ok()
}

/// Parse `pdftotext -bbox-layout` XHTML. `first_page` is the page number of
/// the first `<page>` element (the `-f` argument).
pub fn parse_bbox_layout(xhtml: &str, first_page: u32) -> Result<Vec<PageWords>, String> {
    let mut reader = Reader::from_str(xhtml);
    let mut pages: Vec<PageWords> = Vec::new();
    let mut line = 0usize;
    let mut word: Option<(Word, usize)> = None;
    let mut total_words = 0usize;
    loop {
        let before = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|error| format!("Could not read Poppler's word layout: {error}"))?;
        let after = reader.buffer_position() as usize;
        match event {
            Event::Start(element) => match element.local_name().as_ref() {
                b"page" => {
                    let number = first_page + pages.len() as u32;
                    pages.push(PageWords {
                        number,
                        words: Vec::new(),
                    });
                    line = 0;
                }
                b"line" => line += 1,
                b"word" => {
                    let box_value = |name| attribute_f32(&element, name).unwrap_or(0.0);
                    word = Some((
                        Word {
                            text: String::new(),
                            x_min: box_value("xMin"),
                            y_min: box_value("yMin"),
                            x_max: box_value("xMax"),
                            y_max: box_value("yMax"),
                            line,
                        },
                        after,
                    ));
                }
                _ => {}
            },
            Event::End(element) if element.local_name().as_ref() == b"word" => {
                if let (Some((mut current, content_start)), Some(page)) =
                    (word.take(), pages.last_mut())
                {
                    let raw = &xhtml[content_start..before];
                    current.text = quick_xml::escape::unescape(raw)
                        .map(|text| text.into_owned())
                        .unwrap_or_else(|_| raw.to_string());
                    if !current.text.trim().is_empty() {
                        total_words += 1;
                        if total_words > MAX_WORDS {
                            return Err("The PDF has too many words to annotate safely".into());
                        }
                        page.words.push(current);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(pages)
}

impl PageWords {
    /// Page text as characters plus each character's word index. Words are
    /// joined by a space, except that a line-final hyphen joins directly to
    /// the next line's first word so hyphenated words can still match.
    fn characters(&self) -> (Vec<char>, Vec<Option<usize>>) {
        let mut characters = Vec::new();
        let mut owners = Vec::new();
        for (index, word) in self.words.iter().enumerate() {
            if let Some(previous) = index.checked_sub(1).map(|i| &self.words[i]) {
                let hyphenated = previous.line != word.line
                    && previous
                        .text
                        .ends_with(['-', '\u{2010}', '\u{2011}', '\u{00AD}']);
                if !hyphenated {
                    characters.push(' ');
                    owners.push(None);
                }
            }
            for character in word.text.chars() {
                characters.push(character);
                owners.push(Some(index));
            }
        }
        (characters, owners)
    }

    pub(super) fn searcher(&self) -> PageSearch<'_> {
        let (characters, owners) = self.characters();
        PageSearch {
            page: self,
            haystack: Haystack::new(&characters),
            owners,
        }
    }
}

pub(super) struct PageSearch<'a> {
    page: &'a PageWords,
    haystack: Haystack,
    owners: Vec<Option<usize>>,
}

impl PageSearch<'_> {
    pub fn locate(&self, needle: &Needle, kind: MatchKind) -> Option<LocatedQuote> {
        let found = self.haystack.find(needle, kind)?;
        let first = self.owners.get(found.start).copied().flatten()?;
        let last = self.owners.get(found.end - 1).copied().flatten()?;
        let mut lines: Vec<(usize, (f32, f32, f32, f32))> = Vec::new();
        for word in &self.page.words[first..=last] {
            match lines.last_mut() {
                Some((line, rect)) if *line == word.line => {
                    rect.0 = rect.0.min(word.x_min);
                    rect.1 = rect.1.min(word.y_min);
                    rect.2 = rect.2.max(word.x_max);
                    rect.3 = rect.3.max(word.y_max);
                }
                _ => lines.push((word.line, (word.x_min, word.y_min, word.x_max, word.y_max))),
            }
        }
        Some(LocatedQuote {
            lines: lines.into_iter().map(|(_, rect)| rect).collect(),
            kind: found.kind,
        })
    }
}

/// Run the bundled Poppler `pdftotext -bbox-layout` over a private copy of
/// the PDF. The copy keeps helper processes away from the user's folders
/// (on macOS a child process needs its own permission to read them).
pub fn pdftotext_bbox_layout(pdf: &[u8]) -> Result<Vec<PageWords>, String> {
    let program = crate::deps::resolve_command("pdftotext").ok_or(
        "Pipeline's PDF text tool (pdftotext) is unavailable, so it cannot find quotations in the PDF",
    )?;
    let staging = tempfile::Builder::new()
        .prefix("pipeline_pdf_annotate_")
        .tempdir()
        .map_err(|error| format!("Could not create a private working folder: {error}"))?;
    let input = staging.path().join("reviewed.pdf");
    std::fs::write(&input, pdf)
        .map_err(|error| format!("Could not prepare the PDF for reading: {error}"))?;
    let input_arg = input
        .to_str()
        .ok_or("The private working folder path is not valid UTF-8")?;
    let mut command = program.command(["-bbox-layout", "-enc", "UTF-8", input_arg, "-"]);
    command.env("PATH", crate::env::full_path());
    let output = crate::process::run_bounded_independent(
        &mut command,
        std::time::Duration::from_secs(180),
        MAX_LAYOUT_BYTES,
    )?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Poppler could not read the PDF's words: {}",
            detail.trim()
        ));
    }
    if output.stdout_truncated {
        return Err("The PDF's word layout is too large to annotate safely".into());
    }
    let xhtml = String::from_utf8_lossy(&output.stdout);
    parse_bbox_layout(&xhtml, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: &str = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "x"><html xmlns="http://www.w3.org/1999/xhtml"><body><doc>
<page width="612" height="792"><flow><block>
<line><word xMin="72" yMin="60" xMax="90" yMax="72">We</word><word xMin="93" yMin="60" xMax="112" yMax="72">find</word>
<word xMin="115" yMin="60" xMax="130" yMax="72">no</word><word xMin="133" yMin="60" xMax="160" yMax="72">pre-</word></line>
<line><word xMin="72" yMin="74" xMax="110" yMax="86">trends</word><word xMin="113" yMin="74" xMax="125" yMax="86">in</word>
<word xMin="128" yMin="74" xMax="170" yMax="86">R&amp;D.</word></line>
</block></flow></page>
<page width="612" height="792"></page></doc></body></html>"#;

    #[test]
    fn parses_pages_lines_and_entities() {
        let pages = parse_bbox_layout(LAYOUT, 3).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].number, 3);
        assert_eq!(pages[1].number, 4);
        assert_eq!(pages[0].words.len(), 7);
        assert_eq!(pages[0].words[6].text, "R&D.");
        assert_eq!(pages[0].words[4].line, 2);
    }

    #[test]
    fn locates_quotes_across_line_breaks_and_hyphenation() {
        let pages = parse_bbox_layout(LAYOUT, 1).unwrap();
        let search = pages[0].searcher();
        let needle = Needle::new("find no pre-trends in R&D").unwrap();
        let found = search.locate(&needle, MatchKind::Normalized).unwrap();
        assert_eq!(
            found.lines,
            vec![(93.0, 60.0, 160.0, 72.0), (72.0, 74.0, 170.0, 86.0)]
        );
        let loose = Needle::new("we find no pretrends").unwrap();
        assert!(search.locate(&loose, MatchKind::Normalized).is_none());
        let found = search
            .locate(&loose, MatchKind::IgnoringPunctuation)
            .unwrap();
        assert_eq!(found.lines.len(), 2);
    }
}

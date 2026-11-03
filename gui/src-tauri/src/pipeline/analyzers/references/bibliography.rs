//! Finds the bibliography in extracted document text and splits it into
//! entries. Segmentation is heuristic; each entry keeps its exact text and
//! node so the report can quote it and the evidence verifier can check it.

use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

const HEADINGS: &[&str] = &[
    "references",
    "bibliography",
    "works cited",
    "literature cited",
    "references and notes",
    "cited literature",
    "reference list",
];
/// A heading that starts with one of these ends the list, so "Appendix A:
/// Proofs" stops it as well as a bare "Appendix".
const STOP_HEADINGS: &[&str] = &[
    "appendix",
    "appendices",
    "online appendix",
    "web appendix",
    "internet appendix",
    "data appendix",
    "supplementary",
    "supplemental",
    "tables",
    "figures",
    // Sections Word extraction appends after the body.
    "extracted tables",
    "footnotes",
    "endnotes",
];
const MAX_STOP_HEADING_CHARS: usize = 100;
const MIN_ENTRY_CHARS: usize = 20;
const MAX_ENTRY_CHARS: usize = 1_500;
const MIN_LIST_ENTRIES: usize = 3;
const MAX_EARLIER_ENTRY_CHARS: usize = 600;
const ENTRY_YEAR_WINDOW_CHARS: usize = 200;
/// Lower-case words a name may hold before its year, as in "Bureau of Labor
/// Statistics (2020)". A sentence reaches a year with more.
const MAX_LOWERCASE_WORDS_BEFORE_YEAR: usize = 3;

/// One piece of ordered document text.
pub struct Block<'a> {
    pub node_id: &'a str,
    pub text: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntry {
    pub node_id: String,
    pub text: String,
}

fn heading_text(text: &str) -> String {
    // Extracted Markdown marks headings with `#`, or with bold or italics.
    let trimmed = text
        .trim()
        .trim_matches(['#', '*', '_'])
        .trim()
        .trim_end_matches(':');
    // Drop leading numbering such as "7", "VII.", or "A.".
    let without_number = trimmed
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ')
        .trim();
    let without_roman = without_number
        .split_once(['.', ' '])
        .filter(|(prefix, _)| {
            !prefix.is_empty() && prefix.chars().all(|c| "IVXLCivxlc".contains(c))
        })
        .map(|(_, rest)| rest.trim())
        .unwrap_or(without_number);
    without_roman.to_lowercase()
}

fn is_heading(text: &str, candidates: &[&str]) -> bool {
    text.trim().chars().count() <= 40 && candidates.contains(&heading_text(text).as_str())
}

/// True for a heading-length line that ends the list. A dated line stays a
/// candidate, because a reference can begin with these words.
fn is_stop_heading(text: &str) -> bool {
    static NUMBERED: OnceLock<Regex> = OnceLock::new();
    let numbered = NUMBERED
        .get_or_init(|| Regex::new(r"^(?:table|figure|fig\.)\s+[a-z]?\d+\b").expect("valid"));
    if text.trim().chars().count() > MAX_STOP_HEADING_CHARS || text.trim().contains('\n') {
        return false;
    }
    let heading = heading_text(text);
    let named = STOP_HEADINGS.iter().any(|stop| {
        heading.strip_prefix(stop).is_some_and(|rest| {
            !rest
                .chars()
                .next()
                .is_some_and(|next| next.is_alphanumeric())
        })
    });
    (named || numbered.is_match(&heading)) && year(text).is_none()
}

/// Only text that could be a reference may leave the computer in a lookup.
fn looks_like_reference(text: &str) -> bool {
    let unnumbered = numbered_marker().replace(text, "");
    year(text).is_some()
        || doi(text).is_some()
        || arxiv(text).is_some()
        || author_start().is_match(&unnumbered)
}

fn page_marker() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?im)^\s*<!--\s*PAGE\s+\d+\s*-->\s*$").expect("valid"))
}

fn numbered_marker() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^\s*(?:\[\d{1,4}\]|\d{1,4}\.)\s+").expect("valid"))
}

fn author_start() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        // "Card, D." / "Card, David" / "D. Card" / "van der Berg, J."
        Regex::new(r"^(?:(?:van|von|de|der|da|del|di|le|la)\s+)*[A-Z][A-Za-z'\u{00C0}-\u{024F}\-]+,\s|^[A-Z]\.\s?(?:[A-Z]\.\s?)*[A-Z][a-z]")
            .expect("valid")
    })
}

/// Splits one block into entries at numbered markers, or at line starts
/// that look like a new author list after a line ending in a period.
fn split_block(text: &str) -> Vec<String> {
    let markers: Vec<usize> = numbered_marker()
        .find_iter(text)
        .map(|found| found.start())
        .collect();
    if markers.len() >= 2 {
        let mut parts = Vec::new();
        for (index, start) in markers.iter().enumerate() {
            let end = markers.get(index + 1).copied().unwrap_or(text.len());
            parts.push(text[*start..end].trim().to_string());
        }
        return parts;
    }
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let ends_entry = current.trim_end().ends_with('.');
        if !current.is_empty() && ends_entry && author_start().is_match(line) {
            parts.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Every reference list a document holds, in document order.
#[derive(Debug, Default)]
pub struct Lists {
    pub entries: Vec<RawEntry>,
    /// Earlier sections with a bibliography heading that do not read as a list.
    pub unread: usize,
    /// Earlier lists that ended at text that does not read as a reference.
    pub cut_short: usize,
}

/// Text after an earlier list can be the next chapter with no heading this
/// reader recognizes, so an earlier list keeps only text that reads as an entry.
fn reads_as_entry(text: &str) -> bool {
    let unnumbered = numbered_marker().replace(text, "");
    looks_like_reference(text)
        && text.chars().count() <= MAX_EARLIER_ENTRY_CHARS
        && (author_start().is_match(&unnumbered)
            || doi(text).is_some()
            || arxiv(text).is_some()
            || opens_with_a_name_and_year(&unnumbered))
}

fn opens_with_a_name_and_year(text: &str) -> bool {
    year_pattern().find(text).is_some_and(|found| {
        let opening = &text[..found.start()];
        let lowercase = opening
            .split_whitespace()
            .filter(|word| word.chars().next().is_some_and(char::is_lowercase))
            .count();
        opening.chars().count() <= ENTRY_YEAR_WINDOW_CHARS
            && lowercase <= MAX_LOWERCASE_WORDS_BEFORE_YEAR
    })
}

/// The short text after an earlier list that ends it, such as "Chapter 2". A
/// fragment ending in a period continues the entry before it.
fn ends_earlier_list(text: &str) -> bool {
    !text.trim_end().ends_with('.')
}

#[derive(Default)]
struct Section {
    entries: Vec<RawEntry>,
    cut_short: bool,
}

/// Reads the blocks under one heading. `earlier` applies the stricter rule for
/// any list that is not the document's last.
fn read_section(blocks: &[Block<'_>], earlier: bool) -> Section {
    let mut section = Section::default();
    for block in blocks {
        if is_stop_heading(block.text) || is_heading(block.text, HEADINGS) {
            break;
        }
        if earlier && block.text.trim_start().starts_with('#') {
            break;
        }
        for part in split_block(block.text) {
            if part.chars().count() < MIN_ENTRY_CHARS {
                if earlier && ends_earlier_list(&part) {
                    return section;
                }
                if let Some(last) = section.entries.last_mut() {
                    last.text.push(' ');
                    last.text.push_str(&part);
                }
                continue;
            }
            if earlier && !reads_as_entry(&part) {
                section.cut_short = true;
                return section;
            }
            if !looks_like_reference(&part) {
                continue;
            }
            section.entries.push(RawEntry {
                node_id: block.node_id.to_string(),
                text: part.chars().take(MAX_ENTRY_CHARS).collect(),
            });
        }
    }
    section
}

/// The same identity the duplicate check uses, so a work repeated in two
/// chapter bibliographies is read once rather than reported as a duplicate.
fn entry_identity(text: &str) -> String {
    doi(text).unwrap_or_else(|| {
        super::super::fold::fold(text, super::super::fold::Fold::Latex)
            .text
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect()
    })
}

/// Every bibliography in the document, which is more than one for a paper with
/// an online appendix or a thesis with chapter bibliographies. The last heading
/// followed by at least three entries is the document's list. Headings before
/// it add their entries under the stricter rule above. Paragraphs that cannot
/// be references are left out rather than sent to a lookup.
pub fn lists(blocks: &[Block<'_>]) -> Lists {
    let starts: Vec<usize> = blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| is_heading(block.text, HEADINGS))
        .map(|(index, _)| index)
        .collect();
    let Some((last_list, last_entries)) =
        starts.iter().enumerate().rev().find_map(|(at, start)| {
            let section = read_section(&blocks[start + 1..], false);
            (section.entries.len() >= MIN_LIST_ENTRIES).then_some((at, section.entries))
        })
    else {
        return Lists::default();
    };
    let mut found = Lists::default();
    let mut sections = Vec::with_capacity(last_list + 1);
    for start in &starts[..last_list] {
        let after = &blocks[start + 1..];
        let section = read_section(after, true);
        if section.entries.len() >= MIN_LIST_ENTRIES {
            found.cut_short += usize::from(section.cut_short);
            sections.push(section.entries);
        } else if read_section(after, false).entries.len() >= MIN_LIST_ENTRIES {
            found.unread += 1;
        }
    }
    sections.push(last_entries);
    let mut earlier_lists = HashSet::new();
    for section in sections {
        let identities: Vec<String> = section
            .iter()
            .map(|entry| entry_identity(&entry.text))
            .collect();
        for (entry, identity) in section.into_iter().zip(&identities) {
            if !earlier_lists.contains(identity) {
                found.entries.push(entry);
            }
        }
        earlier_lists.extend(identities);
    }
    found
}

#[cfg(test)]
pub fn entries(blocks: &[Block<'_>]) -> Vec<RawEntry> {
    lists(blocks).entries
}

/// Splits plain text into blocks at blank lines and page markers, for runs
/// without paragraph-level nodes. A marker never joins an entry's text.
pub fn text_blocks(text: &str) -> Vec<Block<'_>> {
    let mut blocks = Vec::new();
    for page in page_marker().split(text) {
        let page = page.trim();
        let mut start = 0;
        for (offset, _) in page.match_indices("\n\n") {
            if offset > start {
                blocks.push(Block {
                    node_id: "",
                    text: &page[start..offset],
                });
            }
            start = offset + 2;
        }
        if start < page.len() {
            blocks.push(Block {
                node_id: "",
                text: &page[start..],
            });
        }
    }
    // A heading glued to its first entry by a single newline is split off.
    let mut split = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block.text.split_once('\n') {
            Some((first, rest)) if is_heading(first, HEADINGS) || is_stop_heading(first) => {
                split.push(Block {
                    node_id: block.node_id,
                    text: first,
                });
                split.push(Block {
                    node_id: block.node_id,
                    text: rest,
                });
            }
            _ => split.push(block),
        }
    }
    split
}

/// Old serial item identifiers put `<...>` inside the DOI, as in
/// `11:1<1::AID-JAE372>3.0.CO;2-D`, and links carry those brackets escaped.
/// Brackets count as part of the DOI only with that shape, a colon inside and
/// the `3.0.CO;2` version after, so a tag still ends the DOI.
pub fn doi(text: &str) -> Option<String> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(
            r#"(?i)\b(10\.\d{4,9}/(?:[^\s"<>{}]|<[^\s"<>{}/]*:[^\s"<>{}/]*>\d\.\d\.[a-z0-9]{2};\d)+)"#,
        )
        .expect("valid")
    });
    let unescaped = ["&lt;", "%3C", "%3c"]
        .iter()
        .fold(text.to_string(), |text, escaped| text.replace(escaped, "<"));
    let unescaped = ["&gt;", "%3E", "%3e"]
        .iter()
        .fold(unescaped, |text, escaped| text.replace(escaped, ">"));
    let found = pattern.captures(&unescaped)?[1].to_string();
    let trimmed = found.trim_end_matches(['.', ',', ';', ')', ']']);
    Some(trimmed.to_ascii_lowercase())
}

/// Whether an entry labels a DOI, as `doi:` or a `doi.org` link, whether or not
/// what follows is a valid one.
pub fn names_doi(text: &str) -> bool {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(r"(?i)\bdoi\s*:\s*\S|\bdoi\.org/\S").expect("valid"))
        .is_match(text)
}

pub fn arxiv(text: &str) -> Option<String> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"(?i)(?:arxiv[:\s]*|arxiv\.org/abs/)(\d{4}\.\d{4,5})").expect("valid")
    });
    Some(pattern.captures(text)?[1].to_string())
}

fn year_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\b(1[89]\d{2}|20\d{2})[a-z]?\b").expect("valid"))
}

pub fn year(text: &str) -> Option<u16> {
    year_pattern()
        .captures(text)
        .and_then(|captures| captures[1].parse().ok())
}

/// First author's family name from the start of an entry.
pub fn first_family(text: &str) -> Option<String> {
    static INITIALS_FIRST: OnceLock<Regex> = OnceLock::new();
    let initials_first = INITIALS_FIRST.get_or_init(|| {
        Regex::new(r"^(?:[A-Z]\.\s?-?)+\s*((?:(?:van|von|de|der|da|del|di|le|la)\s+)*[A-Z][A-Za-z'\u{00C0}-\u{024F}\-]+)")
            .expect("valid")
    });
    let text = numbered_marker().replace(text, "");
    let text = text.trim();
    let clean = |word: &str| {
        let word = word.trim_matches(|c: char| !c.is_alphabetic());
        (word.chars().count() >= 2).then(|| word.to_string())
    };
    // "D. Card and A. Krueger"
    if let Some(captures) = initials_first.captures(text) {
        return captures[1].split_whitespace().last().and_then(clean);
    }
    // "Card, David" / "Card D" / "David Card and Alan Krueger" / "van der Berg"
    let head = text.split([',', '(']).next()?.trim();
    let head = head.split(" and ").next().unwrap_or(head).trim();
    let words: Vec<&str> = head.split_whitespace().collect();
    let last = words.last()?;
    let last_is_initials = last.trim_end_matches('.').chars().all(|c| c.is_uppercase())
        && last.trim_end_matches('.').chars().count() <= 3;
    if words.len() > 1 && last_is_initials {
        clean(words[0])
    } else {
        clean(last)
    }
}

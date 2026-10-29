//! Finds the bibliography in extracted document text and splits it into
//! entries. Segmentation is heuristic; each entry keeps its exact text and
//! node so the report can quote it and the evidence verifier can check it.

use regex::Regex;
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
const STOP_HEADINGS: &[&str] = &[
    "appendix",
    "appendices",
    "online appendix",
    "supplementary material",
    "supplementary materials",
    "tables",
    "figures",
];
const MIN_ENTRY_CHARS: usize = 20;
const MAX_ENTRY_CHARS: usize = 1_500;

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
    let trimmed = text.trim().trim_end_matches(':');
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

/// Entries after the last bibliography heading that is followed by at
/// least three entries, stopping at an appendix-like heading.
pub fn entries(blocks: &[Block<'_>]) -> Vec<RawEntry> {
    let starts: Vec<usize> = blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| is_heading(block.text, HEADINGS))
        .map(|(index, _)| index)
        .collect();
    for start in starts.into_iter().rev() {
        let mut found = Vec::new();
        for block in &blocks[start + 1..] {
            if is_heading(block.text, STOP_HEADINGS) || is_heading(block.text, HEADINGS) {
                break;
            }
            for part in split_block(block.text) {
                let length = part.chars().count();
                if length < MIN_ENTRY_CHARS {
                    if let Some(last) = found.last_mut() {
                        let last: &mut RawEntry = last;
                        last.text.push(' ');
                        last.text.push_str(&part);
                    }
                    continue;
                }
                found.push(RawEntry {
                    node_id: block.node_id.to_string(),
                    text: part.chars().take(MAX_ENTRY_CHARS).collect(),
                });
            }
        }
        if found.len() >= 3 {
            return found;
        }
    }
    Vec::new()
}

/// Splits plain text into blocks at blank lines, for runs without nodes.
pub fn text_blocks(text: &str) -> Vec<Block<'_>> {
    let mut blocks = Vec::new();
    let mut start = 0;
    for (offset, _) in text.match_indices("\n\n") {
        if offset > start {
            blocks.push(Block {
                node_id: "",
                text: &text[start..offset],
            });
        }
        start = offset + 2;
    }
    if start < text.len() {
        blocks.push(Block {
            node_id: "",
            text: &text[start..],
        });
    }
    // A heading glued to its first entry by a single newline is split off.
    let mut split = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block.text.split_once('\n') {
            Some((first, rest))
                if is_heading(first, HEADINGS) || is_heading(first, STOP_HEADINGS) =>
            {
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

pub fn doi(text: &str) -> Option<String> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern =
        PATTERN.get_or_init(|| Regex::new(r#"(?i)\b(10\.\d{4,9}/[^\s"<>{}]+)"#).expect("valid"));
    let found = pattern.captures(text)?[1].to_string();
    let trimmed = found.trim_end_matches(['.', ',', ';', ')', ']']);
    Some(trimmed.to_ascii_lowercase())
}

pub fn arxiv(text: &str) -> Option<String> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"(?i)(?:arxiv[:\s]*|arxiv\.org/abs/)(\d{4}\.\d{4,5})").expect("valid")
    });
    Some(pattern.captures(text)?[1].to_string())
}

pub fn year(text: &str) -> Option<u16> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern =
        PATTERN.get_or_init(|| Regex::new(r"\b(1[89]\d{2}|20\d{2})[a-z]?\b").expect("valid"));
    pattern
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

//! A bounded BibTeX reader for reference checks: entry type, key, and the
//! fields a lookup needs. Macros and `@string` expansion are not performed;
//! unreadable entries are skipped rather than guessed.

use std::collections::BTreeMap;

const MAX_ENTRIES: usize = 2_000;
const MAX_FIELD_CHARS: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibEntry {
    pub kind: String,
    pub key: String,
    pub fields: BTreeMap<String, String>,
}

/// Reads a balanced `{...}` or `"..."` value starting at `start`, returning
/// the inner text and the index after it.
fn delimited(chars: &[char], start: usize) -> Option<(String, usize)> {
    let open = *chars.get(start)?;
    let mut value = String::new();
    if open == '"' {
        let mut depth = 0i32;
        let mut i = start + 1;
        while i < chars.len() {
            match chars[i] {
                '{' => depth += 1,
                '}' => depth -= 1,
                '"' if depth == 0 => return Some((value, i + 1)),
                _ => {}
            }
            value.push(chars[i]);
            i += 1;
        }
        return None;
    }
    if open != '{' {
        return None;
    }
    let mut depth = 1i32;
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((value, i + 1));
                }
            }
            _ => {}
        }
        value.push(chars[i]);
        i += 1;
    }
    None
}

fn skip_space(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

/// Parses the fields of one entry body (after the key and comma).
fn fields(chars: &[char], mut i: usize, end: usize) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    while i < end {
        i = skip_space(chars, i);
        let name_start = i;
        while i < end && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '-' | ':')) {
            i += 1;
        }
        if i == name_start {
            i += 1;
            continue;
        }
        let name: String = chars[name_start..i]
            .iter()
            .collect::<String>()
            .to_lowercase();
        i = skip_space(chars, i);
        if chars.get(i) != Some(&'=') {
            continue;
        }
        i = skip_space(chars, i + 1);
        let mut value = String::new();
        // Concatenations (`a # "b"`) keep each literal part in order.
        loop {
            match chars.get(i) {
                Some('{') | Some('"') => match delimited(chars, i) {
                    Some((part, next)) => {
                        value.push_str(&part);
                        i = next;
                    }
                    None => return fields,
                },
                Some(c) if c.is_alphanumeric() => {
                    let begin = i;
                    while i < end && (chars[i].is_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    value.extend(&chars[begin..i]);
                }
                _ => break,
            }
            i = skip_space(chars, i);
            if chars.get(i) == Some(&'#') {
                i = skip_space(chars, i + 1);
                continue;
            }
            break;
        }
        if !value.is_empty() {
            fields.insert(name, value.chars().take(MAX_FIELD_CHARS).collect());
        }
        i = skip_space(chars, i);
        if chars.get(i) == Some(&',') {
            i += 1;
        }
    }
    fields
}

pub fn parse(text: &str) -> Vec<BibEntry> {
    let chars: Vec<char> = text.chars().collect();
    let mut entries = Vec::new();
    let mut i = 0;
    while i < chars.len() && entries.len() < MAX_ENTRIES {
        if chars[i] != '@' {
            i += 1;
            continue;
        }
        let kind_start = i + 1;
        let mut j = kind_start;
        while j < chars.len() && chars[j].is_alphabetic() {
            j += 1;
        }
        let kind: String = chars[kind_start..j]
            .iter()
            .collect::<String>()
            .to_lowercase();
        j = skip_space(&chars, j);
        if kind.is_empty() || !matches!(chars.get(j), Some('{') | Some('(')) {
            i = j.max(i + 1);
            continue;
        }
        if matches!(kind.as_str(), "comment" | "preamble" | "string") {
            i = j + 1;
            continue;
        }
        // The entry body ends at the brace matching the opening one.
        let Some((body, next)) = (if chars[j] == '{' {
            delimited(&chars, j)
        } else {
            chars[j + 1..].iter().position(|c| *c == ')').map(|offset| {
                (
                    chars[j + 1..j + 1 + offset].iter().collect(),
                    j + offset + 2,
                )
            })
        }) else {
            break;
        };
        let body: Vec<char> = body.chars().collect();
        let comma = body.iter().position(|c| *c == ',').unwrap_or(body.len());
        let key: String = body[..comma].iter().collect::<String>().trim().to_string();
        if !key.is_empty() {
            entries.push(BibEntry {
                kind,
                key,
                fields: fields(&body, (comma + 1).min(body.len()), body.len()),
            });
        }
        i = next;
    }
    entries
}

/// Field text without braces, LaTeX commands, ties, or doubled whitespace.
/// Case is kept; matching folds it separately.
pub fn plain(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' | '}' | '$' => {}
            '~' => out.push(' '),
            '\\' => {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_ascii_alphabetic() {
                    j += 1;
                }
                if j == i + 1 && j < chars.len() {
                    // Accent commands (\"o, \'e) drop the accent; escaped
                    // symbols (\&, \%) keep the symbol.
                    if !matches!(chars[j], '"' | '\'' | '`' | '^' | '~' | '=' | '.') {
                        out.push(chars[j]);
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            c => out.push(c),
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Family names from a BibTeX `author` field.
pub fn family_names(authors: &str) -> Vec<String> {
    authors
        .split(" and ")
        .map(str::trim)
        .filter(|author| !author.is_empty() && !author.eq_ignore_ascii_case("others"))
        .map(|author| {
            let author = plain(author);
            match author.split_once(',') {
                Some((family, _)) => family.trim().to_string(),
                None => author
                    .split_whitespace()
                    .last()
                    .unwrap_or_default()
                    .to_string(),
            }
        })
        .filter(|family| !family.is_empty())
        .collect()
}

/// Citation keys used by `\cite`-family commands in LaTeX source.
pub fn cited_keys(source: &str) -> Vec<String> {
    let pattern =
        regex::Regex::new(r"\\(?:[a-zA-Z]*cite[a-zA-Z]*|nocite)\*?(?:\[[^\]]*\]){0,2}\{([^}]*)\}")
            .expect("valid citation pattern");
    let mut keys = Vec::new();
    for capture in pattern.captures_iter(source) {
        for key in capture[1].split(',') {
            let key = key.trim();
            if !key.is_empty() && key != "*" && !keys.iter().any(|known: &String| known == key) {
                keys.push(key.to_string());
            }
        }
    }
    keys
}

//! Text folding for deterministic matching. Each folded byte keeps the byte
//! offset of the original character it came from, so a match in folded text
//! maps back to an exact range of the source.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fold {
    /// Whitespace runs collapse to one space; zero-width characters drop.
    Whitespace,
    /// Also soft hyphens, line-break hyphenation, typographic quotes and
    /// dashes, ligatures, ellipsis, full-width ASCII forms, and case.
    Typographic,
    /// Also LaTeX commands, braces, comments, ties, and math delimiters.
    Latex,
}

pub struct Folded {
    pub text: String,
    /// `map[i]` is the original byte offset that produced folded byte `i`.
    pub map: Vec<usize>,
}

fn zero_width(c: char) -> bool {
    matches!(
        c,
        '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}'
    )
}

fn typographic(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' | '\u{2032}' | '\u{b4}' | '`' => "'",
        '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{201f}' | '\u{2033}' | '\u{ab}' | '\u{bb}' => {
            "\""
        }
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
        | '\u{2212}' | '\u{2043}' => "-",
        '\u{2026}' => "...",
        '\u{fb00}' => "ff",
        '\u{fb01}' => "fi",
        '\u{fb02}' => "fl",
        '\u{fb03}' => "ffi",
        '\u{fb04}' => "ffl",
        '\u{fb05}' | '\u{fb06}' => "st",
        _ => return None,
    })
}

struct Writer {
    text: String,
    map: Vec<usize>,
    pending_space: Option<usize>,
}

impl Writer {
    fn push_str(&mut self, value: &str, origin: usize) {
        if let Some(space_origin) = self.pending_space.take() {
            if !self.text.is_empty() {
                self.text.push(' ');
                self.map.push(space_origin);
            }
        }
        for _ in 0..value.len() {
            self.map.push(origin);
        }
        self.text.push_str(value);
    }

    fn push_char(&mut self, value: char, origin: usize) {
        let mut buffer = [0u8; 4];
        self.push_str(value.encode_utf8(&mut buffer), origin);
    }

    fn space(&mut self, origin: usize) {
        if self.pending_space.is_none() {
            self.pending_space = Some(origin);
        }
    }
}

pub fn fold(input: &str, mode: Fold) -> Folded {
    let typographic_mode = mode != Fold::Whitespace;
    let latex = mode == Fold::Latex;
    let mut out = Writer {
        text: String::with_capacity(input.len()),
        map: Vec::with_capacity(input.len()),
        pending_space: None,
    };
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (offset, c) = chars[i];
        if latex {
            match c {
                '%' => {
                    while i < chars.len() && chars[i].1 != '\n' {
                        i += 1;
                    }
                    continue;
                }
                '\\' => {
                    let mut j = i + 1;
                    if j < chars.len() && chars[j].1.is_ascii_alphabetic() {
                        while j < chars.len() && chars[j].1.is_ascii_alphabetic() {
                            j += 1;
                        }
                        if j < chars.len() && chars[j].1 == '*' {
                            j += 1;
                        }
                        out.space(offset);
                        i = j;
                        continue;
                    }
                    if j < chars.len() {
                        if chars[j].1 == '\\' {
                            out.space(offset);
                        } else {
                            out.push_char(chars[j].1, chars[j].0);
                        }
                        i = j + 1;
                        continue;
                    }
                }
                '{' | '}' | '$' => {
                    i += 1;
                    continue;
                }
                '~' => {
                    out.space(offset);
                    i += 1;
                    continue;
                }
                '`' | '\'' if chars.get(i + 1).map(|next| next.1) == Some(c) => {
                    out.push_str("\"", offset);
                    i += 2;
                    continue;
                }
                '-' => {
                    out.push_str("-", offset);
                    while i < chars.len() && chars[i].1 == '-' {
                        i += 1;
                    }
                    continue;
                }
                _ => {}
            }
        }
        if c.is_whitespace() {
            out.space(offset);
            i += 1;
            continue;
        }
        if zero_width(c) {
            i += 1;
            continue;
        }
        if typographic_mode {
            if c == '\u{ad}' {
                i += 1;
                continue;
            }
            // "exam-\n  ple" -> "example": a hyphen followed by a line break
            // and a lowercase letter is line-break hyphenation.
            if matches!(c, '-' | '\u{2010}') {
                let mut j = i + 1;
                let mut newline = false;
                while j < chars.len() && chars[j].1.is_whitespace() {
                    newline |= chars[j].1 == '\n';
                    j += 1;
                }
                if newline && j < chars.len() && chars[j].1.is_lowercase() {
                    i = j;
                    continue;
                }
            }
            if let Some(replacement) = typographic(c) {
                out.push_str(replacement, offset);
                i += 1;
                continue;
            }
            if ('\u{ff01}'..='\u{ff5e}').contains(&c) {
                let ascii = char::from_u32(c as u32 - 0xfee0).unwrap_or(c);
                out.push_char(ascii.to_ascii_lowercase(), offset);
                i += 1;
                continue;
            }
            for lower in c.to_lowercase() {
                out.push_char(lower, offset);
            }
            i += 1;
            continue;
        }
        out.push_char(c, offset);
        i += 1;
    }
    Folded {
        text: out.text,
        map: out.map,
    }
}

/// Alphanumeric word tokens with their byte offsets.
pub fn tokens(text: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut start: Option<usize> = None;
    for (offset, c) in text.char_indices() {
        if c.is_alphanumeric() {
            start.get_or_insert(offset);
        } else if let Some(begin) = start.take() {
            result.push((begin, &text[begin..offset]));
        }
    }
    if let Some(begin) = start {
        result.push((begin, &text[begin..]));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_fold_collapses_runs_and_maps_offsets() {
        let folded = fold("  a \n\t b\u{200b}c ", Fold::Whitespace);
        assert_eq!(folded.text, "a bc");
        assert_eq!(folded.map, vec![2, 3, 7, 11]);
    }

    #[test]
    fn typographic_fold_handles_ligatures_quotes_dashes_and_hyphenation() {
        let folded = fold("The \u{fb01}rm\u{2019}s inter-\n  national \u{201c}Effect\u{201d} \u{2014} large\u{2026}", Fold::Typographic);
        assert_eq!(
            folded.text,
            "the firm's international \"effect\" - large..."
        );
    }

    #[test]
    fn hyphenation_keeps_hyphens_before_capitals_and_within_lines() {
        assert_eq!(fold("low-income", Fold::Typographic).text, "low-income");
        assert_eq!(fold("post-\nWar", Fold::Typographic).text, "post- war");
    }

    #[test]
    fn latex_fold_removes_markup() {
        let folded = fold(
            "We find \\emph{large} effects~\\citep{a} ``here'' --- $x$ 50\\% % note\nnext",
            Fold::Latex,
        );
        assert_eq!(folded.text, "we find large effects a \"here\" - x 50% next");
    }

    #[test]
    fn tokens_split_on_non_alphanumerics() {
        assert_eq!(tokens("a-b c1"), vec![(0, "a"), (2, "b"), (4, "c1")]);
    }
}

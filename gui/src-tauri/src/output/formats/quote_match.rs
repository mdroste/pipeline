//! Locate model-supplied quotations in document text.
//!
//! Matching is deliberately conservative. Both sides are folded the same way:
//! whitespace runs collapse to one space, soft hyphens and zero-width
//! characters disappear, typographic quotes and dashes become ASCII, the
//! ellipsis and common ligatures expand, and letters are lower-cased. A
//! second, looser pass compares only letters and digits, which tolerates
//! punctuation and hyphenation differences. A quotation elided with "..." must
//! match its fragments in order within a short distance. Quotations shorter
//! than [`MIN_QUOTE_CHARS`] are never placed: they match too many passages.

pub(super) const MIN_QUOTE_CHARS: usize = 12;
/// Maximum folded characters between consecutive fragments of an elided quote.
const MAX_ELISION_GAP: usize = 1_500;
const MAX_FIRST_FRAGMENT_ATTEMPTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MatchKind {
    /// Equal after whitespace, case, quote, and dash folding.
    Normalized,
    /// Equal after also ignoring everything except letters and digits.
    IgnoringPunctuation,
}

/// A located quotation as a half-open range of source character indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct QuoteMatch {
    pub start: usize,
    pub end: usize,
    pub kind: MatchKind,
}

/// Folded text plus, for every folded character, its source character index.
#[derive(Debug, Default)]
struct Projection {
    text: String,
    /// Byte offset in `text` of each folded character.
    offsets: Vec<usize>,
    source: Vec<usize>,
}

impl Projection {
    fn push(&mut self, character: char, source: usize) {
        self.offsets.push(self.text.len());
        self.text.push(character);
        self.source.push(source);
    }

    fn index_of_byte(&self, byte: usize) -> usize {
        self.offsets.partition_point(|offset| *offset < byte)
    }

    fn find_from(&self, needle: &str, from: usize) -> Option<(usize, usize)> {
        let start_byte = self.offsets.get(from).copied()?;
        let found = self.text[start_byte..].find(needle)? + start_byte;
        let start = self.index_of_byte(found);
        Some((start, start + needle.chars().count()))
    }

    /// Find all fragments in order; returns folded indices [start, end).
    fn find_fragments(&self, fragments: &[String]) -> Option<(usize, usize)> {
        let (first, rest) = fragments.split_first()?;
        let mut from = 0usize;
        for _ in 0..MAX_FIRST_FRAGMENT_ATTEMPTS {
            let (start, mut end) = self.find_from(first, from)?;
            let mut complete = true;
            for fragment in rest {
                match self.find_from(fragment, end) {
                    Some((next_start, next_end)) if next_start - end <= MAX_ELISION_GAP => {
                        end = next_end;
                    }
                    _ => {
                        complete = false;
                        break;
                    }
                }
            }
            if complete {
                return Some((start, end));
            }
            from = start + 1;
        }
        None
    }

    fn source_range(&self, start: usize, end: usize) -> (usize, usize) {
        (self.source[start], self.source[end - 1] + 1)
    }
}

fn fold(character: char, emit: &mut impl FnMut(char)) {
    let expansion: &str = match character {
        '\u{00AD}' | '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' => "",
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '`' | '\u{00B4}' => "'",
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' | '\u{00AB}'
        | '\u{00BB}' => "\"",
        '\u{2010}'..='\u{2015}' | '\u{2212}' | '\u{FE58}' | '\u{FE63}' | '\u{FF0D}' => "-",
        '\u{2026}' => "...",
        '\u{FB00}' => "ff",
        '\u{FB01}' => "fi",
        '\u{FB02}' => "fl",
        '\u{FB03}' => "ffi",
        '\u{FB04}' => "ffl",
        '\u{FB05}' | '\u{FB06}' => "st",
        character if character.is_whitespace() => " ",
        character => {
            for lower in character.to_lowercase() {
                emit(lower);
            }
            return;
        }
    };
    expansion.chars().for_each(emit);
}

fn project(chars: impl IntoIterator<Item = char>) -> Projection {
    let mut projection = Projection::default();
    for (index, character) in chars.into_iter().enumerate() {
        fold(character, &mut |folded| {
            if folded == ' ' && (projection.text.is_empty() || projection.text.ends_with(' ')) {
                return;
            }
            projection.push(folded, index);
        });
    }
    if projection.text.ends_with(' ') {
        projection.text.pop();
        projection.offsets.pop();
        projection.source.pop();
    }
    projection
}

fn alphanumeric(projection: &Projection) -> Projection {
    let mut kept = Projection::default();
    for (character, source) in projection.text.chars().zip(&projection.source) {
        if character.is_alphanumeric() {
            kept.push(character, *source);
        }
    }
    kept
}

fn trim_to_alphanumeric(fragment: &str) -> String {
    fragment
        .trim_matches(|character: char| !character.is_alphanumeric())
        .to_string()
}

/// A quotation prepared once and searched in many haystacks.
#[derive(Debug, Clone)]
pub(super) struct Needle {
    fragments: Vec<String>,
    loose: Vec<String>,
}

impl Needle {
    /// `None` when the quotation is too short to place reliably.
    pub fn new(quote: &str) -> Option<Self> {
        let folded = project(quote.chars()).text.replace("[...]", "...");
        let fragments: Vec<String> = folded
            .split("...")
            .map(trim_to_alphanumeric)
            .filter(|fragment| !fragment.is_empty())
            .collect();
        let loose: Vec<String> = fragments
            .iter()
            .map(|fragment| fragment.chars().filter(|c| c.is_alphanumeric()).collect())
            .filter(|fragment: &String| !fragment.is_empty())
            .collect();
        let loose_chars: usize = loose.iter().map(|fragment| fragment.chars().count()).sum();
        if fragments.is_empty() || loose_chars < MIN_QUOTE_CHARS {
            return None;
        }
        Some(Self { fragments, loose })
    }
}

/// Document text prepared once for repeated quotation searches.
#[derive(Debug, Default)]
pub(super) struct Haystack {
    normalized: Projection,
    loose: Projection,
}

impl Haystack {
    pub fn new(chars: &[char]) -> Self {
        let normalized = project(chars.iter().copied());
        let loose = alphanumeric(&normalized);
        Self { normalized, loose }
    }

    pub fn find(&self, needle: &Needle, kind: MatchKind) -> Option<QuoteMatch> {
        let (projection, fragments) = match kind {
            MatchKind::Normalized => (&self.normalized, &needle.fragments),
            MatchKind::IgnoringPunctuation => (&self.loose, &needle.loose),
        };
        let (start, end) = projection.find_fragments(fragments)?;
        let (start, end) = projection.source_range(start, end);
        Some(QuoteMatch { start, end, kind })
    }

    /// Prefer a normalized match; fall back to the punctuation-blind pass.
    pub fn find_best(&self, needle: &Needle) -> Option<QuoteMatch> {
        self.find(needle, MatchKind::Normalized)
            .or_else(|| self.find(needle, MatchKind::IgnoringPunctuation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locate(haystack: &str, quote: &str) -> Option<(String, MatchKind)> {
        let chars: Vec<char> = haystack.chars().collect();
        let found = Haystack::new(&chars).find_best(&Needle::new(quote)?)?;
        Some((chars[found.start..found.end].iter().collect(), found.kind))
    }

    #[test]
    fn folds_whitespace_quotes_dashes_soft_hyphens_and_case() {
        let text =
            "As shown, the  “pre‑trend” test—which is\u{00A0}weak—fails in col\u{00AD}umn 3.";
        let (span, kind) = locate(
            text,
            "the \"pre-trend\" test-which is weak-fails in column 3",
        )
        .unwrap();
        assert_eq!(kind, MatchKind::Normalized);
        assert_eq!(
            span,
            "the  “pre‑trend” test—which is\u{00A0}weak—fails in col\u{00AD}umn 3"
        );
        let (span, _) = locate(text, "THE “PRE-TREND” TEST").unwrap();
        assert_eq!(span, "the  “pre‑trend” test");
    }

    #[test]
    fn falls_back_to_letters_and_digits_and_expands_ligatures() {
        let text = "We ﬁnd no evidence of pre-\ntrends in the event study.";
        let (span, kind) = locate(text, "find no evidence of pretrends").unwrap();
        assert_eq!(kind, MatchKind::IgnoringPunctuation);
        assert_eq!(span, "ﬁnd no evidence of pre-\ntrends");
    }

    #[test]
    fn elided_quotes_match_fragments_in_order() {
        let text =
            "The first stage is strong. Several robustness checks follow. The estimate is stable.";
        let (span, _) = locate(text, "The first stage is strong … the estimate is stable").unwrap();
        assert_eq!(span, text.trim_end_matches('.'));
        assert!(locate(text, "the estimate is stable ... the first stage is strong").is_none());
    }

    #[test]
    fn short_or_missing_quotes_are_not_placed() {
        assert!(Needle::new("Table 2").is_none());
        assert!(Needle::new("  “...”  ").is_none());
        assert!(locate("Nothing relevant here at all.", "a sentence that is absent").is_none());
    }

    #[test]
    fn surrounding_quote_marks_and_trailing_punctuation_are_ignored() {
        let text = "Our main result (Theorem 2) holds for all n.";
        let (span, kind) = locate(text, "“Theorem 2) holds for all n.”").unwrap();
        assert_eq!(kind, MatchKind::Normalized);
        assert_eq!(span, "Theorem 2) holds for all n");
    }
}

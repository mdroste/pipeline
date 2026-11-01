//! Keeps LaTeX away from the Markdown parser, which would read `*`, `_`, and
//! backslashes inside an equation as Markdown and change the mathematics. The
//! printed report restores protected math for KaTeX; the other exports show
//! it as the LaTeX source it was written in.

#[derive(Debug)]
pub(crate) struct ProtectedMath {
    pub(crate) token: String,
    pub(crate) latex: String,
    pub(crate) display: bool,
}

fn escaped_at(bytes: &[u8], index: usize) -> bool {
    let mut slashes = 0usize;
    let mut cursor = index;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        slashes += 1;
        cursor -= 1;
    }
    slashes % 2 == 1
}

fn math_close(source: &str, mut index: usize, delimiter_len: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            // The escaped character may be several bytes long.
            index += 1;
            index += source[index..].chars().next().map_or(0, char::len_utf8);
            continue;
        }
        if bytes[index] == b'$' && !escaped_at(bytes, index) {
            if delimiter_len == 2 {
                if bytes.get(index + 1) == Some(&b'$') {
                    return Some(index);
                }
            } else if bytes.get(index + 1) != Some(&b'$') {
                return Some(index);
            }
        }
        let character = source[index..].chars().next()?;
        index += character.len_utf8();
    }
    None
}

fn protect_math_chunk(
    source: &str,
    token_base: &str,
    protected: &mut Vec<ProtectedMath>,
) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut index = 0usize;
    while index < bytes.len() {
        // Copy code spans byte-for-byte. KaTeX auto-render ignores code tags,
        // and dollar signs in examples must never become equations.
        if bytes[index] == b'`' {
            let ticks = bytes[index..]
                .iter()
                .take_while(|byte| **byte == b'`')
                .count();
            let marker = "`".repeat(ticks);
            if let Some(relative_end) = source[index + ticks..].find(&marker) {
                let end = index + ticks + relative_end + ticks;
                output.push_str(&source[index..end]);
                index = end;
                continue;
            }
        }

        if bytes[index] == b'$' && !escaped_at(bytes, index) {
            let delimiter_len = usize::from(bytes.get(index + 1) == Some(&b'$')) + 1;
            let content_start = index + delimiter_len;
            if let Some(close) = math_close(source, content_start, delimiter_len) {
                let latex = &source[content_start..close];
                // Inline `$…$` must have non-whitespace ends and stay on one
                // line without crossing a code span: math_close scans raw
                // bytes, so "from $5 to $8 … `$x$`" would otherwise pair a
                // currency `$` with a later paragraph's or code span's `$`
                // and swallow the prose in between as an equation.
                let valid_inline_spacing = delimiter_len == 2
                    || (!latex.chars().next().is_some_and(char::is_whitespace)
                        && !latex.chars().next_back().is_some_and(char::is_whitespace)
                        && !latex.contains('\n')
                        && !latex.contains('`'));
                if !latex.trim().is_empty() && valid_inline_spacing {
                    let token = format!("{token_base}{}END", protected.len());
                    protected.push(ProtectedMath {
                        token: token.clone(),
                        latex: latex.to_string(),
                        display: delimiter_len == 2,
                    });
                    output.push_str(&token);
                    index = close + delimiter_len;
                    continue;
                }
            }
        }

        let character = source[index..]
            .chars()
            .next()
            .expect("index remains on a UTF-8 boundary");
        output.push(character);
        index += character.len_utf8();
    }
    output
}

/// Replace math with alphanumeric placeholders before pulldown-cmark sees it.
/// Otherwise `_`, `*`, and backslashes inside LaTeX can be consumed as Markdown
/// emphasis/escapes.
pub(crate) fn protect_math(markdown: &str) -> (String, Vec<ProtectedMath>) {
    let mut discriminator = 0usize;
    let token_base = loop {
        let candidate = format!("PIPELINEMATHPLACEHOLDER{discriminator}X");
        if !markdown.contains(&candidate) {
            break candidate;
        }
        discriminator += 1;
    };

    let mut output = String::with_capacity(markdown.len());
    let mut prose = String::new();
    let mut protected = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let flush_prose =
        |output: &mut String, prose: &mut String, protected: &mut Vec<ProtectedMath>| {
            if !prose.is_empty() {
                output.push_str(&protect_math_chunk(prose, &token_base, protected));
                prose.clear();
            }
        };

    for line in markdown.split_inclusive('\n') {
        if let Some((marker, count)) = crate::output::fence_marker(line) {
            flush_prose(&mut output, &mut prose, &mut protected);
            if let Some((open_marker, open_count)) = fence {
                if marker == open_marker && count >= open_count {
                    fence = None;
                }
            } else {
                fence = Some((marker, count));
            }
            output.push_str(line);
        } else if fence.is_some() {
            output.push_str(line);
        } else {
            prose.push_str(line);
        }
    }
    flush_prose(&mut output, &mut prose, &mut protected);
    (output, protected)
}

/// Protection for the exports that show equations as LaTeX source.
pub(super) fn protect_source_math(markdown: &str) -> (String, Vec<ProtectedMath>) {
    protect_math(&crate::output::normalize_math_delimiters(markdown))
}

/// Text with each placeholder back as its LaTeX source, delimiters included.
/// `escape` makes model-written math safe for the format it lands in.
pub(super) fn restore_source_math(
    text: &str,
    protected: &[ProtectedMath],
    escape: impl Fn(&str) -> String,
) -> String {
    let mut restored = text.to_string();
    for item in protected {
        let delimiter = if item.display { "$$" } else { "$" };
        let source = format!("{delimiter}{}{delimiter}", item.latex);
        restored = restored.replace(&item.token, &escape(&source));
    }
    restored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backslash_before_a_multibyte_character_ends_on_a_character() {
        for source in ["$5 \\é more", "$5 at C:\\Émile", "$5 \\“quoted\\”", "$5 \\"] {
            let (text, protected) = protect_math(source);
            assert_eq!(text, source);
            assert!(protected.is_empty());
        }
        let (_, protected) = protect_math("$a\\é b$ and more");
        assert_eq!(protected[0].latex, "a\\é b");
    }
}

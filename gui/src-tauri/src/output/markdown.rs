use regex::Regex;
use std::sync::LazyLock;

static PREAMBLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?mi)^(I(?:'|’)(?:ve|ll|m)|I (?:have|need|can|should|will)|Let me|Now (?:I|let)|OK[,.]|Alright[,.]|Here (?:is|are)|Looking at|After reading|Having read)[^\n]*\n*"
    ).expect("preamble regex is invalid")
});

pub(super) const RUN_DETAILS_START: &str = "<!-- PIPELINE RUN DETAILS START -->";
pub(super) const RUN_DETAILS_END: &str = "<!-- PIPELINE RUN DETAILS END -->";

/// Extract content between `<!-- REPORT START -->` and `<!-- REPORT END -->` markers.
/// Falls back to the full text (with preamble stripping) if markers aren't present.
pub fn strip_to_report(text: &str) -> String {
    let trimmed = text.trim();
    between_report_markers(trimmed).unwrap_or_else(|| strip_preamble(trimmed))
}

/// The report text of a current run. Its final step's content is a schema-backed
/// artifact rather than a narrated transcript, so an opening sentence is part of
/// the report; only explicit markers delimit it.
pub(crate) fn report_body(text: &str) -> String {
    let trimmed = text.trim();
    between_report_markers(trimmed).unwrap_or_else(|| trimmed.to_string())
}

fn between_report_markers(trimmed: &str) -> Option<String> {
    const START: &str = "<!-- REPORT START -->";
    const END: &str = "<!-- REPORT END -->";

    let start_pos = outside_fences(trimmed, START)?;
    let content_start = start_pos + START.len();
    if let Some(end_pos) = outside_fences(&trimmed[content_start..], END) {
        return Some(
            trimmed[content_start..content_start + end_pos]
                .trim()
                .to_string(),
        );
    }
    // START found but no END — take everything after START
    Some(trimmed[content_start..].trim().to_string())
}

/// Report markers quoted in literal evidence are content, not delimiters.
/// In particular a generated editorial report must preserve those exact bytes.
fn outside_fences(text: &str, needle: &str) -> Option<usize> {
    let mut fence = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if let Some((marker, count)) = fence_marker(line) {
            match fence {
                Some((open, length)) if marker == open && count >= length => fence = None,
                None => fence = Some((marker, count)),
                _ => {}
            }
        } else if fence.is_none() {
            if let Some(found) = line.find(needle) {
                return Some(offset + found);
            }
        }
        offset += line.len();
    }
    None
}

pub(crate) fn fence_marker(line: &str) -> Option<(u8, usize)> {
    let bytes = line.trim_start().as_bytes();
    let marker = *bytes.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let count = bytes.iter().take_while(|byte| **byte == marker).count();
    (count >= 3).then_some((marker, count))
}

fn normalize_math_line(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut output = String::with_capacity(line.len());
    let mut index = 0usize;
    let mut inline_ticks = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'`' {
            let count = bytes[index..]
                .iter()
                .take_while(|byte| **byte == b'`')
                .count();
            output.push_str(&line[index..index + count]);
            if inline_ticks == 0 {
                inline_ticks = count;
            } else if inline_ticks == count {
                inline_ticks = 0;
            }
            index += count;
            continue;
        }
        if inline_ticks == 0
            && bytes[index] == b'\\'
            && index + 1 < bytes.len()
            && (index == 0 || bytes[index - 1] != b'\\')
        {
            match bytes[index + 1] {
                b'(' | b')' => {
                    output.push('$');
                    index += 2;
                    continue;
                }
                b'[' | b']' => {
                    output.push_str("$$");
                    index += 2;
                    continue;
                }
                _ => {}
            }
        }
        let character = line[index..]
            .chars()
            .next()
            .expect("index remains on a UTF-8 boundary");
        output.push(character);
        index += character.len_utf8();
    }
    output
}

/// Canonicalize the two common LaTeX delimiter families to the dollar form
/// understood consistently by remark-math, the Markdown export path, and
/// KaTeX. Fenced and inline code are intentionally left untouched.
pub fn normalize_math_delimiters(markdown: &str) -> String {
    let mut output = String::with_capacity(markdown.len());
    let mut fence: Option<(u8, usize)> = None;
    for line in markdown.split_inclusive('\n') {
        if let Some((marker, count)) = fence_marker(line) {
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
        } else if let Some(normalized) = normalize_display_environment(line) {
            output.push_str(&normalized);
        } else {
            output.push_str(&normalize_math_line(line));
        }
    }
    output
}

fn normalize_display_environment(line: &str) -> Option<String> {
    static ENVIRONMENT_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"^([ \t]*)\\(begin|end)\{(equation\*?|displaymath|align\*?|gather\*?)\}[ \t]*(\r?\n)?$",
        )
        .expect("display-environment regex must compile")
    });
    let captures = ENVIRONMENT_RE.captures(line)?;
    let indent = captures.get(1).map(|value| value.as_str()).unwrap_or("");
    let boundary = captures.get(2)?.as_str();
    let environment = captures.get(3)?.as_str();
    let newline = captures.get(4).map(|value| value.as_str()).unwrap_or("");
    let replacement = match (boundary, environment) {
        ("begin", "equation" | "equation*" | "displaymath") => "$$",
        ("end", "equation" | "equation*" | "displaymath") => "$$",
        ("begin", "align" | "align*") => "$$\n\\begin{aligned}",
        ("end", "align" | "align*") => "\\end{aligned}\n$$",
        ("begin", "gather" | "gather*") => "$$\n\\begin{gathered}",
        ("end", "gather" | "gather*") => "\\end{gathered}\n$$",
        _ => return None,
    };
    Some(format!("{indent}{replacement}{newline}"))
}

/// Remove UI-only run telemetry from a report before public export.
pub fn clean_export_markdown(markdown: &str) -> String {
    let mut clean = markdown.to_string();
    while let Some(start) = outside_fences(&clean, RUN_DETAILS_START) {
        let Some(relative_end) =
            outside_fences(&clean[start + RUN_DETAILS_START.len()..], RUN_DETAILS_END)
        else {
            break;
        };
        let end = start + RUN_DETAILS_START.len() + relative_end + RUN_DETAILS_END.len();
        let mut next = String::with_capacity(clean.len() - (end - start));
        next.push_str(clean[..start].trim_end());
        next.push_str("\n\n");
        next.push_str(clean[end..].trim_start());
        clean = next;
    }
    clean
}

/// Strip LLM chain-of-thought preamble from output.
/// Removes lines like "I've now read the full paper. Let me compile..."
/// that appear before the actual content.
pub(super) fn strip_preamble(text: &str) -> String {
    let trimmed = text.trim();

    // Only strip from the beginning — don't touch mid-text occurrences.
    // Guard against infinite loops (e.g., zero-width matches).
    let mut result = trimmed;
    for _ in 0..50 {
        let stripped = result.trim_start();
        if let Some(m) = PREAMBLE_RE.find(stripped) {
            if m.start() == 0 && m.end() > 0 {
                result = &stripped[m.end()..];
                continue;
            }
        }
        break;
    }

    result.trim().to_string()
}

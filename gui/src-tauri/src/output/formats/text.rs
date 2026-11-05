//! Escaping and plain-text helpers shared by the report formats.

use super::math::{protect_source_math, restore_source_math};

/// True for characters XML 1.0 can carry. Model text may contain control
/// characters (for example a stray NUL); a single one makes a Word part or
/// an XHTML document unreadable, so they are dropped rather than escaped.
pub(super) fn xml_char_allowed(character: char) -> bool {
    matches!(character, '\t' | '\n' | '\r')
        || matches!(character, '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}')
        || character >= '\u{10000}'
}

/// Escape text for an XML/HTML text node or a double- or single-quoted
/// attribute value.
pub(super) fn escape_xml(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + text.len() / 8);
    for character in text
        .chars()
        .filter(|character| xml_char_allowed(*character))
    {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Shorten text to at most `limit` characters on a character boundary.
pub(super) fn truncate_chars(text: &str, limit: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut shortened: String = text.chars().take(limit.saturating_sub(1)).collect();
    shortened.push('…');
    shortened
}

/// Collapse internal whitespace for one-line labels and locators.
pub(super) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Render Markdown as readable plain text for Word comments and PDF notes,
/// where formatting markup would otherwise appear literally. Text that parses
/// as HTML stays: in a finding, `<treated>` is a word, and these notes are never
/// rendered. Link targets are kept in parentheses so nothing is hidden.
pub(super) fn markdown_to_plain(markdown: &str) -> String {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let mut output = String::with_capacity(markdown.len());
    let mut links: Vec<String> = Vec::new();
    let mut list_depth = 0usize;
    let (markdown, protected_math) = protect_source_math(markdown);
    for event in Parser::new_ext(&markdown, options) {
        match event {
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
                output.push_str(&text)
            }
            Event::SoftBreak => output.push(' '),
            Event::HardBreak => output.push('\n'),
            Event::Start(Tag::List(_)) => list_depth += 1,
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                if list_depth == 0 {
                    output.push('\n');
                }
            }
            Event::Start(Tag::Item) => {
                if !output.is_empty() && !output.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str(&"  ".repeat(list_depth.saturating_sub(1)));
                output.push_str("• ");
            }
            Event::End(TagEnd::Item) => {
                if !output.ends_with('\n') {
                    output.push('\n');
                }
            }
            Event::Start(Tag::Link { dest_url, .. }) => links.push(dest_url.to_string()),
            Event::End(TagEnd::Link) => {
                if let Some(target) = links.pop().filter(|target| !target.is_empty()) {
                    output.push_str(&format!(" ({target})"));
                }
            }
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::BlockQuote(_) | TagEnd::HtmlBlock,
            ) => output.push_str("\n\n"),
            Event::End(TagEnd::CodeBlock | TagEnd::TableRow | TagEnd::TableHead) => {
                output.push('\n')
            }
            Event::End(TagEnd::TableCell) => output.push_str(" | "),
            Event::Rule => output.push_str("\n\n"),
            _ => {}
        }
    }
    let output = restore_source_math(&output, &protected_math, str::to_string);
    let mut paragraphs = Vec::new();
    for block in output.split("\n\n") {
        let block = block
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        if !block.trim().is_empty() {
            paragraphs.push(block.trim().to_string());
        }
    }
    paragraphs.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_and_drops_characters_xml_cannot_carry() {
        assert_eq!(
            escape_xml("<b a=\"1\">'x' & y\u{0}\u{1b}</b>"),
            "&lt;b a=&quot;1&quot;&gt;&#39;x&#39; &amp; y&lt;/b&gt;"
        );
        assert_eq!(escape_xml("α\tβ\u{FFFE}"), "α\tβ");
    }

    #[test]
    fn plain_text_keeps_content_and_link_targets_but_drops_markup() {
        let plain = markdown_to_plain(
            "**Problem.** The *estimate* uses `x`.\n\n- first\n- [second](https://example.org)\n\n<script>alert(1)</script>",
        );
        assert!(plain.starts_with("Problem. The estimate uses x."));
        assert!(plain.contains("• first\n• second (https://example.org)"));
        assert!(plain.ends_with("<script>alert(1)</script>"));
        assert!(!plain.contains("**"));
    }

    #[test]
    fn plain_text_keeps_angle_bracket_words() {
        assert_eq!(
            markdown_to_plain("The <treated> group and n<N> units differ."),
            "The <treated> group and n<N> units differ."
        );
        assert_eq!(
            markdown_to_plain("<div>A block</div>\n\nNext **point**."),
            "<div>A block</div>\n\nNext point."
        );
    }

    #[test]
    fn plain_text_keeps_latex_byte_for_byte() {
        assert_eq!(
            markdown_to_plain(r"Both $x^*$ and $y^*$ and $\{a_i\}$ at $a\,b$, *real* emphasis."),
            r"Both $x^*$ and $y^*$ and $\{a_i\}$ at $a\,b$, real emphasis."
        );
    }

    #[test]
    fn truncation_respects_character_boundaries() {
        assert_eq!(truncate_chars("αβγδ", 3), "αβ…");
        assert_eq!(truncate_chars(" short ", 10), "short");
    }
}

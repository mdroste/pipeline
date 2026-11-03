//! Tells real HTML apart from report text that only parses as HTML.
//!
//! Markdown reads `the <treated> group`, `n<N> units`, and `if p<q and r>s` as
//! raw HTML, and a sanitizer or a Word converter would then remove the words.
//! Every report renderer asks this module instead, so one finding reads the
//! same in Word, HTML, and print. Nothing here is a security boundary: text that
//! is judged literal is escaped, and the HTML sanitizer still runs on the rest.

use pulldown_cmark::{CowStr, Event, TagEnd};
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Tags that are markup even when unclosed, because what they carry must not
/// reach a reader as text.
const ACTIVE_TAGS: &[&str] = &[
    "applet", "base", "embed", "form", "frame", "frameset", "iframe", "link", "math", "meta",
    "noscript", "object", "script", "style", "svg",
];
/// Elements with no closing tag.
const VOID_TAGS: &[&str] = &["br", "hr", "img", "wbr"];
/// Every other element a model might write. Several are single letters, so an
/// opening tag counts only when its own block closes it: `a<b>c` is an inequality.
const CONTAINER_TAGS: &[&str] = &[
    "a",
    "abbr",
    "acronym",
    "address",
    "article",
    "aside",
    "audio",
    "b",
    "bdi",
    "bdo",
    "big",
    "blockquote",
    "body",
    "button",
    "canvas",
    "caption",
    "center",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "dd",
    "del",
    "details",
    "dfn",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "em",
    "fieldset",
    "figcaption",
    "figure",
    "font",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "html",
    "i",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "main",
    "map",
    "mark",
    "menu",
    "meter",
    "nav",
    "nobr",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "picture",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "section",
    "select",
    "small",
    "span",
    "strike",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "tt",
    "u",
    "ul",
    "var",
    "video",
];
/// Attributes HTML allows without a value. Any other bare word after a tag
/// name is prose: `<q and r>`.
const BARE_ATTRIBUTES: &[&str] = &[
    "allowfullscreen",
    "async",
    "autofocus",
    "autoplay",
    "checked",
    "controls",
    "default",
    "defer",
    "disabled",
    "download",
    "hidden",
    "ismap",
    "loop",
    "multiple",
    "muted",
    "novalidate",
    "open",
    "readonly",
    "required",
    "reversed",
    "selected",
];

fn token_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r#"(?s)<!--.*?-->|<[!?][^>]*>|</?[A-Za-z][A-Za-z0-9-]*(?:\s(?:[^<>"']|"[^"]*"|'[^']*')*)?/?>"#,
        )
        .expect("valid")
    })
}

fn attribute_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r#"([^\s=/"'<>]+)(\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+))?"#).expect("valid")
    })
}

/// Raw HTML whose content is not report text. It is one pattern so the earliest
/// construct wins, and an unclosed one runs to the end of its block, as in HTML.
fn hidden_content_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(concat!(
            r"(?is)<!--.*?(?:-->|\z)",
            r"|<!\[CDATA\[.*?(?:\]\]>|\z)",
            r"|<\?.*?(?:\?>|\z)",
            r"|<script\b.*?(?:</script\s*>|\z)",
            r"|<style\b.*?(?:</style\s*>|\z)",
        ))
        .expect("valid")
    })
}

/// How the text of a block closes its tags, read from the end of the block back
/// so each opening tag pairs with a closing tag after it.
#[derive(Default)]
struct OpenTags {
    closing: HashMap<String, usize>,
}

impl OpenTags {
    fn is_markup(&mut self, token: &str) -> bool {
        if token.starts_with("<!") || token.starts_with("<?") {
            return true;
        }
        let closing = token.starts_with("</");
        let inner = token
            .trim_start_matches('<')
            .trim_start_matches('/')
            .trim_end_matches('>');
        let self_closed = inner.ends_with('/');
        let inner = inner.trim_end_matches('/');
        let (name, attributes) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
        let name = name.to_ascii_lowercase();
        let active = ACTIVE_TAGS.contains(&name.as_str());
        let void = VOID_TAGS.contains(&name.as_str());
        if !active && !void && !CONTAINER_TAGS.contains(&name.as_str()) {
            return false;
        }
        if closing {
            *self.closing.entry(name).or_default() += 1;
            return true;
        }
        let closed = self.closing.get_mut(&name).filter(|count| **count > 0);
        let is_closed = closed.is_some();
        if let Some(count) = closed {
            *count -= 1;
        }
        if active {
            return true;
        }
        let attributes: Vec<_> = attribute_pattern().captures_iter(attributes).collect();
        let valued = !attributes.is_empty()
            && attributes.iter().all(|attribute| {
                attribute.get(2).is_some()
                    || BARE_ATTRIBUTES.contains(&attribute[1].to_ascii_lowercase().as_str())
            });
        void || self_closed || valued || (attributes.is_empty() && is_closed)
    }

    /// Rewrites raw HTML, keeping or dropping each token by what it is.
    fn rewrite(&mut self, raw: &str, markup: fn(&str) -> &str, text: fn(&str) -> String) -> String {
        let tokens: Vec<_> = token_pattern().find_iter(raw).collect();
        let kinds: Vec<bool> = tokens
            .iter()
            .rev()
            .map(|token| self.is_markup(token.as_str()))
            .collect();
        let mut output = String::with_capacity(raw.len());
        let mut end = 0;
        for (token, is_markup) in tokens.iter().zip(kinds.into_iter().rev()) {
            output.push_str(&raw[end..token.start()]);
            if is_markup {
                output.push_str(markup(token.as_str()));
            } else {
                output.push_str(&text(token.as_str()));
            }
            end = token.end();
        }
        output.push_str(&raw[end..]);
        output
    }
}

/// The parser gives an HTML block one event per line, but a comment, a script,
/// or a tag can span lines, so a block is judged whole.
fn join_html_blocks<'a>(events: impl Iterator<Item = Event<'a>>) -> Vec<Event<'a>> {
    let mut joined = Vec::with_capacity(events.size_hint().0);
    let mut block = String::new();
    for event in events {
        if let Event::Html(line) = &event {
            block.push_str(line);
            continue;
        }
        if !block.is_empty() {
            joined.push(Event::Html(CowStr::from(std::mem::take(&mut block))));
        }
        joined.push(event);
    }
    if !block.is_empty() {
        joined.push(Event::Html(CowStr::from(block)));
    }
    joined
}

fn map_raw_html<'a>(
    events: impl Iterator<Item = Event<'a>>,
    mut rewrite: impl FnMut(&mut OpenTags, &str) -> String,
) -> Vec<Event<'a>> {
    let mut events = join_html_blocks(events);
    let mut tags = OpenTags::default();
    for event in events.iter_mut().rev() {
        match event {
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::TableCell
                | TagEnd::HtmlBlock,
            ) => tags.closing.clear(),
            Event::Html(raw) | Event::InlineHtml(raw) => {
                *raw = CowStr::from(rewrite(&mut tags, raw));
            }
            _ => {}
        }
    }
    events
}

/// Events for an HTML renderer: real markup stays for the sanitizer, and text
/// that only parses as HTML is escaped so the sanitizer leaves it alone.
pub(crate) fn escape_html_text<'a>(events: impl Iterator<Item = Event<'a>>) -> Vec<Event<'a>> {
    map_raw_html(events, |tags, raw| {
        tags.rewrite(
            raw,
            |token| token,
            |token| token.replace('<', "&lt;").replace('>', "&gt;"),
        )
    })
}

/// Events for a format that cannot show markup: each raw HTML event holds only
/// its text, with the markup, comments, and any script or style content gone.
pub(crate) fn html_to_plain_text<'a>(events: impl Iterator<Item = Event<'a>>) -> Vec<Event<'a>> {
    map_raw_html(events, |tags, raw| {
        let text = hidden_content_pattern().replace_all(raw, "");
        tags.rewrite(&text, |_| "", str::to_string)
    })
}

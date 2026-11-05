//! Readable text from sanitized HTML. HTML parsing decodes entities and repairs
//! malformed markup; block and table separators retain the document's meaning.
use html5ever::tokenizer::{BufferQueue, StartTag, Token, TokenSink, TokenSinkResult, Tokenizer};
use std::cell::{Cell, RefCell};

#[derive(Default)]
struct Text {
    output: RefCell<String>,
    pre: Cell<bool>,
    cells: Cell<usize>,
}

fn newline(output: &mut String) {
    while output.ends_with(' ') {
        output.pop();
    }
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
}

impl TokenSink for Text {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut out = self.output.borrow_mut();
        match token {
            Token::CharacterTokens(text) => {
                for c in text.chars() {
                    if !self.pre.get() && c.is_whitespace() {
                        if !out.is_empty() && !out.ends_with([' ', '\n', '\t']) {
                            out.push(' ');
                        }
                    } else {
                        out.push(c);
                    }
                }
            }
            Token::TagToken(tag) => {
                let name = tag.name.as_ref();
                match name {
                    "td" | "th" if tag.kind == StartTag => {
                        if self.cells.get() > 0 {
                            out.push('\t');
                        }
                        self.cells.set(self.cells.get() + 1);
                    }
                    "tr" => {
                        newline(&mut out);
                        self.cells.set(0);
                    }
                    "pre" => {
                        newline(&mut out);
                        self.pre.set(tag.kind == StartTag);
                    }
                    "p" | "div" | "section" | "article" | "header" | "footer" | "main"
                    | "aside" | "nav" | "blockquote" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                    | "li" | "ul" | "ol" | "dl" | "dt" | "dd" | "table" | "caption" => {
                        newline(&mut out)
                    }
                    "br" | "hr" if tag.kind == StartTag => out.push('\n'),
                    _ => {}
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

pub(super) fn from_html(html: &str) -> String {
    // Keep structural tags through sanitization; scripts, styles and comments
    // are removed before the tokenizer sees any text.
    let clean = ammonia::Builder::default().clean(html).to_string();
    let input = BufferQueue::default();
    input.push_back(clean.into());
    let tokenizer = Tokenizer::new(Text::default(), Default::default());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    tokenizer
        .sink
        .output
        .into_inner()
        .trim_matches([' ', '\n'])
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_table_columns_rows_and_heading_separate() {
        assert_eq!(from_html("<h1>Output</h1><table><tr><th>Year</th><th>GDP</th></tr><tr><td>2024</td><td>100</td></tr><tr><td>2025</td><td>102</td></tr></table>"),
            "Output\nYear\tGDP\n2024\t100\n2025\t102");
        assert_eq!(
            from_html("<table><tr><td></td><td>100</td></tr></table>"),
            "\t100"
        );
    }
    #[test]
    fn repairs_html_decodes_entities_and_excludes_non_content() {
        assert_eq!(from_html("<p>A &amp; B &lt; 5<p>Next<br>line<ul><li>One<li>Two</ul><script>secret()</script><style>secret</style><!-- hidden -->"),
            "A & B < 5\nNext\nline\nOne\nTwo");
        assert_eq!(
            from_html("<p>GDP is <b>100</b> units.</p><pre>x  y\n  z</pre>"),
            "GDP is 100 units.\nx  y\n  z"
        );
    }
}

//! A small WordprocessingML body builder with a Markdown converter.
//!
//! Only structure that Word renders predictably is produced: styled
//! paragraphs, bold/italic/strike/code runs, simple bullet or numbered list
//! prefixes, block quotes, code blocks, and plain tables. Raw HTML in model
//! output is dropped; link targets are written out after the link text.

use super::text::escape_xml;

#[derive(Debug, Clone, Default)]
pub(super) struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub color: Option<&'static str>,
    /// Half-points (Word's `w:sz`).
    pub size: Option<u32>,
}

impl Run {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn bold(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: true,
            ..Self::default()
        }
    }

    fn write(&self, xml: &mut String) {
        if self.text.is_empty() {
            return;
        }
        xml.push_str("<w:r>");
        let mut properties = String::new();
        if self.code {
            properties
                .push_str(r#"<w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Consolas"/>"#);
        }
        if self.bold {
            properties.push_str("<w:b/>");
        }
        if self.italic {
            properties.push_str("<w:i/>");
        }
        if self.strike {
            properties.push_str("<w:strike/>");
        }
        if let Some(color) = self.color {
            properties.push_str(&format!(r#"<w:color w:val="{color}"/>"#));
        }
        if let Some(size) = self.size.or(self.code.then_some(19)) {
            properties.push_str(&format!(r#"<w:sz w:val="{size}"/>"#));
        }
        if !properties.is_empty() {
            xml.push_str("<w:rPr>");
            xml.push_str(&properties);
            xml.push_str("</w:rPr>");
        }
        for (line_number, line) in self.text.split('\n').enumerate() {
            if line_number > 0 {
                xml.push_str("<w:br/>");
            }
            for (part_number, part) in line.split('\t').enumerate() {
                if part_number > 0 {
                    xml.push_str("<w:tab/>");
                }
                if !part.is_empty() {
                    xml.push_str(r#"<w:t xml:space="preserve">"#);
                    xml.push_str(&escape_xml(part));
                    xml.push_str("</w:t>");
                }
            }
        }
        xml.push_str("</w:r>");
    }
}

#[derive(Debug, Default)]
pub(super) struct Body {
    xml: String,
}

impl Body {
    pub fn into_xml(self) -> String {
        self.xml
    }

    pub fn paragraph(&mut self, style: Option<&str>, indent: u32, runs: &[Run]) {
        self.xml.push_str("<w:p>");
        if style.is_some() || indent > 0 {
            self.xml.push_str("<w:pPr>");
            if let Some(style) = style {
                self.xml
                    .push_str(&format!(r#"<w:pStyle w:val="{}"/>"#, escape_xml(style)));
            }
            if indent > 0 {
                self.xml.push_str(&format!(
                    r#"<w:ind w:left="{}" w:hanging="360"/>"#,
                    indent * 360
                ));
            }
            self.xml.push_str("</w:pPr>");
        }
        for run in runs {
            run.write(&mut self.xml);
        }
        self.xml.push_str("</w:p>");
    }

    pub fn text(&mut self, style: Option<&str>, text: &str) {
        self.paragraph(style, 0, &[Run::plain(text)]);
    }

    fn table(&mut self, rows: &[Vec<Vec<Run>>], header_rows: usize) {
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
        let width = 9_360 / columns;
        self.xml.push_str(concat!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders>"#,
            r#"<w:top w:val="single" w:sz="4" w:space="0" w:color="8A9298"/>"#,
            r#"<w:bottom w:val="single" w:sz="4" w:space="0" w:color="8A9298"/>"#,
            r#"<w:insideH w:val="single" w:sz="2" w:space="0" w:color="D7D9D8"/>"#,
            r#"</w:tblBorders></w:tblPr><w:tblGrid>"#
        ));
        for _ in 0..columns {
            self.xml.push_str(&format!(r#"<w:gridCol w:w="{width}"/>"#));
        }
        self.xml.push_str("</w:tblGrid>");
        for (row_number, row) in rows.iter().enumerate() {
            self.xml.push_str("<w:tr>");
            for column in 0..columns {
                self.xml.push_str(&format!(
                    r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/></w:tcPr>"#
                ));
                let mut runs = row.get(column).cloned().unwrap_or_default();
                if row_number < header_rows {
                    runs.iter_mut().for_each(|run| run.bold = true);
                }
                self.paragraph(None, 0, &runs);
                self.xml.push_str("</w:tc>");
            }
            self.xml.push_str("</w:tr>");
        }
        self.xml.push_str("</w:tbl>");
        // Word expects a paragraph between a table and what follows it.
        self.paragraph(None, 0, &[]);
    }

    /// Append Markdown as Word paragraphs. Headings start at `heading_base`
    /// (1 maps `#` to Heading1).
    pub fn markdown(&mut self, markdown: &str, heading_base: u32) {
        Converter::new(self, heading_base).run(markdown);
    }
}

struct Converter<'a> {
    body: &'a mut Body,
    heading_base: u32,
    runs: Vec<Run>,
    style: Option<String>,
    bold: usize,
    italic: usize,
    strike: usize,
    quote_depth: usize,
    lists: Vec<Option<u64>>,
    item_prefix: Option<String>,
    code_block: Option<String>,
    links: Vec<String>,
    table: Option<Vec<Vec<Vec<Run>>>>,
    header_rows: usize,
}

impl<'a> Converter<'a> {
    fn new(body: &'a mut Body, heading_base: u32) -> Self {
        Self {
            body,
            heading_base,
            runs: Vec::new(),
            style: None,
            bold: 0,
            italic: 0,
            strike: 0,
            quote_depth: 0,
            lists: Vec::new(),
            item_prefix: None,
            code_block: None,
            links: Vec::new(),
            table: None,
            header_rows: 0,
        }
    }

    fn push_text(&mut self, text: &str, code: bool) {
        if text.is_empty() {
            return;
        }
        let run = Run {
            text: text.to_string(),
            bold: self.bold > 0,
            italic: self.italic > 0,
            strike: self.strike > 0,
            code,
            ..Run::default()
        };
        if let Some(table) = self.table.as_mut() {
            if let Some(cell) = table.last_mut().and_then(|row| row.last_mut()) {
                cell.push(run);
            }
            return;
        }
        if let Some(prefix) = self.item_prefix.take() {
            self.runs.push(Run::plain(prefix));
        }
        self.runs.push(run);
    }

    fn flush(&mut self) {
        if let Some(prefix) = self.item_prefix.take() {
            self.runs.insert(0, Run::plain(prefix));
        }
        if self.runs.is_empty() {
            return;
        }
        let runs = std::mem::take(&mut self.runs);
        let indent = self.lists.len() as u32;
        let style = self.style.take().or_else(|| {
            if self.quote_depth > 0 {
                Some("Quote".to_string())
            } else if indent > 0 {
                Some("ListParagraph".to_string())
            } else {
                None
            }
        });
        self.body.paragraph(style.as_deref(), indent, &runs);
    }

    fn run(mut self, markdown: &str) {
        use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        for event in Parser::new_ext(markdown, options) {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    self.flush();
                    let level = (level as u32 + self.heading_base - 1).clamp(1, 3);
                    self.style = Some(format!("Heading{level}"));
                }
                Event::End(TagEnd::Heading(_) | TagEnd::Paragraph) => self.flush(),
                Event::Start(Tag::Paragraph) => {}
                Event::Start(Tag::BlockQuote(_)) => {
                    self.flush();
                    self.quote_depth += 1;
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    self.flush();
                    self.quote_depth = self.quote_depth.saturating_sub(1);
                }
                Event::Start(Tag::CodeBlock(_)) => {
                    self.flush();
                    self.code_block = Some(String::new());
                }
                Event::End(TagEnd::CodeBlock) => {
                    let code = self.code_block.take().unwrap_or_default();
                    for line in code.trim_end_matches('\n').split('\n') {
                        self.body.paragraph(Some("Code"), 0, &[Run::plain(line)]);
                    }
                }
                Event::Start(Tag::List(start)) => {
                    self.flush();
                    self.lists.push(start);
                }
                Event::End(TagEnd::List(_)) => {
                    self.flush();
                    self.lists.pop();
                }
                Event::Start(Tag::Item) => {
                    self.flush();
                    let prefix = match self.lists.last_mut() {
                        Some(Some(number)) => {
                            let prefix = format!("{number}.\t");
                            *number += 1;
                            prefix
                        }
                        _ => "•\t".to_string(),
                    };
                    self.item_prefix = Some(prefix);
                }
                Event::End(TagEnd::Item) => self.flush(),
                Event::Start(Tag::Strong) => self.bold += 1,
                Event::End(TagEnd::Strong) => self.bold = self.bold.saturating_sub(1),
                Event::Start(Tag::Emphasis) => self.italic += 1,
                Event::End(TagEnd::Emphasis) => self.italic = self.italic.saturating_sub(1),
                Event::Start(Tag::Strikethrough) => self.strike += 1,
                Event::End(TagEnd::Strikethrough) => self.strike = self.strike.saturating_sub(1),
                Event::Start(Tag::Link { dest_url, .. }) => self.links.push(dest_url.to_string()),
                Event::End(TagEnd::Link) => {
                    if let Some(target) = self.links.pop().filter(|target| !target.is_empty()) {
                        self.push_text(&format!(" ({target})"), false);
                    }
                }
                Event::Start(Tag::Image { .. }) => self.push_text("[Image: ", false),
                Event::End(TagEnd::Image) => self.push_text("]", false),
                Event::Start(Tag::Table(_)) => {
                    self.flush();
                    self.table = Some(Vec::new());
                    self.header_rows = 0;
                }
                Event::Start(Tag::TableHead) => {
                    self.header_rows = 1;
                    if let Some(table) = self.table.as_mut() {
                        table.push(Vec::new());
                    }
                }
                Event::Start(Tag::TableRow) => {
                    if let Some(table) = self.table.as_mut() {
                        table.push(Vec::new());
                    }
                }
                Event::Start(Tag::TableCell) => {
                    if let Some(row) = self.table.as_mut().and_then(|table| table.last_mut()) {
                        row.push(Vec::new());
                    }
                }
                Event::End(TagEnd::Table) => {
                    if let Some(rows) = self.table.take() {
                        self.body.table(&rows, self.header_rows);
                    }
                }
                Event::Text(text) => {
                    if let Some(code) = self.code_block.as_mut() {
                        code.push_str(&text);
                    } else {
                        self.push_text(&text, false);
                    }
                }
                Event::Code(text) => self.push_text(&text, true),
                Event::SoftBreak => self.push_text(" ", false),
                Event::HardBreak => self.push_text("\n", false),
                Event::Rule => {
                    self.flush();
                    self.body.paragraph(None, 0, &[]);
                }
                Event::FootnoteReference(label) => self.push_text(&format!("[{label}]"), false),
                Event::TaskListMarker(done) => {
                    self.push_text(if done { "[x] " } else { "[ ] " }, false)
                }
                _ => {}
            }
        }
        self.flush();
    }
}

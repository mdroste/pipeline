//! Streaming worksheet reader: cells become per-column statistics and an
//! optional preview window; rows are never collected for the whole sheet.
use super::{attr, bad, push_text, Limited};
use crate::workbench::data::{
    csv::missing as missing_token,
    profile::{cell_text, ColumnStats, Window, MAX_COLUMNS, PREVIEW_COLUMNS, PREVIEW_ROWS},
    MissingCell,
};
use crate::workbench::store::{WorkbenchError, WorkbenchResult};
use quick_xml::{events::Event, Reader};
use std::collections::{BTreeMap, HashSet};
use std::io::{BufReader, Read};

const MAX_SHEET_XML: u64 = 512 * 1024 * 1024;
const MAX_ROWS: usize = 1_048_576;
const MAX_FORMATS_PER_COLUMN: usize = 4;

pub(super) fn column_letters(mut col: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}
/// Zero-based column of a reference such as `AB12`.
fn reference_column(r: &str) -> WorkbenchResult<usize> {
    let letters = r
        .bytes()
        .take_while(u8::is_ascii_alphabetic)
        .collect::<Vec<_>>();
    if letters.is_empty() || letters.len() > 3 {
        return Err(bad(format!("cell reference {r} is invalid")));
    }
    let col = letters.iter().fold(0usize, |n, b| {
        n * 26 + usize::from(b.to_ascii_uppercase() - b'A') + 1
    }) - 1;
    if col >= 16_384 {
        return Err(bad(format!("cell reference {r} is invalid")));
    }
    Ok(col)
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Empty,
    Number(String, usize),
    Text(String),
    Bool(bool),
    Error(String),
}
impl Value {
    fn blank(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Text(s) => s.trim().is_empty(),
            _ => false,
        }
    }
    fn header(&self) -> Option<String> {
        match self {
            Self::Number(s, _) | Self::Text(s) if !s.trim().is_empty() => Some(s.trim().into()),
            Self::Bool(b) => Some(if *b { "TRUE" } else { "FALSE" }.into()),
            _ => None,
        }
    }
}
#[derive(Default)]
struct CellState {
    col: usize,
    kind: Option<String>,
    style: usize,
    text: String,
    has_value: bool,
    formula: bool,
}
fn classify(cell: CellState, shared: &[String], at: &str) -> WorkbenchResult<Value> {
    let text = cell.text;
    Ok(match cell.kind.as_deref() {
        Some("s") if !cell.has_value => Value::Empty,
        Some("s") => Value::Text(
            text.trim()
                .parse::<usize>()
                .ok()
                .and_then(|i| shared.get(i))
                .cloned()
                .ok_or_else(|| bad(format!("cell {at} refers to a missing shared string")))?,
        ),
        Some("inlineStr" | "str" | "d") => Value::Text(text),
        Some("b") => match text.trim() {
            "1" | "true" => Value::Bool(true),
            "0" | "false" => Value::Bool(false),
            "" => Value::Empty,
            _ => return Err(bad(format!("cell {at} has an invalid boolean"))),
        },
        Some("e") => Value::Error(text.trim().to_string()),
        Some("n") | None => {
            let t = text.trim();
            if t.is_empty() {
                Value::Empty
            } else if t.parse::<f64>().is_ok_and(f64::is_finite) {
                Value::Number(t.to_string(), cell.style)
            } else {
                return Err(bad(format!("cell {at} has an invalid number")));
            }
        }
        Some(other) => return Err(bad(format!("cell {at} has unknown type {other}"))),
    })
}

/// Worksheet accumulator: the first non-empty row is the header and its
/// first to last named cells are the table's columns. Empty rows inside the
/// table count as all-missing; trailing empty rows are dropped.
pub(super) struct Sheet<'a> {
    formats: &'a [String],
    pub header_row: Option<usize>,
    /// Sheet column of the first table column.
    first: usize,
    pub names: Vec<String>,
    pub stats: Vec<ColumnStats>,
    pub number_formats: Vec<BTreeMap<String, usize>>,
    pub rows: usize,
    last_row: usize,
    pub formulas: usize,
    pub window: Option<Window>,
}
impl<'a> Sheet<'a> {
    pub fn new(formats: &'a [String], window: Option<Window>) -> Self {
        Self {
            formats,
            header_row: None,
            first: 0,
            names: Vec::new(),
            stats: Vec::new(),
            number_formats: Vec::new(),
            rows: 0,
            last_row: 0,
            formulas: 0,
            window,
        }
    }
    fn header(&mut self, number: usize, cells: &[(usize, Value)]) -> WorkbenchResult<()> {
        let named = |(c, v): &&(usize, Value)| (!v.blank()).then_some(*c);
        let first = cells.iter().find_map(|c| named(&c)).unwrap_or(0);
        let last = cells.iter().rev().find_map(|c| named(&c)).unwrap_or(0);
        let width = last + 1 - first;
        if width > MAX_COLUMNS {
            return Err(WorkbenchError::invalid(format!(
                "This worksheet's header has {width} columns; Pipeline catalogs at most {MAX_COLUMNS}."
            )));
        }
        let mut names = Vec::with_capacity(width);
        let mut seen = HashSet::new();
        for col in first..=last {
            let name = cells
                .iter()
                .find(|(c, _)| *c == col)
                .and_then(|(_, v)| v.header())
                .ok_or_else(|| {
                    bad(format!(
                        "header cell {}{number} is empty; name every column in the header row",
                        column_letters(col)
                    ))
                })?;
            crate::workbench::desk::check_text(&name, 500)?;
            if !seen.insert(name.clone()) {
                return Err(bad(format!(
                    "column name {name} appears twice in the header"
                )));
            }
            names.push(name);
        }
        self.header_row = Some(number);
        self.first = first;
        self.stats = vec![ColumnStats::default(); width];
        self.number_formats = vec![BTreeMap::new(); width];
        if let Some(w) = self.window.as_mut() {
            w.columns = names.iter().take(PREVIEW_COLUMNS).cloned().collect();
        }
        self.names = names;
        Ok(())
    }
    fn row(&mut self, number: usize, cells: Vec<(usize, Value)>) -> WorkbenchResult<()> {
        if number <= self.last_row || number > MAX_ROWS {
            return Err(bad(format!(
                "row {number} is out of order or beyond Excel's limit"
            )));
        }
        self.last_row = number;
        if cells.iter().all(|(_, v)| v.blank()) {
            return Ok(());
        }
        let Some(header) = self.header_row else {
            return self.header(number, &cells);
        };
        let index = number - header - 1;
        let width = self.names.len();
        let gap = index - self.rows;
        if gap > 0 {
            for s in &mut self.stats {
                s.missing("empty", gap);
            }
            if let Some(w) = self.window.as_mut() {
                let from = self.rows.max(w.start);
                let to = index.min(w.start.saturating_add(PREVIEW_ROWS));
                for _ in from..to.max(from) {
                    w.rows.push(vec![None; width.min(PREVIEW_COLUMNS)]);
                }
            }
        }
        let keep = self.window.as_ref().is_some_and(|w| w.wants(index));
        let mut shown = vec![None; if keep { width.min(PREVIEW_COLUMNS) } else { 0 }];
        let line = self.window.as_ref().map_or(0, |w| w.rows.len());
        let mut present = vec![false; width];
        for (sheet_col, value) in cells {
            let col = sheet_col.wrapping_sub(self.first);
            if sheet_col < self.first || col >= width {
                if !value.blank() {
                    return Err(bad(format!(
                        "cell {}{number} is outside the header's columns; add a header or remove the cell",
                        column_letters(sheet_col)
                    )));
                }
                continue;
            }
            present[col] = true;
            let stats = &mut self.stats[col];
            let (text, code) = match value {
                Value::Empty => (None, Some("empty".to_string())),
                Value::Text(s) if missing_token(&s) => {
                    let token = s.trim();
                    (
                        None,
                        Some(if token.is_empty() { "empty" } else { token }.to_string()),
                    )
                }
                Value::Text(s) => {
                    stats.texts += 1;
                    (Some(s), None)
                }
                Value::Number(s, style) => {
                    stats.numbers += 1;
                    let format = self.formats.get(style).map_or("General", String::as_str);
                    let seen = &mut self.number_formats[col];
                    if format != "General"
                        && (seen.len() < MAX_FORMATS_PER_COLUMN || seen.contains_key(format))
                    {
                        *seen.entry(format.to_string()).or_default() += 1;
                    }
                    (Some(s), None)
                }
                Value::Bool(b) => {
                    stats.booleans += 1;
                    (Some(if b { "TRUE" } else { "FALSE" }.to_string()), None)
                }
                Value::Error(e) => (None, Some(e)),
            };
            if let Some(code) = &code {
                stats.missing(code, 1);
            }
            if keep && col < PREVIEW_COLUMNS {
                shown[col] = text.as_deref().map(cell_text);
                if let (Some(code), Some(w)) = (code, self.window.as_mut()) {
                    if code != "empty" {
                        w.missing_codes.push(MissingCell {
                            row: line,
                            column: col,
                            code,
                        });
                    }
                }
            }
        }
        for (col, here) in present.iter().enumerate() {
            if !here {
                self.stats[col].missing("empty", 1);
            }
        }
        if keep {
            if let Some(w) = self.window.as_mut() {
                w.rows.push(shown);
            }
        }
        self.rows = index + 1;
        Ok(())
    }
}
pub(super) fn worksheet(
    entry: impl Read,
    sheet: &mut Sheet,
    shared: &[String],
) -> WorkbenchResult<()> {
    let mut reader = Reader::from_reader(BufReader::new(Limited {
        inner: entry,
        left: MAX_SHEET_XML,
    }));
    let mut buf = Vec::new();
    let mut cells: Vec<(usize, Value)> = Vec::new();
    let (mut row, mut next_col) = (0usize, 0usize);
    let mut cell: Option<CellState> = None;
    let (mut in_value, mut in_inline, mut in_t, mut phonetic) = (false, false, false, false);
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|_| bad("the worksheet is not well-formed XML or exceeds 512 MiB"))?;
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let open = matches!(event, Event::Start(_));
                match e.local_name().as_ref() {
                    b"row" => {
                        row = match attr(e, b"r")? {
                            Some(r) => r
                                .parse()
                                .map_err(|_| bad(format!("row number {r} is invalid")))?,
                            None => row + 1,
                        };
                        cells.clear();
                        next_col = 0;
                        if !open {
                            sheet.row(row, std::mem::take(&mut cells))?;
                        }
                    }
                    b"c" => {
                        let col = match attr(e, b"r")? {
                            Some(r) => reference_column(&r)?,
                            None => next_col,
                        };
                        if cells.last().is_some_and(|(c, _)| *c >= col) {
                            return Err(bad(format!("row {row} repeats or reorders its cells")));
                        }
                        next_col = col + 1;
                        let state = CellState {
                            col,
                            kind: attr(e, b"t")?,
                            style: attr(e, b"s")?.and_then(|s| s.parse().ok()).unwrap_or(0),
                            ..CellState::default()
                        };
                        if open {
                            cell = Some(state);
                        } else {
                            cells.push((col, Value::Empty));
                        }
                    }
                    b"v" => {
                        if let Some(c) = cell.as_mut() {
                            c.has_value = true;
                            in_value = open;
                        }
                    }
                    b"f" => {
                        if let Some(c) = cell.as_mut() {
                            c.formula = true;
                        }
                    }
                    b"is" => in_inline = open,
                    b"rPh" => phonetic = open,
                    b"t" => in_t = open && in_inline && !phonetic,
                    _ => {}
                }
            }
            Event::End(e) => match e.local_name().as_ref() {
                b"row" => sheet.row(row, std::mem::take(&mut cells))?,
                b"c" => {
                    if let Some(c) = cell.take() {
                        let at = format!("{}{row}", column_letters(c.col));
                        sheet.formulas += usize::from(c.formula);
                        let col = c.col;
                        cells.push((col, classify(c, shared, &at)?));
                    }
                    (in_value, in_inline, in_t, phonetic) = (false, false, false, false);
                }
                b"v" => in_value = false,
                b"is" => in_inline = false,
                b"rPh" => phonetic = false,
                b"t" => in_t = false,
                _ => {}
            },
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) if in_value || in_t => {
                if let Some(c) = cell.as_mut() {
                    push_text(&mut c.text, &event)?;
                }
            }
            Event::DocType(_) => return Err(bad("it contains a DTD")),
            Event::Eof => return Ok(()),
            _ => {}
        }
        buf.clear();
    }
}

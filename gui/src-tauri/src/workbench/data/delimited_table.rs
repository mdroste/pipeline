//! Streaming preview of a live CSV or TSV file, for the file viewer. It stops as
//! soon as a page is full, so a file of any size opens at once. Rows are never
//! counted ahead: `total_rows` is a total only once `end_reached` says so.
//!
//! The dialect is the one `csv` reads, with quoted separators and newlines,
//! doubled quotes, and LF, CR, or CRLF endings. That reader rejects what it cannot
//! catalog. This one shows the file as it is: ragged rows stay ragged, cells keep
//! their exact text, and a stray quote is an ordinary character. Only blank lines
//! are skipped, because a line ending would otherwise read as a row.
//!
//! Memory is bounded per cell, per row, and per column count, and bytes scanned
//! are bounded per request. Load more resumes at a row's byte offset, so it costs
//! the same at any depth. The stamp stops an offset from an older version of the
//! file being applied to a newer one.
use super::file_table::FileTable;
use crate::workbench::store::{WorkbenchError, WorkbenchResult};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::io::{Read, Seek, SeekFrom};

pub(super) const PREVIEW_ROWS: usize = 100;
const MAX_COLUMNS: usize = 200;
const MAX_CELL_CHARS: usize = 2_000;
/// Every character needs at most four bytes, so this holds `MAX_CELL_CHARS`.
const MAX_CELL_BYTES: usize = MAX_CELL_CHARS * 4;
const MAX_ROW_BYTES: usize = 64 * 1024;
const MAX_SCAN_BYTES: u64 = 16 * 1024 * 1024;
const CHUNK_BYTES: u64 = 64 * 1024;
const BOM: &[u8] = b"\xef\xbb\xbf";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableResume {
    pub offset: u64,
    pub stamp: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TablePreview {
    /// Where the next page starts. None at the end of the file, and at a row too
    /// long to pass.
    pub next_offset: Option<u64>,
    pub end_reached: bool,
    /// The widest row in this page, counting columns that are not shown.
    pub column_count: usize,
    /// Cut cells as (data row, column). A None row is the header.
    pub clipped: Vec<(Option<usize>, usize)>,
    /// Some bytes were not UTF-8, so those characters are replacements.
    pub approximate: bool,
    pub stamp: String,
}

#[derive(Default)]
struct Row {
    cells: Vec<String>,
    clipped: Vec<usize>,
    width: usize,
    approximate: bool,
}
#[derive(Default)]
enum State {
    #[default]
    Field,
    Quoted,
    /// A quote inside a quoted field: it closes the field or doubles a quote.
    QuoteEnd,
}
#[derive(Default)]
struct Scanner {
    separator: u8,
    state: State,
    cell: Vec<u8>,
    /// The current cell has content, even if none of it was kept.
    cell_seen: bool,
    cell_clipped: bool,
    row: Row,
    row_bytes: usize,
}
impl Scanner {
    fn keep(&mut self, byte: u8) {
        self.cell_seen = true;
        if self.row.width < MAX_COLUMNS
            && self.cell.len() < MAX_CELL_BYTES
            && self.row_bytes < MAX_ROW_BYTES
        {
            self.cell.push(byte);
            self.row_bytes += 1;
        } else {
            self.cell_clipped = true;
        }
    }
    fn end_cell(&mut self) {
        if self.row.width < MAX_COLUMNS {
            let text = String::from_utf8_lossy(&self.cell);
            self.row.approximate |= matches!(text, Cow::Owned(_));
            let cut = text.char_indices().nth(MAX_CELL_CHARS).map(|(at, _)| at);
            if self.cell_clipped || cut.is_some() {
                self.row.clipped.push(self.row.width);
            }
            let text = &text[..cut.unwrap_or(text.len())];
            self.row.cells.push(text.to_string());
        }
        self.row.width += 1;
        self.cell.clear();
        self.cell_seen = false;
        self.cell_clipped = false;
        self.state = State::Field;
    }
    /// Take one byte, and report whether it ended a row.
    fn feed(&mut self, byte: u8) -> bool {
        match self.state {
            State::Quoted if byte == b'"' => self.state = State::QuoteEnd,
            State::Quoted => self.keep(byte),
            State::QuoteEnd if byte == b'"' => {
                self.keep(byte);
                self.state = State::Quoted;
            }
            _ => {
                self.state = State::Field;
                if byte == self.separator {
                    self.end_cell();
                } else if byte == b'\n' || byte == b'\r' {
                    // The LF of a CRLF lands here too, as an empty line.
                    if self.started() {
                        self.end_cell();
                        return true;
                    }
                } else if byte == b'"' && !self.cell_seen {
                    self.cell_seen = true;
                    self.state = State::Quoted;
                } else {
                    self.keep(byte);
                }
            }
        }
        false
    }
    fn started(&self) -> bool {
        self.cell_seen || self.row.width > 0
    }
    fn take_row(&mut self) -> Row {
        self.row_bytes = 0;
        std::mem::take(&mut self.row)
    }
    /// A row with no ending: the last of the file, or one cut off by the scan limit.
    fn finish_row(&mut self, clipped: bool) -> Row {
        self.cell_clipped |= clipped;
        self.end_cell();
        self.take_row()
    }
}

struct Page {
    start: usize,
    /// The next row is the header: only the first page has one.
    header: bool,
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    clipped: Vec<(Option<usize>, usize)>,
    widest: usize,
    approximate: bool,
}
impl Page {
    fn full(&self) -> bool {
        self.rows.len() >= PREVIEW_ROWS
    }
    fn accept(&mut self, row: Row) {
        let index = (!self.header).then_some(self.start + self.rows.len());
        self.clipped
            .extend(row.clipped.into_iter().map(|column| (index, column)));
        self.widest = self.widest.max(row.width);
        self.approximate |= row.approximate;
        if self.header {
            self.columns = row.cells;
            self.header = false;
        } else {
            self.rows.push(row.cells.into_iter().map(Some).collect());
        }
    }
}

enum Stop {
    PageFull,
    EndOfFile,
    ScanLimit,
}

/// The first page, or the page that `resume` points to, with `start` numbering
/// its rows. Only a resumed page has no `columns`: the first page has them.
pub fn delimited_table(
    mut reader: impl Read + Seek,
    extension: &str,
    resume: Option<&TableResume>,
    start: usize,
    stamp: String,
) -> WorkbenchResult<FileTable> {
    if resume.is_some_and(|resume| resume.stamp != stamp) {
        let mut error = WorkbenchError::invalid("This file changed on disk");
        error.code = "file_changed".into();
        return Err(error);
    }
    let offset = resume.map_or(0, |resume| resume.offset);
    reader.seek(SeekFrom::Start(offset)).map_err(super::err)?;
    let mut scanner = Scanner {
        separator: if extension == "tsv" { b'\t' } else { b',' },
        ..Scanner::default()
    };
    let mut page = Page {
        start: if resume.is_some() { start } else { 0 },
        header: resume.is_none(),
        columns: Vec::new(),
        rows: Vec::new(),
        clipped: Vec::new(),
        widest: 0,
        approximate: false,
    };
    let (mut position, mut row_end, mut scanned) = (offset, offset, 0u64);
    let mut chunk = Vec::with_capacity(CHUNK_BYTES as usize);
    let stop = 'scan: loop {
        let want = CHUNK_BYTES.min(MAX_SCAN_BYTES - scanned);
        if want == 0 {
            break Stop::ScanLimit;
        }
        chunk.clear();
        let read = reader
            .by_ref()
            .take(want)
            .read_to_end(&mut chunk)
            .map_err(super::err)? as u64;
        let bytes = match chunk.strip_prefix(BOM) {
            Some(rest) if position == 0 => rest,
            _ => &chunk[..],
        };
        position += (chunk.len() - bytes.len()) as u64;
        scanned += read;
        for &byte in bytes {
            if page.full() {
                // Look past the last row's line ending: more rows may not exist.
                if byte != b'\n' && byte != b'\r' {
                    break 'scan Stop::PageFull;
                }
                position += 1;
                continue;
            }
            position += 1;
            if scanner.feed(byte) {
                page.accept(scanner.take_row());
                row_end = position;
            }
        }
        if read < want {
            break Stop::EndOfFile;
        }
    };
    let (next_offset, end_reached) = match stop {
        Stop::PageFull => (Some(position), false),
        Stop::EndOfFile => {
            if scanner.started() {
                page.accept(scanner.finish_row(false));
            }
            (None, true)
        }
        Stop::ScanLimit if !scanner.started() => (Some(position), false),
        // The next request starts at this row, with the whole limit for it.
        Stop::ScanLimit if row_end > offset => (Some(row_end), false),
        // One row outlasted the limit, so show its start and stop here.
        Stop::ScanLimit => {
            page.accept(scanner.finish_row(true));
            (None, false)
        }
    };
    Ok(FileTable {
        columns: page.columns,
        total_rows: page.start + page.rows.len(),
        rows: page.rows,
        start: page.start,
        page_size: PREVIEW_ROWS,
        sheet: None,
        sheets: Vec::new(),
        missing_codes: Vec::new(),
        preview: Some(TablePreview {
            next_offset,
            end_reached,
            column_count: page.widest,
            clipped: page.clipped,
            approximate: page.approximate,
            stamp,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn read(bytes: &[u8], extension: &str) -> FileTable {
        delimited_table(Cursor::new(bytes), extension, None, 0, "1".into()).unwrap()
    }
    fn resume(bytes: &[u8], table: &FileTable) -> FileTable {
        let preview = table.preview.as_ref().unwrap();
        let resume = TableResume {
            offset: preview.next_offset.unwrap(),
            stamp: preview.stamp.clone(),
        };
        let start = table.start + table.rows.len();
        delimited_table(Cursor::new(bytes), "csv", Some(&resume), start, "1".into()).unwrap()
    }
    fn cells(table: &FileTable) -> Vec<Vec<&str>> {
        table
            .rows
            .iter()
            .map(|row| row.iter().map(|cell| cell.as_deref().unwrap()).collect())
            .collect()
    }
    fn preview(table: &FileTable) -> &TablePreview {
        table.preview.as_ref().unwrap()
    }
    fn numbered(rows: usize) -> Vec<u8> {
        let mut text = String::from("id,value\n");
        for row in 0..rows {
            text.push_str(&format!("{row},v{row}\n"));
        }
        text.into_bytes()
    }

    /// Endless bytes, repeating after a prefix, that refuse to be read past a
    /// bound. It stands for a file of any size.
    struct Bounded {
        prefix: Vec<u8>,
        pattern: Vec<u8>,
        at: u64,
        bound: u64,
    }
    impl Read for Bounded {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            for slot in buffer.iter_mut() {
                assert!(self.at < self.bound, "read past byte {}", self.bound);
                let at = self.at as usize;
                *slot = match self.prefix.get(at) {
                    Some(byte) => *byte,
                    None => self.pattern[(at - self.prefix.len()) % self.pattern.len()],
                };
                self.at += 1;
            }
            Ok(buffer.len())
        }
    }
    impl Seek for Bounded {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            let SeekFrom::Start(offset) = position else {
                panic!("the reader only seeks from the start");
            };
            self.at = offset;
            Ok(offset)
        }
    }

    #[test]
    fn opening_stops_as_soon_as_the_page_is_full() {
        let file = Bounded {
            prefix: b"id,value\n".to_vec(),
            pattern: b"1,two\n".to_vec(),
            at: 0,
            bound: CHUNK_BYTES,
        };
        let table = delimited_table(file, "csv", None, 0, "1".into()).unwrap();
        assert_eq!(table.rows.len(), PREVIEW_ROWS);
        assert_eq!(table.columns, ["id", "value"]);
        assert!(!preview(&table).end_reached);
        // 9 bytes of header, then 6 bytes for each of the 100 rows.
        assert_eq!(preview(&table).next_offset, Some(609));
    }

    #[test]
    fn load_more_starts_at_its_offset_and_rescans_nothing_before_it() {
        let deep = 6 << 40;
        let file = Bounded {
            prefix: Vec::new(),
            pattern: b"1,two\n".to_vec(),
            at: 0,
            bound: deep + CHUNK_BYTES,
        };
        let request = TableResume {
            offset: deep,
            stamp: "1".into(),
        };
        let table = delimited_table(file, "csv", Some(&request), 500, "1".into()).unwrap();
        assert_eq!(table.rows.len(), PREVIEW_ROWS);
        assert_eq!(table.start, 500);
        assert!(table.columns.is_empty());
        assert_eq!(preview(&table).next_offset, Some(deep + 600));
    }

    #[test]
    fn a_row_with_no_end_stops_at_the_scan_limit_with_memory_bounded() {
        let file = Bounded {
            prefix: b"id,value\n1,".to_vec(),
            pattern: b"x".to_vec(),
            at: 0,
            bound: MAX_SCAN_BYTES,
        };
        let first = delimited_table(file, "csv", None, 0, "1".into()).unwrap();
        assert!(first.rows.is_empty());
        assert_eq!(preview(&first).next_offset, Some(9));
        assert!(!preview(&first).end_reached);
        let file = Bounded {
            prefix: b"id,value\n1,".to_vec(),
            pattern: b"x".to_vec(),
            at: 0,
            bound: 9 + MAX_SCAN_BYTES,
        };
        let request = TableResume {
            offset: 9,
            stamp: "1".into(),
        };
        let stuck = delimited_table(file, "csv", Some(&request), 0, "1".into()).unwrap();
        assert_eq!(stuck.rows[0][1].as_ref().unwrap().len(), MAX_CELL_CHARS);
        assert_eq!(preview(&stuck).clipped, [(Some(0), 1)]);
        assert_eq!(preview(&stuck).next_offset, None);
        assert!(!preview(&stuck).end_reached);
    }

    #[test]
    fn rows_after_the_scan_limit_are_reached_by_load_more() {
        let long = "x".repeat(MAX_SCAN_BYTES as usize - 64);
        let bytes = format!("id,value\n1,{long}\n2,b\n3,{long}\n4,d\n").into_bytes();
        let first = read(&bytes, "csv");
        assert_eq!(first.rows.len(), 2);
        let second = resume(&bytes, &first);
        assert_eq!(second.rows.len(), 2);
        assert_eq!(second.start, 2);
        assert_eq!(second.rows[1][1].as_deref(), Some("d"));
        assert_eq!(preview(&second).clipped, [(Some(2), 1)]);
        assert!(preview(&second).end_reached);
    }

    #[test]
    fn quoted_separators_newlines_and_quotes_stay_in_their_cell() {
        let table = read(
            b"a,b\n\"one, two\",\"line\nnext\"\n\"say \"\"hi\"\"\",\r\n",
            "csv",
        );
        assert_eq!(
            cells(&table),
            [vec!["one, two", "line\nnext"], vec!["say \"hi\"", ""]]
        );
        assert_eq!(table.total_rows, 2);
    }

    #[test]
    fn tsv_splits_on_tabs_and_keeps_commas() {
        let table = read(b"a\tb\n1,5\t\"x\ty\"\n", "tsv");
        assert_eq!(table.columns, ["a", "b"]);
        assert_eq!(cells(&table), [["1,5", "x\ty"]]);
    }

    #[test]
    fn a_byte_order_mark_is_not_part_of_the_first_column() {
        let table = read(b"\xef\xbb\xbf\"id\",value\n1,2\n", "csv");
        assert_eq!(table.columns, ["id", "value"]);
    }

    #[test]
    fn latin1_bytes_are_replaced_and_reported() {
        let table = read(b"name,city\nJos\xe9,Z\xfcrich\n", "csv");
        assert_eq!(cells(&table), [["Jos\u{fffd}", "Z\u{fffd}rich"]]);
        assert!(preview(&table).approximate);
        assert!(!preview(&read(b"name\nJos\xc3\xa9\n", "csv")).approximate);
    }

    #[test]
    fn cells_keep_their_exact_text() {
        let table = read(b"a,b,c,d\n007,NA,1.50, x \n", "csv");
        assert_eq!(cells(&table), [["007", "NA", "1.50", " x "]]);
    }

    #[test]
    fn short_and_long_rows_are_kept_as_they_are() {
        let table = read(b"a,b\n1\n1,2,3\n", "csv");
        assert_eq!(cells(&table), [vec!["1"], vec!["1", "2", "3"]]);
        assert_eq!(preview(&table).column_count, 3);
    }

    #[test]
    fn empty_and_header_only_files_have_no_rows() {
        let empty = read(b"", "csv");
        assert!(empty.columns.is_empty() && empty.rows.is_empty());
        assert!(preview(&empty).end_reached);
        for bytes in [&b"a,b\n"[..], b"a,b", b"a,b\r\n\r\n\n"] {
            let header = read(bytes, "csv");
            assert_eq!(header.columns, ["a", "b"]);
            assert_eq!(header.total_rows, 0);
            assert!(preview(&header).end_reached);
        }
    }

    #[test]
    fn exactly_one_page_has_no_next_page() {
        for ending in ["", "\n", "\r\n\n\n"] {
            let mut bytes = numbered(PREVIEW_ROWS);
            bytes.pop();
            bytes.extend_from_slice(ending.as_bytes());
            let table = read(&bytes, "csv");
            assert_eq!(table.rows.len(), PREVIEW_ROWS);
            assert_eq!(preview(&table).next_offset, None);
            assert!(preview(&table).end_reached);
            assert_eq!(table.total_rows, PREVIEW_ROWS);
        }
    }

    #[test]
    fn pages_continue_without_gaps_and_end_with_the_exact_total() {
        let bytes = numbered(PREVIEW_ROWS * 2 + 1);
        let first = read(&bytes, "csv");
        let second = resume(&bytes, &first);
        assert_eq!(second.rows[0][0].as_deref(), Some("100"));
        assert_eq!(second.start, PREVIEW_ROWS);
        let third = resume(&bytes, &second);
        assert_eq!(cells(&third), [["200", "v200"]]);
        assert_eq!(third.total_rows, PREVIEW_ROWS * 2 + 1);
        assert!(preview(&third).end_reached);
    }

    #[test]
    fn a_long_cell_is_cut_and_marked() {
        let long = "é".repeat(MAX_CELL_CHARS + 5);
        let exact = "x".repeat(MAX_CELL_CHARS);
        let bytes = format!("{long},b\n{exact},{long}\n").into_bytes();
        let table = read(&bytes, "csv");
        assert_eq!(table.columns[0].chars().count(), MAX_CELL_CHARS);
        assert_eq!(table.rows[0][0].as_deref(), Some(exact.as_str()));
        assert_eq!(
            table.rows[0][1].as_ref().unwrap().chars().count(),
            MAX_CELL_CHARS
        );
        assert_eq!(preview(&table).clipped, [(None, 0), (Some(0), 1)]);
        assert!(!preview(&table).approximate);
    }

    #[test]
    fn a_long_cell_leaves_room_for_the_rest_of_its_row() {
        let long = "x".repeat(MAX_ROW_BYTES * 2);
        let table = read(format!("a,b\n{long},next\n").as_bytes(), "csv");
        assert_eq!(table.rows[0][1].as_deref(), Some("next"));
        assert_eq!(preview(&table).clipped, [(Some(0), 0)]);
    }

    #[test]
    fn a_row_keeps_a_bounded_number_of_bytes() {
        let cell = "x".repeat(MAX_CELL_CHARS);
        let row = vec![cell.as_str(); 100].join(",");
        let table = read(format!("a\n{row}\n").as_bytes(), "csv");
        let kept: usize = table.rows[0]
            .iter()
            .map(|cell| cell.as_ref().unwrap().len())
            .sum();
        assert_eq!(kept, MAX_ROW_BYTES);
        assert_eq!(table.rows[0].len(), 100);
        assert!(preview(&table).clipped.len() > 60);
    }

    #[test]
    fn columns_past_the_limit_are_counted_but_not_kept() {
        let header = (0..MAX_COLUMNS + 50)
            .map(|column| format!("c{column}"))
            .collect::<Vec<_>>()
            .join(",");
        let table = read(format!("{header}\n1\n").as_bytes(), "csv");
        assert_eq!(table.columns.len(), MAX_COLUMNS);
        assert_eq!(preview(&table).column_count, MAX_COLUMNS + 50);
    }

    #[test]
    fn a_stray_or_unclosed_quote_does_not_lose_the_file() {
        assert_eq!(cells(&read(b"a\nab\"c\n", "csv")), [["ab\"c"]]);
        assert_eq!(
            cells(&read(b"a,b\n1,\"open\n2,3\n", "csv")),
            [["1", "open\n2,3\n"]]
        );
    }

    #[test]
    fn the_viewer_receives_the_preview_in_its_own_names() {
        let json = serde_json::to_value(read(b"a,b\n1,2\n", "csv")).unwrap();
        assert_eq!(json["totalRows"], 1);
        assert_eq!(json["preview"]["endReached"], true);
        assert_eq!(json["preview"]["columnCount"], 2);
        assert!(json["preview"]["nextOffset"].is_null());
    }

    #[test]
    fn an_offset_from_another_version_is_refused() {
        let request = TableResume {
            offset: 9,
            stamp: "old".into(),
        };
        let error = delimited_table(
            Cursor::new(b"a\n1\n"),
            "csv",
            Some(&request),
            1,
            "new".into(),
        )
        .unwrap_err();
        assert_eq!(error.code, "file_changed");
    }
}

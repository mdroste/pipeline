//! Bounded Office Open XML (`.xlsx`) reader for one worksheet. Cells keep their
//! cached values: shared and inline strings, numbers, booleans and error codes.
//! Dates stay serial numbers; each column records its cell number formats.
//! Every archive part is read through a decompressed-size limit, and the
//! worksheet is streamed so only statistics and a preview window are kept.
mod sheet;

use super::profile::{Profile, Window};
use super::{SourceFormat, VariableDefinition};
use crate::workbench::store::{WorkbenchError, WorkbenchResult};
use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};
use sheet::{worksheet, Sheet};
use std::collections::HashMap;
use std::io::{BufReader, Cursor, Read};

const MAX_ENTRIES: usize = 10_000;
/// Workbook, relationship and style parts.
const MAX_PART: u64 = 16 * 1024 * 1024;
const MAX_SHARED_XML: u64 = 128 * 1024 * 1024;
const MAX_SHARED_TEXT: usize = 64 * 1024 * 1024;
const MAX_SHARED_COUNT: usize = 2_000_000;
const MAX_CELL_TEXT: usize = 1024 * 1024;

fn bad(message: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::invalid(format!("Excel workbook refused: {message}"))
}
type Archive<'a> = zip::ZipArchive<Cursor<&'a [u8]>>;

/// Refuses to yield more than `left` decompressed bytes.
struct Limited<R> {
    inner: R,
    left: u64,
}
impl<R: Read> Read for Limited<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.left == 0 {
            let mut probe = [0u8; 1];
            return match self.inner.read(&mut probe)? {
                0 => Ok(0),
                _ => Err(std::io::Error::other("archive part exceeds its limit")),
            };
        }
        let max = buf.len().min(self.left.min(usize::MAX as u64) as usize);
        let n = self.inner.read(&mut buf[..max])?;
        self.left -= n as u64;
        Ok(n)
    }
}
fn open(bytes: &[u8]) -> WorkbenchResult<Archive<'_>> {
    let archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| {
        bad("it is not an .xlsx archive (password-protected, .xls and .xlsb workbooks are not supported)")
    })?;
    if archive.len() > MAX_ENTRIES {
        return Err(bad("it has too many archive entries"));
    }
    Ok(archive)
}
/// One whole part within `limit` decompressed bytes; `None` when absent.
fn part(archive: &mut Archive, name: &str, limit: u64) -> WorkbenchResult<Option<Vec<u8>>> {
    let entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(_) => return Err(bad(format!("{name} is unreadable or encrypted"))),
    };
    if entry.size() > limit {
        return Err(bad(format!("{name} exceeds its {} MiB limit", limit >> 20)));
    }
    let mut data = Vec::new();
    Limited {
        inner: entry,
        left: limit,
    }
    .read_to_end(&mut data)
    .map_err(|_| bad(format!("{name} is unreadable or exceeds its limit")))?;
    Ok(Some(data))
}
fn attr(e: &BytesStart, name: &[u8]) -> WorkbenchResult<Option<String>> {
    for a in e.attributes() {
        let a = a.map_err(|_| bad("an XML attribute is malformed"))?;
        if a.key.local_name().as_ref() == name {
            return a
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map(|v| Some(v.into_owned()))
                .map_err(|_| bad("an XML attribute is malformed"));
        }
    }
    Ok(None)
}
/// Append character data, resolving character and predefined entity references.
fn push_text(out: &mut String, event: &Event) -> WorkbenchResult<()> {
    let malformed = || bad("cell text is malformed");
    match event {
        Event::Text(t) => out.push_str(&t.xml10_content().map_err(|_| malformed())?),
        Event::CData(t) => out.push_str(&t.decode().map_err(|_| malformed())?),
        Event::GeneralRef(r) => match r.resolve_char_ref().map_err(|_| malformed())? {
            Some(c) => out.push(c),
            None => {
                let name = r.decode().map_err(|_| malformed())?;
                out.push_str(
                    quick_xml::escape::resolve_predefined_entity(&name)
                        .ok_or_else(|| bad("it uses an undefined XML entity"))?,
                );
            }
        },
        _ => {}
    }
    if out.len() > MAX_CELL_TEXT {
        return Err(bad("a cell exceeds 1 MiB of text"));
    }
    Ok(())
}
/// Visit start/empty elements of a small in-memory part.
fn elements(
    xml: &[u8],
    mut visit: impl FnMut(&BytesStart, bool) -> WorkbenchResult<()>,
) -> WorkbenchResult<()> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|_| bad("an archive part is not well-formed XML"))?
        {
            Event::Start(e) => visit(&e, true)?,
            Event::Empty(e) => visit(&e, false)?,
            Event::DocType(_) => return Err(bad("it contains a DTD")),
            Event::Eof => return Ok(()),
            _ => {}
        }
        buf.clear();
    }
}
/// `(id, type, target)` for each internal relationship.
fn relationships(xml: &[u8]) -> WorkbenchResult<Vec<(String, String, String)>> {
    let mut out = Vec::new();
    elements(xml, |e, _| {
        if e.local_name().as_ref() == b"Relationship"
            && attr(e, b"TargetMode")?.as_deref() != Some("External")
        {
            if let (Some(id), Some(kind), Some(target)) =
                (attr(e, b"Id")?, attr(e, b"Type")?, attr(e, b"Target")?)
            {
                out.push((id, kind, target));
            }
        }
        Ok(())
    })?;
    Ok(out)
}
/// Resolve a relationship target against the source part's directory.
fn resolve(dir: &str, target: &str) -> String {
    let (base, target) = match target.strip_prefix('/') {
        Some(absolute) => ("", absolute),
        None => (dir, target),
    };
    let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}
fn rels_path(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}
fn builtin_format(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "mm-dd-yy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yy h:mm",
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[Red](#,##0.00)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}
/// Whether a number format renders dates or times (quoted text and colour or
/// locale blocks are ignored).
pub(super) fn is_date_format(code: &str) -> bool {
    if code.eq_ignore_ascii_case("general") {
        return false;
    }
    let mut plain = String::new();
    let (mut quoted, mut bracket, mut escaped) = (false, String::new(), false);
    let mut in_bracket = false;
    for c in code.chars() {
        if escaped {
            escaped = false;
        } else if quoted {
            quoted = c != '"';
        } else if in_bracket {
            if c == ']' {
                in_bracket = false;
                if matches!(
                    bracket.to_ascii_lowercase().as_str(),
                    "h" | "hh" | "m" | "mm" | "s" | "ss"
                ) {
                    plain.push('h');
                }
            } else {
                bracket.push(c);
            }
        } else {
            match c {
                '"' => quoted = true,
                '\\' => escaped = true,
                '[' => {
                    in_bracket = true;
                    bracket.clear();
                }
                c => plain.push(c.to_ascii_lowercase()),
            }
        }
    }
    plain.contains(['d', 'm', 'y', 'h', 's'])
}
/// Cell number format code per `cellXfs` index.
fn styles(xml: &[u8]) -> WorkbenchResult<Vec<String>> {
    let mut custom = HashMap::new();
    let mut formats = Vec::new();
    let mut in_cell_xfs = false;
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|_| bad("xl/styles.xml is not well-formed XML"))?;
        match &event {
            Event::Start(e) | Event::Empty(e) => match e.local_name().as_ref() {
                b"numFmt" => {
                    if let (Some(id), Some(code)) = (attr(e, b"numFmtId")?, attr(e, b"formatCode")?)
                    {
                        custom.insert(id.parse::<u32>().unwrap_or(u32::MAX), code);
                    }
                }
                b"cellXfs" => in_cell_xfs = matches!(event, Event::Start(_)),
                b"xf" if in_cell_xfs => {
                    let id = attr(e, b"numFmtId")?
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);
                    formats.push(
                        custom
                            .get(&id)
                            .cloned()
                            .or_else(|| builtin_format(id).map(str::to_string))
                            .unwrap_or_else(|| format!("built-in format {id}")),
                    );
                    if formats.len() > 65_536 {
                        return Err(bad("it declares too many cell styles"));
                    }
                }
                _ => {}
            },
            Event::End(e) if e.local_name().as_ref() == b"cellXfs" => in_cell_xfs = false,
            Event::DocType(_) => return Err(bad("it contains a DTD")),
            Event::Eof => return Ok(formats),
            _ => {}
        }
        buf.clear();
    }
}
/// Shared-string table; rich-text runs are joined and phonetic runs dropped.
fn shared_strings(entry: impl Read) -> WorkbenchResult<Vec<String>> {
    let mut reader = Reader::from_reader(BufReader::new(Limited {
        inner: entry,
        left: MAX_SHARED_XML,
    }));
    let (mut out, mut total) = (Vec::new(), 0usize);
    let (mut current, mut in_t, mut phonetic) = (String::new(), false, false);
    let mut buf = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|_| bad("the shared-string table is malformed or exceeds its limit"))?;
        match &event {
            Event::Start(e) => match e.local_name().as_ref() {
                b"si" => current.clear(),
                b"rPh" => phonetic = true,
                b"t" => in_t = !phonetic,
                _ => {}
            },
            Event::End(e) => match e.local_name().as_ref() {
                b"si" => {
                    total += current.len();
                    if out.len() >= MAX_SHARED_COUNT || total > MAX_SHARED_TEXT {
                        return Err(bad("the shared-string table exceeds its limit"));
                    }
                    out.push(std::mem::take(&mut current));
                }
                b"rPh" => phonetic = false,
                b"t" => in_t = false,
                _ => {}
            },
            Event::Empty(e) if e.local_name().as_ref() == b"si" => out.push(String::new()),
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) if in_t => {
                push_text(&mut current, &event)?
            }
            Event::DocType(_) => return Err(bad("it contains a DTD")),
            Event::Eof => return Ok(out),
            _ => {}
        }
        buf.clear();
    }
}
/// Profile one worksheet (the first when `sheet` is `None`) and optionally
/// keep a preview window starting at data row `start`.
pub(super) fn read(
    bytes: &[u8],
    sheet: Option<&str>,
    start: Option<usize>,
) -> WorkbenchResult<(Profile, Option<Window>)> {
    let mut archive = open(bytes)?;
    let root = part(&mut archive, "_rels/.rels", MAX_PART)?
        .map(|xml| relationships(&xml))
        .transpose()?
        .unwrap_or_default();
    let workbook_path = root
        .iter()
        .find(|(_, kind, _)| kind.ends_with("/officeDocument"))
        .map_or_else(|| "xl/workbook.xml".to_string(), |(_, _, t)| resolve("", t));
    let workbook = part(&mut archive, &workbook_path, MAX_PART)?
        .ok_or_else(|| bad("it has no workbook part"))?;
    let mut sheets = Vec::new();
    let mut date1904 = false;
    elements(&workbook, |e, _| {
        match e.local_name().as_ref() {
            b"sheet" => {
                if let (Some(name), Some(id)) = (attr(e, b"name")?, attr(e, b"id")?) {
                    sheets.push((name, id));
                }
            }
            b"workbookPr" => {
                date1904 = matches!(attr(e, b"date1904")?.as_deref(), Some("1" | "true"));
            }
            _ => {}
        }
        Ok(())
    })?;
    let dir = workbook_path
        .rsplit_once('/')
        .map_or("", |(d, _)| d)
        .to_string();
    let rels = part(&mut archive, &rels_path(&workbook_path), MAX_PART)?
        .map(|xml| relationships(&xml))
        .transpose()?
        .unwrap_or_default();
    let target = |suffix: &str| {
        rels.iter()
            .find(|(_, kind, _)| kind.ends_with(suffix))
            .map(|(_, _, t)| resolve(&dir, t))
    };
    let (sheet_name, rel) = match sheet.map(str::trim).filter(|s| !s.is_empty()) {
        Some(wanted) => sheets
            .iter()
            .find(|(name, _)| name == wanted)
            .cloned()
            .ok_or_else(|| {
                bad(format!(
                    "it has no sheet named {wanted}; available sheets: {}",
                    sheets
                        .iter()
                        .take(20)
                        .map(|(n, _)| n.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?,
        None => sheets
            .first()
            .cloned()
            .ok_or_else(|| bad("it has no worksheets"))?,
    };
    let (_, kind, sheet_target) = rels
        .iter()
        .find(|(id, _, _)| *id == rel)
        .ok_or_else(|| bad(format!("sheet {sheet_name} has no worksheet part")))?;
    if !kind.ends_with("/worksheet") {
        return Err(bad(format!("sheet {sheet_name} is not a worksheet")));
    }
    let sheet_path = resolve(&dir, sheet_target);
    let formats = match target("/styles") {
        Some(path) => part(&mut archive, &path, MAX_PART)?
            .map(|xml| styles(&xml))
            .transpose()?
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let shared = match target("/sharedStrings") {
        Some(path) => match archive.by_name(&path) {
            Ok(entry) => shared_strings(entry)?,
            Err(zip::result::ZipError::FileNotFound) => Vec::new(),
            Err(_) => return Err(bad("the shared-string table is unreadable")),
        },
        None => Vec::new(),
    };
    let mut state = Sheet::new(&formats, start.map(Window::new));
    let entry = archive.by_name(&sheet_path).map_err(|_| {
        bad(format!(
            "worksheet part {sheet_path} is missing or unreadable"
        ))
    })?;
    worksheet(entry, &mut state, &shared)?;
    if state.header_row.is_none() {
        return Err(bad(format!("sheet {sheet_name} has no header row")));
    }
    let date_system = if date1904 { "1904" } else { "1900" };
    let mut dates = Vec::new();
    let mut mixed = Vec::new();
    let columns = state
        .names
        .iter()
        .zip(&state.stats)
        .zip(&state.number_formats)
        .map(|((name, stats), formats)| {
            if formats.keys().any(|f| is_date_format(f)) {
                dates.push(name.as_str());
            }
            if stats.numbers > 0 && stats.texts > 0 {
                mixed.push(name.as_str());
            }
            VariableDefinition {
                name: name.clone(),
                inferred_type: stats.inferred_type().into(),
                missing: stats.missing,
                units: None,
                description: None,
                origin: "inferred from all worksheet rows".into(),
                label: None,
                storage_type: None,
                display_format: (!formats.is_empty())
                    .then(|| formats.keys().cloned().collect::<Vec<_>>().join(" | ")),
                value_labels: None,
                missing_codes: stats.codes.clone(),
            }
        })
        .collect::<Vec<_>>();
    let mut limitations = vec![
        "The first non-empty row is the header. Cells keep their saved values; formulas are not recalculated.".to_string(),
        "Missing: empty cells, Excel error values (such as #N/A) and the text NA, N/A, NaN, null or dot, each counted under missingCodes. Values are never replaced with zero.".into(),
        format!("Dates and times remain Excel serial numbers in the {date_system} date system; displayFormat records each column's cell number formats."),
    ];
    if !dates.is_empty() {
        limitations.push(format!(
            "Columns with date or time formats: {}.",
            dates
                .iter()
                .take(20)
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !mixed.is_empty() {
        limitations.push(format!(
            "Columns mixing numbers and text are typed as text: {}.",
            mixed
                .iter()
                .take(20)
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if state.formulas > 0 {
        limitations.push(format!(
            "{} cells contain formulas; their saved results were read.",
            state.formulas
        ));
    }
    let profile = Profile {
        rows: state.rows,
        columns,
        value_labels: Vec::new(),
        source_format: Some(SourceFormat {
            kind: "xlsx".into(),
            sheet: Some(sheet_name),
            sheets: sheets.into_iter().take(100).map(|(n, _)| n).collect(),
            date_system: Some(date_system.into()),
            ..SourceFormat::default()
        }),
        limitations,
    };
    Ok((profile, state.window))
}

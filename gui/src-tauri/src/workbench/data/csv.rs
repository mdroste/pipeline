//! RFC-4180 CSV/TSV reader. Rows are streamed from the captured bytes, so only
//! column statistics and a preview window are kept in memory.
use super::profile::{cell_text, Profile, Window, PREVIEW_COLUMNS};
use super::VariableDefinition;
use crate::workbench::{
    desk::{self, err},
    store::{WorkbenchError, WorkbenchResult},
};

const MAX_ROWS: usize = 1_000_001;
const MAX_COLUMNS: usize = 2000;

/// RFC-4180 quoted fields, embedded newlines, escaped quotes, UTF-8, CRLF.
/// Reject ragged rows and incomplete quoting instead of silently dropping cells.
/// Each row reaches `visit` with its index, the header being row 0. Empty lines at
/// the end of the file are line endings, not rows; earlier ones stay rows.
fn each_row(
    text: &str,
    separator: u8,
    mut visit: impl FnMut(usize, &[String]) -> WorkbenchResult<()>,
) -> WorkbenchResult<()> {
    let (mut count, mut width) = (0usize, 0usize);
    let mut end_row = |row: &mut Vec<String>| {
        if count == 0 {
            width = row.len();
        }
        if count >= MAX_ROWS {
            return Err(WorkbenchError::invalid("CSV exceeds one million rows"));
        }
        if row.len() != width {
            let columns = if row.len() == 1 { "column" } else { "columns" };
            return Err(WorkbenchError::invalid(format!(
                "CSV row {} has {} {columns}, but the header has {width}",
                count + 1,
                row.len()
            )));
        }
        visit(count, row)?;
        count += 1;
        row.clear();
        Ok(())
    };
    let mut row = Vec::new();
    let mut field = Vec::new();
    let mut quoted = false;
    let mut closed = false;
    // Empty lines not yet followed by anything.
    let mut blank = 0usize;
    let b = text.trim_start_matches('\u{feff}').as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if quoted {
            if c == b'"' {
                if b.get(i + 1) == Some(&b'"') {
                    field.push(c);
                    i += 1;
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
        } else if c == b'"' && field.is_empty() && !closed {
            quoted = true;
        } else if (c == b'\n' || c == b'\r') && row.is_empty() && field.is_empty() && !closed {
            if c == b'\r' && b.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            blank += 1;
        } else if c == separator || c == b'\n' || c == b'\r' {
            for _ in 0..std::mem::take(&mut blank) {
                row.push(String::new());
                end_row(&mut row)?;
            }
            row.push(String::from_utf8(std::mem::take(&mut field)).map_err(err)?);
            closed = false;
            if c != separator {
                if c == b'\r' && b.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                end_row(&mut row)?;
            }
        } else {
            if closed || c == b'"' {
                return Err(WorkbenchError::invalid("Invalid CSV quoting"));
            }
            field.push(c);
        }
        i += 1;
        if field.len() > 1024 * 1024 || row.len() > MAX_COLUMNS {
            return Err(WorkbenchError::invalid(
                "CSV field or column limit exceeded",
            ));
        }
    }
    if quoted {
        return Err(WorkbenchError::invalid("Unclosed CSV quote"));
    }
    if !field.is_empty() || !row.is_empty() || closed {
        for _ in 0..blank {
            row.push(String::new());
            end_row(&mut row)?;
        }
        row.push(String::from_utf8(field).map_err(err)?);
        end_row(&mut row)?;
    }
    if width == 0 || width > MAX_COLUMNS {
        return Err(WorkbenchError::invalid(
            "CSV is empty or has too many columns",
        ));
    }
    Ok(())
}
/// What to do with a file that is not UTF-8, which Excel's plain "CSV" is not.
fn not_utf8(bytes: &[u8]) -> WorkbenchError {
    let encoding = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        "UTF-16"
    } else {
        "an older encoding such as Windows-1252"
    };
    WorkbenchError::invalid(format!(
        "This file is saved in {encoding}, not UTF-8. Save it again as CSV UTF-8 and import that copy."
    ))
}
#[cfg(test)]
pub(crate) fn parse(text: &str, separator: u8) -> WorkbenchResult<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    each_row(text, separator, |_, row| {
        rows.push(row.to_vec());
        Ok(())
    })?;
    Ok(rows)
}
/// Missing tokens shared by delimited and spreadsheet text cells.
pub(super) fn missing(s: &str) -> bool {
    s.trim().is_empty() || ["NA", "N/A", "NaN", "null", "."].contains(&s.trim())
}
fn separator(ext: &str) -> u8 {
    if ext == "tsv" {
        b'\t'
    } else {
        b','
    }
}
fn rows(
    bytes: &[u8],
    ext: &str,
    visit: impl FnMut(usize, &[String]) -> WorkbenchResult<()>,
) -> WorkbenchResult<()> {
    each_row(
        std::str::from_utf8(bytes).map_err(|_| not_utf8(bytes))?,
        separator(ext),
        visit,
    )
}
/// Dictionary and missingness over every captured row.
pub(super) fn profile(bytes: &[u8], ext: &str) -> WorkbenchResult<Profile> {
    struct Column {
        name: String,
        missing: usize,
        numeric: bool,
    }
    let mut columns: Vec<Column> = Vec::new();
    let mut count = 0usize;
    rows(bytes, ext, |index, row| {
        if index == 0 {
            let mut names = std::collections::HashSet::new();
            for name in row {
                desk::check_text(name, 500)?;
                if !names.insert(name) {
                    return Err(WorkbenchError::invalid(
                        "Dataset column names must be unique",
                    ));
                }
                columns.push(Column {
                    name: name.clone(),
                    missing: 0,
                    numeric: true,
                });
            }
            return Ok(());
        }
        count += 1;
        for (column, cell) in columns.iter_mut().zip(row) {
            if missing(cell) {
                column.missing += 1;
            } else if column.numeric {
                column.numeric = cell.parse::<f64>().is_ok_and(f64::is_finite);
            }
        }
        Ok(())
    })?;
    Ok(Profile {
        rows: count,
        columns: columns
            .into_iter()
            .map(|c| VariableDefinition {
                name: c.name,
                inferred_type: if c.missing == count {
                    "unknown"
                } else if c.numeric {
                    "number"
                } else {
                    "text"
                }
                .into(),
                missing: c.missing,
                units: None,
                description: None,
                origin: "inferred from all captured rows".into(),
                label: None,
                storage_type: None,
                display_format: None,
                value_labels: None,
                missing_codes: Default::default(),
            })
            .collect(),
        limitations: vec![
            "Missing: empty, NA, N/A, NaN, null, or dot. Values are never replaced with zero."
                .into(),
            "Inferred types are observations, not declared economic units.".into(),
        ],
        ..Profile::default()
    })
}
pub(super) fn window(bytes: &[u8], ext: &str, start: usize) -> WorkbenchResult<Window> {
    let mut window = Window::new(start);
    rows(bytes, ext, |index, row| {
        if index == 0 {
            window.columns = row.iter().take(PREVIEW_COLUMNS).cloned().collect();
        } else if window.wants(index - 1) {
            let shown = row
                .iter()
                .take(PREVIEW_COLUMNS)
                .map(|s| if missing(s) { None } else { Some(cell_text(s)) })
                .collect();
            window.rows.push(shown);
        }
        Ok(())
    })?;
    Ok(window)
}

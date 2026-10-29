//! RFC-4180 CSV/TSV reader. Rows are parsed in memory within the 32 MiB capture limit.
use super::profile::{cell_text, Profile, Window, PREVIEW_COLUMNS, PREVIEW_ROWS};
use super::VariableDefinition;
use crate::workbench::{
    desk::{self, err},
    store::{WorkbenchError, WorkbenchResult},
};

/// RFC-4180 quoted fields, embedded newlines, escaped quotes, UTF-8, CRLF.
/// Reject ragged rows and incomplete quoting instead of silently dropping cells.
pub(crate) fn parse(text: &str, separator: u8) -> WorkbenchResult<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = Vec::new();
    let mut quoted = false;
    let mut closed = false;
    let b = text.trim_start_matches('\u{feff}').as_bytes();
    let mut i = 0;
    let mut cells = 0usize;
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
        } else if c == separator || c == b'\n' || c == b'\r' {
            cells += 1;
            if cells > 2_000_000 {
                return Err(WorkbenchError::invalid("CSV exceeds two million cells"));
            }
            row.push(String::from_utf8(std::mem::take(&mut field)).map_err(err)?);
            closed = false;
            if c != separator {
                if c == b'\r' && b.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                rows.push(std::mem::take(&mut row));
                if rows.len() > 1_000_001 {
                    return Err(WorkbenchError::invalid("CSV exceeds one million rows"));
                }
            }
        } else {
            if closed || c == b'"' {
                return Err(WorkbenchError::invalid("Invalid CSV quoting"));
            }
            field.push(c);
        }
        i += 1;
        if field.len() > 1024 * 1024 || row.len() > 2000 {
            return Err(WorkbenchError::invalid(
                "CSV field or column limit exceeded",
            ));
        }
    }
    if quoted {
        return Err(WorkbenchError::invalid("Unclosed CSV quote"));
    }
    if !field.is_empty() || !row.is_empty() || closed {
        row.push(String::from_utf8(field).map_err(err)?);
        rows.push(row);
    }
    let width = rows.first().map_or(0, Vec::len);
    if width == 0
        || width > 2000
        || rows.len() > 1_000_001
        || cells.saturating_add(1) > 2_000_000
        || rows.iter().any(|r| r.len() != width)
    {
        return Err(WorkbenchError::invalid(
            "CSV is empty or has inconsistent column counts",
        ));
    }
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
fn table(bytes: &[u8], ext: &str) -> WorkbenchResult<Vec<Vec<String>>> {
    parse(std::str::from_utf8(bytes).map_err(err)?, separator(ext))
}
/// Dictionary and missingness over every captured row.
pub(super) fn profile(bytes: &[u8], ext: &str) -> WorkbenchResult<Profile> {
    let table = table(bytes, ext)?;
    let mut names = std::collections::HashSet::new();
    let mut columns = Vec::new();
    for (i, name) in table[0].iter().enumerate() {
        desk::check_text(name, 500)?;
        if !names.insert(name) {
            return Err(WorkbenchError::invalid(
                "Dataset column names must be unique",
            ));
        }
        let values = table
            .iter()
            .skip(1)
            .map(|r| r[i].as_str())
            .collect::<Vec<_>>();
        let count = values.iter().filter(|s| missing(s)).count();
        let numeric = values
            .iter()
            .filter(|s| !missing(s))
            .all(|s| s.parse::<f64>().is_ok_and(f64::is_finite));
        columns.push(VariableDefinition {
            name: name.clone(),
            inferred_type: if count == values.len() {
                "unknown"
            } else if numeric {
                "number"
            } else {
                "text"
            }
            .into(),
            missing: count,
            units: None,
            description: None,
            origin: "inferred from all captured rows".into(),
            label: None,
            storage_type: None,
            display_format: None,
            value_labels: None,
            missing_codes: Default::default(),
        });
    }
    Ok(Profile {
        rows: table.len() - 1,
        columns,
        limitations: vec![
            "Missing: empty, NA, N/A, NaN, null, or dot. Values are never replaced with zero."
                .into(),
            "Inferred types are observations, not declared economic units.".into(),
        ],
        ..Profile::default()
    })
}
pub(super) fn window(bytes: &[u8], ext: &str, start: usize) -> WorkbenchResult<Window> {
    let rows = table(bytes, ext)?;
    let mut window = Window::new(start);
    window.columns = rows[0].iter().take(PREVIEW_COLUMNS).cloned().collect();
    window.rows = rows
        .iter()
        .skip(1 + start.min(rows.len() - 1))
        .take(PREVIEW_ROWS)
        .map(|r| {
            r.iter()
                .take(PREVIEW_COLUMNS)
                .map(|s| if missing(s) { None } else { Some(cell_text(s)) })
                .collect()
        })
        .collect();
    Ok(window)
}

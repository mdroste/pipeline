//! Paged tables over a live workbook, for the file viewer. CSV and TSV files can
//! be any size, so `delimited_table` streams them instead of reading them here.
//! Stata files are usually too large to page, so the viewer does not show them.
//! Captured datasets keep `catalog::preview_rows`; this retains nothing.
use super::delimited_table::TablePreview;
use super::profile::{MissingCell, Window};
use super::xlsx;
use crate::workbench::store::{WorkbenchError, WorkbenchResult};
use serde::Serialize;
use std::io::Read;

const TABLE_ROWS: usize = 200;
const MAX_WORKBOOK_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub start: usize,
    pub page_size: usize,
    pub total_rows: usize,
    pub sheet: Option<String>,
    pub sheets: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing_codes: Vec<MissingCell>,
    /// Set for a streamed file, where `total_rows` is the rows read so far.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<TablePreview>,
}
fn table(
    window: Window,
    total_rows: usize,
    sheet: Option<String>,
    sheets: Vec<String>,
) -> FileTable {
    FileTable {
        columns: window.columns,
        rows: window.rows,
        start: window.start,
        page_size: TABLE_ROWS,
        total_rows,
        sheet,
        sheets,
        missing_codes: window.missing_codes,
        preview: None,
    }
}
fn bounded(reader: impl Read, size: u64, limit: u64, label: &str) -> WorkbenchResult<Vec<u8>> {
    if size > limit {
        return Err(WorkbenchError::invalid(format!(
            "{label} larger than {} MB open as source text only",
            limit / (1024 * 1024)
        )));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(super::err)?;
    if bytes.len() as u64 > limit {
        return Err(WorkbenchError::invalid(format!(
            "{label} grew beyond the table limit"
        )));
    }
    Ok(bytes)
}
/// One page starting at data row `start`.
pub fn file_table(
    reader: impl Read,
    size: u64,
    extension: &str,
    sheet: Option<&str>,
    start: usize,
) -> WorkbenchResult<FileTable> {
    let unavailable = || WorkbenchError::invalid("This table has no readable rows");
    match extension {
        "xlsx" | "xlsm" => {
            let bytes = bounded(reader, size, MAX_WORKBOOK_BYTES, "Workbooks")?;
            let (profile, window) =
                xlsx::read_window(&bytes, sheet, Some(Window::with_limit(start, TABLE_ROWS)))?;
            let format = profile.source_format.unwrap_or_default();
            Ok(table(
                window.ok_or_else(unavailable)?,
                profile.rows,
                format.sheet,
                format.sheets,
            ))
        }
        _ => Err(WorkbenchError::invalid("This file is not a table")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_workbooks_are_tables() {
        assert!(file_table(&b""[..], 0, "txt", None, 0).is_err());
        assert!(file_table(&b"a,b\n1,2\n"[..], 8, "csv", None, 0).is_err());
        assert!(file_table(&b"not stata"[..], 9, "dta", None, 0).is_err());
        let error = file_table(&b""[..], MAX_WORKBOOK_BYTES + 1, "xlsx", None, 0).unwrap_err();
        assert!(error.message.contains("64 MB"));
        assert!(file_table(&b"not a workbook"[..], 14, "xlsx", None, 0).is_err());
    }
}

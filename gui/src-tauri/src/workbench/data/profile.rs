//! Reader-neutral dictionary profile and bounded preview window.
use super::{SourceFormat, ValueLabelSet, VariableDefinition};
use serde::Serialize;
use std::collections::BTreeMap;

pub(super) const PREVIEW_ROWS: usize = 25;
pub(super) const PREVIEW_COLUMNS: usize = 100;
pub(super) const PREVIEW_CELL_BYTES: usize = 500;
/// Variables catalogued per dataset (the CSV limit applies to every format).
pub(super) const MAX_COLUMNS: usize = 2000;

/// Exhaustive dictionary facts produced by one pass of a format reader.
#[derive(Debug, Default)]
pub(crate) struct Profile {
    pub rows: usize,
    pub columns: Vec<VariableDefinition>,
    pub value_labels: Vec<ValueLabelSet>,
    pub source_format: Option<SourceFormat>,
    pub limitations: Vec<String>,
}

/// A missing cell in a preview whose code is more specific than "missing".
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MissingCell {
    pub row: usize,
    pub column: usize,
    pub code: String,
}

/// A bounded page of display text, 25 rows unless a caller asks for its own
/// limit, and 100 columns; `None` is a missing cell.
#[derive(Debug, Default)]
pub(crate) struct Window {
    pub start: usize,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub missing_codes: Vec<MissingCell>,
    limit: usize,
}
impl Window {
    pub fn new(start: usize) -> Self {
        Self::with_limit(start, PREVIEW_ROWS)
    }
    pub fn with_limit(start: usize, limit: usize) -> Self {
        Self {
            start,
            limit,
            ..Self::default()
        }
    }
    /// First row past this page.
    pub fn end(&self) -> usize {
        self.start.saturating_add(self.limit)
    }
    pub fn wants(&self, row: usize) -> bool {
        row >= self.start && row < self.end()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowPreview {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub start: usize,
    pub total_rows: usize,
    pub coverage: String,
    /// Specific missing codes (for example `.a` or `#N/A`) for `None` cells.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing_codes: Vec<MissingCell>,
}

/// Bounded display text for a preview cell.
pub(super) fn cell_text(s: &str) -> String {
    crate::workbench::search::prefix(s, PREVIEW_CELL_BYTES).to_string()
}

/// Per-column counts for readers whose missing codes are open-ended text.
#[derive(Debug, Default, Clone)]
pub(super) struct ColumnStats {
    pub missing: usize,
    pub codes: BTreeMap<String, usize>,
    pub numbers: usize,
    pub texts: usize,
    pub booleans: usize,
}
impl ColumnStats {
    pub fn missing(&mut self, code: &str, count: usize) {
        self.missing += count;
        *self.codes.entry(code.to_string()).or_default() += count;
    }
    pub fn inferred_type(&self) -> &'static str {
        match (self.numbers > 0, self.texts > 0, self.booleans > 0) {
            (false, false, false) => "unknown",
            (true, false, false) => "number",
            (false, false, true) => "boolean",
            _ => "text",
        }
    }
}

/// Refuse dictionaries that cannot be stored, after trimming value labels.
/// Desk records hold at most 256 KiB of JSON.
pub(super) fn fit_dictionary(
    version: &mut super::DatasetVersion,
) -> crate::workbench::store::WorkbenchResult<()> {
    const BUDGET: usize = 240 * 1024;
    let size = |v: &super::DatasetVersion| serde_json::to_string(v).map_or(usize::MAX, |s| s.len());
    if size(version) <= BUDGET {
        return Ok(());
    }
    for set in &mut version.value_labels {
        set.omitted += set.labels.len();
        set.labels.clear();
    }
    if !version.value_labels.is_empty() {
        version.limitations.push(
            "Value-label entries were omitted to keep this dictionary within Pipeline's record limit; set names remain attached to their variables."
                .into(),
        );
    }
    if size(version) <= BUDGET {
        return Ok(());
    }
    Err(crate::workbench::store::WorkbenchError::invalid(format!(
        "The dictionary for {} variables exceeds Pipeline's 256 KiB dataset record limit. Save a subset of variables, or import an exporter dictionary.",
        version.columns.len()
    )))
}

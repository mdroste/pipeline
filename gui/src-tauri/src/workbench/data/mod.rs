//! Immutable tabular vintages and declared sample definitions. No rows enter context implicitly.
//!
//! `catalog` owns capture, previews and samples; `csv`, `dta` and `xlsx` are
//! the bounded format readers; `connectors` owns statistical-source requests;
//! `file_table` and `delimited_table` show live files in the viewer.
mod catalog;
pub mod connectors;
mod csv;
mod delimited_table;
mod dta;
mod file_table;
mod profile;
#[cfg(test)]
mod tests;
mod xlsx;

#[cfg(test)]
pub(crate) use catalog::import_bytes;
pub(crate) use catalog::import_capture;
pub use catalog::{
    commit, import, import_metadata, prepare_local, preview_rows, save_sample, PreparedDataset,
};
pub use delimited_table::{delimited_table, TableResume};
pub use file_table::{file_table, FileTable};
pub use profile::{MissingCell, RowPreview};

use super::{
    desk::{err, ResearchObjectRef},
    store::{Store, WorkbenchError, WorkbenchResult},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataPolicy {
    pub dictionary: bool,
    pub summaries: bool,
    pub assistant_rows: bool,
    pub package_data: bool,
}
impl Default for DataPolicy {
    fn default() -> Self {
        Self {
            dictionary: true,
            summaries: false,
            assistant_rows: false,
            package_data: false,
        }
    }
}
pub fn policy(store: &Store, ws: &str) -> WorkbenchResult<DataPolicy> {
    store.workspace(ws)?;
    let s: Option<String> = store
        .connection()?
        .query_row(
            "SELECT body_json FROM data_policies WHERE workspace_id=?1",
            [ws],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    s.map(|s| serde_json::from_str(&s).map_err(err))
        .unwrap_or(Ok(DataPolicy::default()))
}
pub fn save_policy(store: &Store, ws: &str, value: DataPolicy) -> WorkbenchResult<DataPolicy> {
    store.workspace(ws)?;
    let mut connection = store.connection()?;
    let tx = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(err)?;
    let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM turns t JOIN session_bindings b ON b.id=t.binding_id JOIN sessions s ON s.id=b.session_id WHERE s.workspace_id=?1 AND t.terminal_at IS NULL)",[ws],|r|r.get(0)).map_err(err)?;
    if active {
        return Err(WorkbenchError::conflict(
            "Wait for this project's active turn before changing dataset access",
        ));
    }
    tx.execute("INSERT INTO data_policies VALUES(?1,1,?2) ON CONFLICT(workspace_id) DO UPDATE SET revision=revision+1,body_json=?2",params![ws,serde_json::to_string(&value).map_err(err)?]).map_err(err)?;
    tx.commit().map_err(err)?;
    Ok(value)
}
/// One variable of a captured dataset. Optional fields are omitted when a
/// format does not declare them, so CSV dictionaries keep their original shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableDefinition {
    pub name: String,
    pub inferred_type: String,
    pub missing: usize,
    pub units: Option<String>,
    pub description: Option<String>,
    pub origin: String,
    /// Variable label declared by the file (for example a Stata variable label).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Declared storage type, for example `double`, `str12` or `strL`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_type: Option<String>,
    /// Declared display or cell number format, for example `%td` or `yyyy-mm-dd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_format: Option<String>,
    /// Name of the attached set in `DatasetVersion::value_labels`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_labels: Option<String>,
    /// Counts per distinct missing code (`.`, `.a`, `#N/A`, `empty`). A summary,
    /// so it follows the Summaries policy exactly like `missing`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub missing_codes: BTreeMap<String, usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataAcquisition {
    pub provider: String,
    pub source: String,
    pub retrieved_at: String,
    pub requested_vintage: Option<String>,
    pub returned_vintage: Option<String>,
    pub series_ids: Vec<String>,
    pub units: Option<String>,
    pub frequency: Option<String>,
    pub transformation: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ValueLabel {
    /// Labelled value as text: an integer, or a Stata missing code such as `.a`.
    pub value: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ValueLabelSet {
    pub name: String,
    pub labels: Vec<ValueLabel>,
    /// Entries left out to keep the dictionary within its record limit.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub omitted: usize,
}
fn is_zero(n: &usize) -> bool {
    *n == 0
}
/// File-level facts declared by the source format.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceFormat {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sheets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_system: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetVersion {
    pub schema_version: u32,
    pub artifact_id: Option<String>,
    pub content_hash: String,
    pub external_reference: Option<String>,
    pub rows: usize,
    pub columns: Vec<VariableDefinition>,
    pub acquisition: DataAcquisition,
    pub diagnostics_coverage: String,
    /// Captured format: `csv`, `tsv`, `dta` or `xlsx` (historical field name).
    pub delimiter: String,
    pub limitations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub value_labels: Vec<ValueLabelSet>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_format: Option<SourceFormat>,
    /// Statistical-source terms, notice and citation recorded at capture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector: Option<connectors::ConnectorProvenance>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SampleDefinition {
    pub datasets: Vec<ResearchObjectRef>,
    pub inclusion_rules: String,
    pub filters: Vec<String>,
    pub weights: Option<String>,
    pub date_range: Option<String>,
    pub unit_of_observation: String,
    pub membership_hash: Option<String>,
    pub origin: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDataset {
    pub workspace_id: String,
    pub title: String,
    pub path: String,
    pub acquisition: DataAcquisition,
    pub supersedes: Option<String>,
    pub operation_id: String,
    /// Worksheet name for `.xlsx`; the first sheet when omitted.
    #[serde(default)]
    pub sheet: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataDataset {
    pub workspace_id: String,
    pub title: String,
    pub version: DatasetVersion,
    pub operation_id: String,
}
/// Policy applies before indexing or assembling context, including inferred
/// diagnostics. Labels and formats are dictionary; missing counts are summaries.
pub(crate) fn dictionary_text(body: &str, policy: &DataPolicy) -> WorkbenchResult<String> {
    let mut v: Value = serde_json::from_str(body).map_err(err)?;
    if !policy.summaries {
        if let Some(cols) = v["columns"].as_array_mut() {
            for c in cols {
                if let Some(c) = c.as_object_mut() {
                    c.remove("missing");
                    c.remove("missingCodes");
                }
            }
        }
    }
    serde_json::to_string(&v).map_err(err)
}

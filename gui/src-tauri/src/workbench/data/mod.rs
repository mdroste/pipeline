//! Immutable tabular vintages and declared sample definitions. No rows enter context implicitly.
use super::{
    desk::{self, err, DeskRecord, ResearchObjectRef},
    store::{Store, WorkbenchError, WorkbenchResult},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io::Read, path::Path};
const MAX_BYTES: u64 = 32 * 1024 * 1024;
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableDefinition {
    pub name: String,
    pub inferred_type: String,
    pub missing: usize,
    pub units: Option<String>,
    pub description: Option<String>,
    pub origin: String,
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
    pub delimiter: String,
    pub limitations: Vec<String>,
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
}
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
fn missing(s: &str) -> bool {
    s.trim().is_empty() || ["NA", "N/A", "NaN", "null", "."].contains(&s.trim())
}
pub fn import(store: &Store, r: ImportDataset) -> WorkbenchResult<DeskRecord> {
    let path = Path::new(&r.path);
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !["csv", "tsv"].contains(&ext.as_str()) {
        return Err(WorkbenchError::invalid(
            "Import CSV/TSV or an explicit metadata export; .dta needs a qualified exporter",
        ));
    }
    if fs::symlink_metadata(path)
        .map_err(err)?
        .file_type()
        .is_symlink()
    {
        return Err(WorkbenchError::invalid("Select a regular dataset file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(err)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(WorkbenchError::invalid(
            "Dataset exceeds the 32 MiB local import limit",
        ));
    }
    import_bytes(
        store,
        &r.workspace_id,
        &r.title,
        &bytes,
        &ext,
        r.acquisition,
        r.supersedes.as_deref(),
        &r.operation_id,
    )
}
#[allow(clippy::too_many_arguments)] // Internal capture boundary; callers pass explicit provenance and immutable-version identity.
pub(crate) fn import_bytes(
    store: &Store,
    ws: &str,
    title: &str,
    bytes: &[u8],
    ext: &str,
    acquisition: DataAcquisition,
    supersedes: Option<&str>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    store.workspace(ws)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(WorkbenchError::invalid("Dataset exceeds 32 MiB"));
    }
    let table = parse(
        std::str::from_utf8(bytes).map_err(err)?,
        if ext == "tsv" { b'\t' } else { b',' },
    )?;
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
        });
    }
    let content_hash = desk::hash(bytes);
    let artifact = format!("data_{content_hash}");
    let destination = store
        .root_path()
        .join("blobs")
        .join(format!("{content_hash}.{ext}"));
    if !destination.exists() {
        fs::write(&destination, bytes).map_err(err)?;
    }
    if desk::hash(&fs::read(&destination).map_err(err)?) != content_hash {
        return Err(WorkbenchError::invalid("Dataset blob integrity failure"));
    }
    store.connection()?.execute("INSERT OR IGNORE INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,created_at) VALUES(?1,?2,?3,?4,?5,'dataset',?6,?7)",params![format!("{artifact}_{}",desk::hash(ws.as_bytes())),ws,content_hash,ext,bytes.len() as i64,destination.to_string_lossy(),desk::now()]).map_err(err)?;
    let artifact_id: String = store
        .connection()?
        .query_row(
            "SELECT id FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 AND media_kind=?3",
            params![ws, content_hash, ext],
            |r| r.get(0),
        )
        .map_err(err)?;
    let value = DatasetVersion {
        schema_version: 1,
        artifact_id: Some(artifact_id),
        content_hash,
        external_reference: None,
        rows: table.len() - 1,
        columns,
        acquisition,
        diagnostics_coverage: "exhaustive captured rows".into(),
        delimiter: ext.into(),
        limitations: vec![
            "Missing: empty, NA, N/A, NaN, null, or dot. Values are never replaced with zero."
                .into(),
            "Inferred types are observations, not declared economic units.".into(),
        ],
    };
    desk::insert(
        store,
        ws,
        "dataset",
        title,
        serde_json::to_value(value).map_err(err)?,
        supersedes,
        operation,
    )
}
pub fn save_sample(
    store: &Store,
    ws: &str,
    title: &str,
    value: SampleDefinition,
    supersedes: Option<&str>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    desk::check_text(&value.inclusion_rules, 16000)?;
    desk::check_text(&value.unit_of_observation, 500)?;
    if value.origin != "declared"
        || value.datasets.is_empty()
        || value.datasets.len() > 32
        || value.filters.len() > 100
    {
        return Err(WorkbenchError::invalid(
            "Declare a sample over 1–32 exact dataset versions",
        ));
    }
    for r in &value.datasets {
        let d = desk::record(store, ws, &r.id)?;
        if d.kind != "dataset" || r.kind != "dataset" || d.content_hash != r.revision {
            return Err(WorkbenchError::invalid(
                "Sample references an invalid dataset version",
            ));
        }
    }
    if let Some(hash) = &value.membership_hash {
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(WorkbenchError::invalid("Membership hash must be SHA-256"));
        }
    }
    desk::insert(
        store,
        ws,
        "sample",
        title,
        serde_json::to_value(value).map_err(err)?,
        supersedes,
        operation,
    )
}
/// Policy applies before indexing or assembling context, including inferred diagnostics.
pub(crate) fn dictionary_text(body: &str, policy: &DataPolicy) -> WorkbenchResult<String> {
    let mut v: Value = serde_json::from_str(body).map_err(err)?;
    if !policy.summaries {
        if let Some(cols) = v["columns"].as_array_mut() {
            for c in cols {
                if let Some(c) = c.as_object_mut() {
                    c.remove("missing");
                }
            }
        }
    }
    serde_json::to_string(&v).map_err(err)
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataDataset {
    pub workspace_id: String,
    pub title: String,
    pub version: DatasetVersion,
    pub operation_id: String,
}
pub fn import_metadata(store: &Store, mut r: MetadataDataset) -> WorkbenchResult<DeskRecord> {
    if r.version.schema_version != 1
        || r.version.artifact_id.is_some()
        || r.version
            .external_reference
            .as_ref()
            .is_none_or(|s| s.trim().is_empty())
        || r.version.content_hash.len() != 64
        || !r
            .version
            .content_hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
        || r.version.columns.len() > 2000
    {
        return Err(WorkbenchError::invalid("Metadata exports require schemaVersion 1, an immutable external reference and SHA-256, with no local artifact ID"));
    }
    for c in &r.version.columns {
        desk::check_text(&c.name, 500)?;
        if c.origin != "declared" && c.origin != "exporter" {
            return Err(WorkbenchError::invalid(
                "External dictionaries must identify declared or exporter origin",
            ));
        }
    }
    r.version.limitations.push("External bytes were not read or verified by Pipeline. Dimensions and diagnostics are exporter declarations.".into());
    desk::insert(
        store,
        &r.workspace_id,
        "dataset",
        &r.title,
        serde_json::to_value(r.version).map_err(err)?,
        None,
        &r.operation_id,
    )
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowPreview {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub start: usize,
    pub total_rows: usize,
    pub coverage: String,
}
pub fn preview_rows(
    store: &Store,
    ws: &str,
    id: &str,
    start: usize,
    assistant: bool,
) -> WorkbenchResult<RowPreview> {
    if assistant && !policy(store, ws)?.assistant_rows {
        return Err(WorkbenchError::invalid(
            "Project policy does not permit assistant access to dataset rows",
        ));
    }
    let record = desk::record(store, ws, id)?;
    if record.kind != "dataset" {
        return Err(WorkbenchError::invalid("Choose a dataset version"));
    }
    let v: DatasetVersion = serde_json::from_value(record.body).map_err(err)?;
    let artifact = v
        .artifact_id
        .ok_or_else(|| WorkbenchError::invalid("External metadata has no captured local rows"))?;
    let path:String=store.connection()?.query_row("SELECT storage_reference FROM artifacts WHERE id=?1 AND workspace_id=?2 AND content_hash=?3",params![artifact,ws,v.content_hash],|r|r.get(0)).map_err(err)?;
    let canonical = Path::new(&path).canonicalize().map_err(err)?;
    if !canonical.starts_with(
        store
            .root_path()
            .join("blobs")
            .canonicalize()
            .map_err(err)?,
    ) {
        return Err(WorkbenchError::invalid("Dataset escaped blob storage"));
    }
    let mut bytes = Vec::new();
    fs::File::open(canonical)
        .map_err(err)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > MAX_BYTES || desk::hash(&bytes) != v.content_hash {
        return Err(WorkbenchError::invalid(
            "Dataset capture failed integrity verification",
        ));
    }
    let rows = parse(
        std::str::from_utf8(&bytes).map_err(err)?,
        if v.delimiter == "tsv" { b'\t' } else { b',' },
    )?;
    let preview = rows
        .iter()
        .skip(1 + start.min(v.rows))
        .take(25)
        .map(|r| {
            r.iter()
                .take(100)
                .map(|s| {
                    if missing(s) {
                        None
                    } else {
                        Some(crate::workbench::search::prefix(s, 500).to_string())
                    }
                })
                .collect()
        })
        .collect();
    Ok(RowPreview{columns:rows[0].iter().take(100).cloned().collect(),rows:preview,start,total_rows:v.rows,coverage:"Preview only: at most 25 rows, 100 columns, 500 bytes per cell; dictionary diagnostics cover all captured rows.".into()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_preserves_missing_unicode_and_newlines() {
        let rows = parse("name,x\r\n\"α,β\",\r\n\"two\nlines\",0\r\n", b',').unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1], vec!["α,β", ""]);
        assert_eq!(rows[2], vec!["two\nlines", "0"]);
        assert!(missing(&rows[1][1]));
        assert!(!missing(&rows[2][1]));
        assert!(parse("x,y\n1", b',').is_err());
        assert!(parse("x\n\"bad", b',').is_err());
    }
}

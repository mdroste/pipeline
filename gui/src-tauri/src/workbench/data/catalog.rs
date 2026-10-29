//! Dataset capture, bounded previews, exporter dictionaries and declared samples.
//!
//! Local files are read and profiled by `prepare_local` outside database
//! workers; `commit` stores the blob and immutable record inside one.
use super::{
    connectors::{Capture, ConnectorProvenance},
    csv, dta,
    profile::{fit_dictionary, Profile, RowPreview},
    xlsx, DataAcquisition, DatasetVersion, ImportDataset, MetadataDataset, SampleDefinition,
};
use crate::workbench::{
    desk::{self, err, DeskRecord},
    store::{Store, WorkbenchError, WorkbenchResult},
};
use rusqlite::params;
use std::{fs, io::Read, path::Path};

/// Captured bytes are retained in the store and parsed in memory.
const MAX_BYTES: u64 = 32 * 1024 * 1024;
/// Larger Stata files are catalogued by streaming, without keeping a copy.
const MAX_DICTIONARY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const FORMATS: [&str; 4] = ["csv", "tsv", "dta", "xlsx"];

/// A profiled capture waiting to be committed by a database worker.
pub struct PreparedDataset {
    workspace_id: String,
    title: String,
    ext: String,
    acquisition: DataAcquisition,
    supersedes: Option<String>,
    operation_id: String,
    /// Bytes retained as a blob; `None` for a dictionary-only capture.
    bytes: Option<Vec<u8>>,
    content_hash: String,
    external_reference: Option<String>,
    profile: Profile,
    connector: Option<ConnectorProvenance>,
}
fn profile_bytes(bytes: &[u8], ext: &str, sheet: Option<&str>) -> WorkbenchResult<Profile> {
    match ext {
        "dta" => Ok(dta::scan(bytes, bytes.len() as u64, None)?.profile),
        "xlsx" => Ok(xlsx::read(bytes, sheet, None)?.0),
        _ => csv::profile(bytes, ext),
    }
}
/// Read and profile a local file without touching the store.
pub fn prepare_local(r: ImportDataset) -> WorkbenchResult<PreparedDataset> {
    let path = Path::new(&r.path);
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !FORMATS.contains(&ext.as_str()) {
        return Err(WorkbenchError::invalid(
            "Import CSV, TSV, Stata .dta or Excel .xlsx data; for other formats import an exporter dictionary",
        ));
    }
    let file = crate::safety::open_regular_file(path)
        .map_err(|_| WorkbenchError::invalid("Select a regular dataset file"))?;
    let len = file.metadata().map_err(err)?.len();
    let mut prepared = PreparedDataset {
        workspace_id: r.workspace_id,
        title: r.title,
        ext: ext.clone(),
        acquisition: r.acquisition,
        supersedes: r.supersedes,
        operation_id: r.operation_id,
        bytes: None,
        content_hash: String::new(),
        external_reference: None,
        profile: Profile::default(),
        connector: None,
    };
    if len > MAX_BYTES {
        if ext != "dta" {
            return Err(WorkbenchError::invalid(
                "Dataset exceeds the 32 MiB local import limit",
            ));
        }
        if len > MAX_DICTIONARY_BYTES {
            return Err(WorkbenchError::invalid(
                "Stata files larger than 2 GiB cannot be catalogued; save a subset or import an exporter dictionary",
            ));
        }
        let scan = dta::scan(std::io::BufReader::with_capacity(1 << 20, file), len, None)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "dataset.dta".into());
        prepared.content_hash = scan.hash;
        prepared.profile = scan.profile;
        prepared.external_reference = Some(format!("{name} (local file; bytes not retained)"));
        return Ok(prepared);
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(WorkbenchError::invalid(
            "Dataset exceeds the 32 MiB local import limit",
        ));
    }
    prepared.profile = profile_bytes(&bytes, &ext, r.sheet.as_deref())?;
    prepared.content_hash = desk::hash(&bytes);
    prepared.bytes = Some(bytes);
    Ok(prepared)
}
pub fn import(store: &Store, r: ImportDataset) -> WorkbenchResult<DeskRecord> {
    commit(store, prepare_local(r)?)
}
fn store_blob(
    store: &Store,
    ws: &str,
    bytes: &[u8],
    ext: &str,
    content_hash: &str,
) -> WorkbenchResult<String> {
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
    store
        .connection()?
        .query_row(
            "SELECT id FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 AND media_kind=?3",
            params![ws, content_hash, ext],
            |r| r.get(0),
        )
        .map_err(err)
}
/// Store the blob (when retained) and the immutable dataset record.
pub fn commit(store: &Store, p: PreparedDataset) -> WorkbenchResult<DeskRecord> {
    let ws = p.workspace_id.as_str();
    store.workspace(ws)?;
    let retained = p.bytes.is_some();
    let mut limitations = p.profile.limitations;
    if !retained {
        limitations.push("Pipeline read every row once to build this dictionary but did not keep a copy of the file; row previews are unavailable and the SHA-256 identifies the bytes that were read.".into());
    }
    let mut version = DatasetVersion {
        schema_version: 1,
        artifact_id: None,
        content_hash: p.content_hash,
        external_reference: p.external_reference,
        rows: p.profile.rows,
        columns: p.profile.columns,
        acquisition: p.acquisition,
        diagnostics_coverage: if retained {
            "exhaustive captured rows"
        } else {
            "exhaustive rows read at capture; bytes not retained"
        }
        .into(),
        delimiter: p.ext,
        limitations,
        value_labels: p.profile.value_labels,
        source_format: p.profile.source_format,
        connector: p.connector,
    };
    // Size the record before writing a blob; the artifact ID adds well under the slack.
    fit_dictionary(&mut version)?;
    if let Some(bytes) = &p.bytes {
        version.artifact_id = Some(store_blob(
            store,
            ws,
            bytes,
            &version.delimiter,
            &version.content_hash,
        )?);
    }
    desk::insert(
        store,
        ws,
        "dataset",
        &p.title,
        serde_json::to_value(version).map_err(err)?,
        p.supersedes.as_deref(),
        &p.operation_id,
    )
}
/// In-memory capture used by tests; production captures use `commit` or `import_capture`.
#[cfg(test)]
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
    commit(
        store,
        PreparedDataset {
            workspace_id: ws.into(),
            title: title.into(),
            ext: ext.into(),
            acquisition,
            supersedes: supersedes.map(str::to_string),
            operation_id: operation.into(),
            content_hash: desk::hash(bytes),
            profile: profile_bytes(bytes, ext, None)?,
            bytes: Some(bytes.to_vec()),
            external_reference: None,
            connector: None,
        },
    )
}
/// Record a statistical-source capture with its terms, notice and citation.
pub(crate) fn import_capture(
    store: &Store,
    ws: &str,
    capture: Capture,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    store.workspace(ws)?;
    let mut profile = csv::profile(&capture.csv, "csv")?;
    profile.limitations.push(format!(
        "Retrieved from {} under the terms recorded in connector; repeat its notice and citation wherever these data appear.",
        capture.acquisition.provider
    ));
    commit(
        store,
        PreparedDataset {
            workspace_id: ws.into(),
            title: crate::workbench::search::prefix(&capture.title, 500).into(),
            ext: "csv".into(),
            acquisition: capture.acquisition,
            supersedes: None,
            operation_id: operation.into(),
            content_hash: desk::hash(&capture.csv),
            profile,
            bytes: Some(capture.csv),
            external_reference: None,
            connector: Some(capture.provenance),
        },
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
pub fn preview_rows(
    store: &Store,
    ws: &str,
    id: &str,
    start: usize,
    assistant: bool,
) -> WorkbenchResult<RowPreview> {
    if assistant && !super::policy(store, ws)?.assistant_rows {
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
    let unavailable = || WorkbenchError::invalid("Dataset preview is unavailable");
    let window = match v.delimiter.as_str() {
        "dta" => dta::scan(&bytes[..], bytes.len() as u64, Some(start))?
            .window
            .ok_or_else(unavailable)?,
        "xlsx" => {
            let sheet = v.source_format.as_ref().and_then(|f| f.sheet.as_deref());
            xlsx::read(&bytes, sheet, Some(start))?
                .1
                .ok_or_else(unavailable)?
        }
        ext => csv::window(&bytes, ext, start)?,
    };
    Ok(RowPreview {
        columns: window.columns,
        rows: window.rows,
        start,
        total_rows: v.rows,
        coverage: "Preview only: at most 25 rows, 100 columns, 500 bytes per cell; dictionary diagnostics cover all captured rows.".into(),
        missing_codes: window.missing_codes,
    })
}

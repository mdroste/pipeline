//! Statistical data connectors. Each connector validates its query, names its
//! bounded HTTPS requests, and interprets the responses without network access,
//! so parsing is tested from recorded fixtures. `fetch` performs the requests
//! outside database workers with the acquisition client (30-second timeout, no
//! redirects); callers record the capture through the data catalog.
mod dbnomics;
mod fred;
#[cfg(test)]
mod tests;
mod world_bank;

pub use dbnomics::DbnomicsQuery;
pub use fred::FredRequest;
pub use world_bank::WorldBankQuery;

use super::DataAcquisition;
use crate::workbench::{
    acquisition::{self, Fetched},
    desk::{self, DeskRecord},
    store::{Store, WorkbenchError, WorkbenchResult},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// When the recorded licence, notice and citation text was last checked
/// against each service's published terms.
pub(crate) const TERMS_REVIEWED: &str = "2026-10-28";

/// Terms, notice and citation stored on every connector capture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorProvenance {
    /// `fred`, `dbnomics` or `world_bank`.
    pub connector: String,
    /// Exact request parameters, without credentials.
    pub query: Value,
    /// Request URLs, without credentials.
    pub request_urls: Vec<String>,
    /// The source's own "last updated" or indexing time, when it reports one.
    pub last_updated: Option<String>,
    pub licence: String,
    /// Text the source requires wherever its data are shown.
    pub notice: Option<String>,
    pub citation: String,
    pub terms_url: String,
    pub terms_reviewed: String,
}

pub(crate) struct Request {
    pub url: reqwest::Url,
    pub max_bytes: usize,
}
/// One completed capture: CSV bytes plus provenance.
#[derive(Debug)]
pub(crate) struct Capture {
    pub title: String,
    pub csv: Vec<u8>,
    pub acquisition: DataAcquisition,
    pub provenance: ConnectorProvenance,
}
pub(crate) trait DataConnector {
    /// Provider label recorded on dataset versions and failed receipts.
    fn provider(&self) -> &'static str;
    /// Secret-free description of the query for receipts.
    fn query(&self) -> Value;
    /// Validated requests, issued in order.
    fn requests(&self) -> WorkbenchResult<Vec<Request>>;
    /// Interpret responses to `requests`; never performs network access.
    fn capture(&self, responses: Vec<Fetched>, retrieved_at: &str) -> WorkbenchResult<Capture>;
}
pub(crate) async fn fetch<C: DataConnector + Sync>(connector: &C) -> WorkbenchResult<Capture> {
    let requests = connector.requests()?;
    let client = acquisition::client()?;
    let mut responses = Vec::with_capacity(requests.len());
    for request in requests {
        responses.push(acquisition::fetch(client.get(request.url), request.max_bytes).await?);
    }
    connector.capture(responses, &desk::now())
}

/// Keyless series captures accepted by `workbench_acquire_series`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "connector", rename_all = "snake_case")]
pub enum SeriesQuery {
    Dbnomics(DbnomicsQuery),
    WorldBank(WorldBankQuery),
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesRequest {
    pub workspace_id: String,
    pub operation_id: String,
    pub query: SeriesQuery,
}
pub(crate) async fn acquire_series(query: &SeriesQuery) -> WorkbenchResult<Capture> {
    match query {
        SeriesQuery::Dbnomics(q) => fetch(q).await,
        SeriesQuery::WorldBank(q) => fetch(q).await,
    }
}
impl SeriesQuery {
    fn connector(&self) -> &dyn DataConnector {
        match self {
            Self::Dbnomics(q) => q,
            Self::WorldBank(q) => q,
        }
    }
}
/// Record a capture as a dataset version, or a failure as an acquisition receipt.
pub(crate) fn record_series(
    store: &Store,
    ws: &str,
    query: &SeriesQuery,
    result: WorkbenchResult<Capture>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    let connector = query.connector();
    match result {
        Ok(capture) => super::import_capture(store, ws, capture, operation),
        Err(e) => desk::insert(
            store,
            ws,
            "acquisition",
            connector.provider(),
            json!({"provider":connector.provider(),"query":connector.query(),"retrievedAt":desk::now(),"state":"failed","error":e.message}),
            None,
            operation,
        ),
    }
}

/// A non-success status with the service's own message, when it sent one.
pub(super) fn status_error(
    provider: &str,
    response: &Fetched,
    message: Option<&str>,
) -> WorkbenchError {
    let detail = message
        .map(|m| format!(": {}", crate::workbench::search::prefix(m.trim(), 300)))
        .unwrap_or_default();
    WorkbenchError::invalid(format!(
        "{provider} returned HTTP {}{detail}; no data were imported",
        response.status
    ))
}
pub(super) fn parse_json(provider: &str, response: &Fetched) -> WorkbenchResult<Value> {
    serde_json::from_slice(&response.body)
        .map_err(|_| WorkbenchError::invalid(format!("{provider} returned malformed JSON")))
}
/// RFC 4180 CSV with every field quoted.
pub(super) fn csv(header: &[&str], rows: impl IntoIterator<Item = Vec<String>>) -> Vec<u8> {
    let mut out = header.join(",");
    out.push('\n');
    for row in rows {
        let fields = row
            .iter()
            .map(|f| format!("\"{}\"", f.replace('"', "\"\"")))
            .collect::<Vec<_>>();
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out.into_bytes()
}
/// JSON scalar as CSV text: numbers keep their decimal form, null is empty.
pub(super) fn scalar(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
/// `October 28, 2026` for citations.
pub(super) fn citation_date(retrieved_at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(retrieved_at)
        .map(|d| d.format("%B %-d, %Y").to_string())
        .unwrap_or_else(|_| retrieved_at.to_string())
}

//! Host-owned literature acquisition. Network waits occur outside database workers.
use super::{
    desk::{self, err, DeskRecord},
    research,
    store::{Store, WorkbenchError, WorkbenchResult},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub title: String,
    pub doi: Option<String>,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub url: Option<String>,
    pub abstract_text: Option<String>,
    pub metadata: Value,
}
pub fn network_enabled(store: &Store, ws: &str) -> WorkbenchResult<bool> {
    store.workspace(ws)?;
    Ok(store
        .connection()?
        .query_row(
            "SELECT enabled FROM acquisition_settings WHERE workspace_id=?1",
            [ws],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?
        .unwrap_or(false))
}
pub fn set_network(store: &Store, ws: &str, enabled: bool) -> WorkbenchResult<bool> {
    store.workspace(ws)?;
    store.connection()?.execute("INSERT INTO acquisition_settings VALUES(?1,?2) ON CONFLICT(workspace_id) DO UPDATE SET enabled=?2",params![ws,enabled]).map_err(err)?;
    Ok(enabled)
}
pub fn normalize_doi(value: &str) -> Option<String> {
    let s = value.trim().to_lowercase();
    let s = s
        .strip_prefix("https://doi.org/")
        .or_else(|| s.strip_prefix("http://dx.doi.org/"))
        .or_else(|| s.strip_prefix("doi:"))
        .unwrap_or(&s)
        .trim();
    if s.starts_with("10.")
        && s.contains('/')
        && !s.chars().any(char::is_whitespace)
        && s.len() <= 300
    {
        Some(s.into())
    } else {
        None
    }
}
fn client() -> WorkbenchResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Pipeline-Academic-Workspace/1.0")
        .build()
        .map_err(|_| WorkbenchError::invalid("Unable to initialize acquisition client"))
}
async fn bytes(request: reqwest::RequestBuilder, max: usize) -> WorkbenchResult<(Vec<u8>, String)> {
    let mut response = request
        .send()
        .await
        .map_err(|_| WorkbenchError::invalid("Acquisition connection failed or timed out"))?;
    if !response.status().is_success() {
        return Err(WorkbenchError::invalid(format!(
            "Acquisition returned HTTP {}; no content was imported",
            response.status().as_u16()
        )));
    }
    if response.content_length().is_some_and(|n| n > max as u64) {
        return Err(WorkbenchError::invalid(
            "Acquisition response exceeds its byte limit",
        ));
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut data = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| WorkbenchError::invalid("Acquisition response was interrupted"))?
    {
        if data.len() + chunk.len() > max {
            return Err(WorkbenchError::invalid(
                "Acquisition response exceeds its byte limit",
            ));
        }
        data.extend_from_slice(&chunk);
    }
    Ok((data, mime))
}
pub async fn crossref(query: &str) -> WorkbenchResult<Vec<Candidate>> {
    desk::check_text(query, 1000)?;
    let client = client()?;
    let request = if let Some(doi) = normalize_doi(query) {
        let mut url = reqwest::Url::parse("https://api.crossref.org/works/").map_err(err)?;
        url.path_segments_mut()
            .map_err(|_| WorkbenchError::invalid("Invalid Crossref endpoint"))?
            .pop_if_empty()
            .push(&doi);
        client.get(url)
    } else {
        client
            .get("https://api.crossref.org/works")
            .query(&[("query.bibliographic", query), ("rows", "20")])
    };
    let (body, mime) = bytes(request, 2 * 1024 * 1024).await?;
    if !mime.contains("json") {
        return Err(WorkbenchError::invalid(
            "Crossref returned non-JSON metadata",
        ));
    }
    let data: Value = serde_json::from_slice(&body).map_err(err)?;
    let items = data["message"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| vec![data["message"].clone()]);
    items.into_iter().take(20).map(|m|{let title=m["title"][0].as_str().unwrap_or("Untitled work").to_string();let doi=m["DOI"].as_str().and_then(normalize_doi);let authors=m["author"].as_array().into_iter().flatten().take(100).map(|a|format!("{} {}",a["given"].as_str().unwrap_or(""),a["family"].as_str().unwrap_or(""))).collect();
        let abstract_text=m["abstract"].as_str().map(|s|crate::workbench::search::prefix(s,16000).to_string());
        Ok(Candidate{title,doi,authors,year:m["issued"]["date-parts"][0][0].as_i64(),url:m["URL"].as_str().map(str::to_string),abstract_text,metadata:json!({"type":m["type"],"containerTitle":m["container-title"],"license":m["license"],"relation":m["relation"],"published":m["published"]})})}).collect()
}
pub fn record_lookup(
    store: &Store,
    ws: &str,
    query: &str,
    result: WorkbenchResult<Vec<Candidate>>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    if let Some(previous) = desk::operation_record(store, ws, operation)? {
        if previous.kind == "acquisition"
            && previous.body["provider"] == "crossref"
            && previous.body["query"] == query
        {
            return Ok(previous);
        }
        return Err(WorkbenchError::conflict(
            "Acquisition operation was reused with different inputs",
        ));
    }
    let body = match result {
        Ok(candidates) => {
            json!({"provider":"crossref","query":query,"retrievedAt":desk::now(),"state":"new","access":"metadata","candidates":candidates})
        }
        Err(e) => {
            json!({"provider":"crossref","query":query,"retrievedAt":desk::now(),"state":"failed","error":e.message})
        }
    };
    desk::insert(
        store,
        ws,
        "acquisition",
        super::search::prefix(query, 500),
        body,
        None,
        operation,
    )
}
pub fn import_candidate(
    store: &Store,
    ws: &str,
    receipt: &str,
    index: usize,
    citation_key: Option<String>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    if let Some(previous) = desk::operation_record(store, ws, operation)? {
        if previous.kind == "acquisition"
            && previous.body["candidateReceiptId"] == receipt
            && previous.body["candidateIndex"] == index
            && previous.body["citationKey"] == json!(citation_key)
        {
            return Ok(previous);
        }
        return Err(WorkbenchError::conflict(
            "Source import operation was reused with different inputs",
        ));
    }
    let r = desk::record(store, ws, receipt)?;
    if r.kind != "acquisition" {
        return Err(WorkbenchError::invalid("Select an acquisition receipt"));
    }
    let c: Candidate = serde_json::from_value(
        r.body["candidates"]
            .get(index)
            .cloned()
            .ok_or_else(|| WorkbenchError::invalid("Candidate is unavailable"))?,
    )
    .map_err(err)?;
    // Retry the source operation without creating an extra identity.
    let previous: Option<String> = store
        .connection()?
        .query_row(
            "SELECT entity_id FROM change_log WHERE operation_id=?1 AND entity_type='source'",
            [format!("{operation}-source")],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    let source = if let Some(id) = previous {
        research::source_by_id(store, ws, &id)?
    } else {
        research::import_source(store,research::ImportSourceRequest{workspace_id:ws.into(),title:c.title.clone(),citation_key:citation_key.clone(),identifiers:json!({"doi":c.doi,"authors":c.authors,"year":c.year,"metadata":c.metadata}),version_label:c.year.map(|y|y.to_string()),path:None,locator:c.url.clone(),access_state:"metadata".into(),acquired_via:"web".into(),operation_id:format!("{operation}-source")})?.source
    };
    if let Some(text) = c.abstract_text {
        let hash = desk::hash(text.as_bytes());
        let path = store.root_path().join("blobs").join(format!("{hash}.txt"));
        if !path.exists() {
            std::fs::write(&path, text).map_err(err)?;
        }
        store.connection()?.execute("UPDATE source_versions SET text_reference=?2,content_hash=?3,access_state='abstract' WHERE id=?1 AND text_reference IS NULL",params![source.version_id,path.to_string_lossy(),hash]).map_err(err)?;
    }
    desk::insert(
        store,
        ws,
        "acquisition",
        &c.title,
        json!({"provider":"crossref","query":r.body["query"],"retrievedAt":r.body["retrievedAt"],"candidateReceiptId":receipt,"candidateIndex":index,"citationKey":citation_key,"state":"metadata_only","sourceId":source.id,"sourceVersionId":source.version_id,"canonicalUrl":c.url,"doi":c.doi,"access":"metadata_or_abstract"}),
        None,
        operation,
    )
}
fn safe_url(value: &str) -> WorkbenchResult<reqwest::Url> {
    let url =
        reqwest::Url::parse(value).map_err(|_| WorkbenchError::invalid("Enter an HTTPS URL"))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none_or(|h| {
            h == "localhost" || h.ends_with(".local") || h.parse::<std::net::IpAddr>().is_ok()
        })
    {
        return Err(WorkbenchError::invalid(
            "Acquisition needs a public HTTPS hostname without embedded credentials",
        ));
    }
    Ok(url)
}
pub async fn download_pdf(url: &str) -> WorkbenchResult<Vec<u8>> {
    let url = safe_url(url)?;
    let (data, mime) = bytes(client()?.get(url), 32 * 1024 * 1024).await?;
    // Redirects intentionally require the researcher to inspect and enter the final URL.
    if !mime.starts_with("application/pdf") || !data.starts_with(b"%PDF-") {
        return Err(WorkbenchError::invalid("URL did not return a PDF. A landing page or blocked download is not full text; enter the final PDF URL."));
    }
    tokio::task::spawn_blocking(move || {
        validate_pdf_pages(&data)?;
        Ok(data)
    })
    .await
    .map_err(err)?
}
/// Remote PDFs must have a bounded, readable page tree before entering the store.
/// This probe runs outside the database worker and never acquires its gate.
pub(crate) fn validate_pdf_pages(data: &[u8]) -> WorkbenchResult<usize> {
    use std::io::Write;
    let bin = crate::deps::resolve_command("pdfinfo").ok_or_else(|| {
        WorkbenchError::invalid("Install Poppler (pdfinfo) to check acquired PDFs before import")
    })?;
    let mut file = tempfile::Builder::new()
        .prefix("pipeline-acquired-")
        .suffix(".pdf")
        .tempfile()
        .map_err(err)?;
    file.write_all(data).map_err(err)?;
    let mut command = bin.command([file.path().as_os_str()]);
    command.env("LC_ALL", "C");
    let output = crate::process::run_bounded(&mut command, Duration::from_secs(10), 16 * 1024)
        .map_err(|_| WorkbenchError::invalid("PDF page inspection failed or timed out"))?;
    if !output.status.success() || output.stdout_truncated {
        return Err(WorkbenchError::invalid(
            "Acquired PDF has an unreadable page tree",
        ));
    }
    let pages = String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("Pages:")
                .and_then(|n| n.trim().parse::<usize>().ok())
        })
        .ok_or_else(|| WorkbenchError::invalid("Acquired PDF page count is unavailable"))?;
    if !(1..=300).contains(&pages) {
        return Err(WorkbenchError::invalid(
            "Acquired PDFs must contain between 1 and 300 pages",
        ));
    }
    Ok(pages)
}
pub fn record_pdf(
    store: &Store,
    ws: &str,
    title: &str,
    url: &str,
    result: WorkbenchResult<Vec<u8>>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    if let Some(previous) = desk::operation_record(store, ws, operation)? {
        if previous.kind == "acquisition"
            && previous.title == title
            && previous.body["provider"] == "explicit_url"
            && previous.body["canonicalUrl"] == url
        {
            return Ok(previous);
        }
        return Err(WorkbenchError::conflict(
            "PDF acquisition operation was reused with different inputs",
        ));
    }
    let body = match result {
        Err(e) => {
            json!({"provider":"explicit_url","canonicalUrl":url,"retrievedAt":desk::now(),"state":"failed","access":"unavailable","error":e.message})
        }
        Ok(bytes) => {
            let hash = desk::hash(&bytes);
            let path = store.root_path().join("blobs").join(format!("{hash}.pdf"));
            if !path.exists() {
                std::fs::write(&path, bytes).map_err(err)?;
            }
            let imported = research::import_source(
                store,
                research::ImportSourceRequest {
                    workspace_id: ws.into(),
                    title: title.into(),
                    citation_key: None,
                    identifiers: json!({"canonicalUrl":url}),
                    version_label: None,
                    path: Some(path.to_string_lossy().into_owned()),
                    locator: Some(url.into()),
                    access_state: "partial".into(),
                    acquired_via: "web".into(),
                    operation_id: format!("{operation}-source"),
                },
            )?;
            store
                .connection()?
                .execute(
                    "UPDATE source_versions SET locator=?2 WHERE id=?1",
                    params![imported.source.version_id, url],
                )
                .map_err(err)?;
            json!({"provider":"explicit_url","canonicalUrl":url,"retrievedAt":desk::now(),"contentHash":hash,"state":if imported.source.access_state=="unavailable"{"failed"}else{"ready_to_read"},"access":imported.source.access_state,"sourceId":imported.source.id,"sourceVersionId":imported.source.version_id,"limitations":["PDF text extraction may omit equations and pages; inspect the captured PDF."]})
        }
    };
    desk::insert(store, ws, "acquisition", title, body, None, operation)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FredRequest {
    pub workspace_id: String,
    pub series_id: String,
    pub vintage: String,
    pub api_key: String,
    pub operation_id: String,
}
pub async fn fred(r: &FredRequest) -> WorkbenchResult<(Vec<u8>, super::data::DataAcquisition)> {
    if r.series_id.is_empty()
        || r.series_id.len() > 100
        || !r
            .series_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(WorkbenchError::invalid("Invalid FRED series ID"));
    }
    chrono::NaiveDate::parse_from_str(&r.vintage, "%Y-%m-%d")
        .map_err(|_| WorkbenchError::invalid("Choose an exact vintage date (YYYY-MM-DD)"))?;
    if r.api_key.len() != 32 || !r.api_key.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(WorkbenchError::invalid(
            "Enter a FRED API key; it is used only for this request",
        ));
    }
    let client = client()?;
    let params = [
        ("series_id", r.series_id.as_str()),
        ("api_key", r.api_key.as_str()),
        ("file_type", "json"),
        ("realtime_start", r.vintage.as_str()),
        ("realtime_end", r.vintage.as_str()),
    ];
    let (meta, _) = bytes(
        client
            .get("https://api.stlouisfed.org/fred/series")
            .query(&params),
        1024 * 1024,
    )
    .await?;
    let meta: Value = serde_json::from_slice(&meta)
        .map_err(|_| WorkbenchError::invalid("Invalid FRED series metadata"))?;
    let (observations, _) = bytes(
        client
            .get("https://api.stlouisfed.org/fred/series/observations")
            .query(&params)
            .query(&[("limit", "100000"), ("units", "lin")]),
        16 * 1024 * 1024,
    )
    .await?;
    let v: Value = serde_json::from_slice(&observations)
        .map_err(|_| WorkbenchError::invalid("Invalid FRED observations"))?;
    let rows = v["observations"]
        .as_array()
        .ok_or_else(|| WorkbenchError::invalid("FRED returned no observation array"))?;
    if v["count"].as_u64() != Some(rows.len() as u64) {
        return Err(WorkbenchError::invalid(
            "FRED response is incomplete; this adapter refuses truncated series",
        ));
    }
    let mut csv = String::from("date,value,realtime_start,realtime_end\n");
    for row in rows {
        let fields = ["date", "value", "realtime_start", "realtime_end"]
            .map(|k| format!("\"{}\"", row[k].as_str().unwrap_or("").replace('"', "\"\"")));
        csv.push_str(&fields.join(","));
        csv.push('\n');
    }
    let info = &meta["seriess"][0];
    let acquisition = super::data::DataAcquisition {
        provider: "FRED/ALFRED".into(),
        source: format!("https://fred.stlouisfed.org/series/{}", r.series_id),
        retrieved_at: desk::now(),
        requested_vintage: Some(r.vintage.clone()),
        returned_vintage: v["realtime_start"].as_str().map(str::to_string),
        series_ids: vec![r.series_id.clone()],
        units: info["units"].as_str().map(str::to_string),
        frequency: info["frequency"].as_str().map(str::to_string),
        transformation: Some("lin (untransformed)".into()),
    };
    Ok((csv.into_bytes(), acquisition))
}
pub fn inbox_state(
    store: &Store,
    ws: &str,
    id: &str,
    state: &str,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    if !["read", "excluded", "ready_to_read", "metadata_only"].contains(&state) {
        return Err(WorkbenchError::invalid("Invalid reading inbox state"));
    }
    let previous = desk::record(store, ws, id)?;
    if previous.kind != "acquisition" || previous.body["sourceVersionId"].as_str().is_none() {
        return Err(WorkbenchError::invalid(
            "Import a source before changing its reading state",
        ));
    }
    if state == "ready_to_read" && previous.body["access"] == "metadata_or_abstract" {
        return Err(WorkbenchError::invalid("Attach readable full text first"));
    }
    let mut body = previous.body;
    body["state"] = json!(state);
    desk::insert(
        store,
        ws,
        "acquisition",
        &previous.title,
        body,
        Some(id),
        operation,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "live public Crossref API; run explicitly when qualifying acquisition"]
    async fn live_crossref_doi_lookup() {
        let results = crossref("10.1257/aer.20151086").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doi.as_deref(), Some("10.1257/aer.20151086"));
        assert!(!results[0].title.is_empty());
    }
    #[test]
    fn normalized_doi_and_url_boundaries() {
        assert_eq!(
            normalize_doi("https://doi.org/10.1000/ABC"),
            Some("10.1000/abc".into())
        );
        assert_eq!(normalize_doi("not a DOI"), None);
        assert!(safe_url("file:///tmp/a.pdf").is_err());
        assert!(safe_url("https://user:secret@example.com/a.pdf").is_err());
        assert!(safe_url("https://127.0.0.1/a.pdf").is_err());
    }
}

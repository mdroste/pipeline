use super::*;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BibEntry {
    pub id: String,
    pub key: String,
    pub entry_type: String,
    pub fields: BTreeMap<String, String>,
    pub raw: String,
    pub start: usize,
    pub end: usize,
    pub warnings: Vec<String>,
}
fn split_top_level(text: &str, delimiter: u8) -> WorkbenchResult<Vec<&str>> {
    let mut depth = 0i32;
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0;
    let mut parts = Vec::new();
    for (i, b) in text.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if b == b'\\' {
            escaped = true;
            continue;
        }
        if b == b'"' && depth == 0 {
            quoted = !quoted;
            continue;
        }
        if !quoted {
            match b {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth < 0 {
                        return Err(WorkbenchError::invalid("Unbalanced BibTeX field"));
                    }
                }
                _ => {}
            }
        }
        if b == delimiter && depth == 0 && !quoted {
            parts.push(&text[start..i]);
            start = i + 1;
        }
    }
    if depth != 0 || quoted {
        return Err(WorkbenchError::invalid("Unterminated BibTeX field"));
    }
    parts.push(&text[start..]);
    Ok(parts)
}
pub fn parse_bibtex(text: &str) -> WorkbenchResult<Vec<BibEntry>> {
    if text.len() > 2 * 1024 * 1024 {
        return Err(WorkbenchError::invalid("Bibliography exceeds 2 MiB"));
    }
    let bytes = text.as_bytes();
    let mut cursor = 0;
    let mut entries = Vec::new();
    let mut keys = BTreeSet::new();
    while cursor < bytes.len() {
        if bytes[cursor] == b'%' {
            cursor += text[cursor..].find('\n').unwrap_or(bytes.len() - cursor);
            if cursor < bytes.len() {
                cursor += 1;
            }
            continue;
        }
        if bytes[cursor] != b'@' {
            cursor += 1;
            continue;
        }
        let start = cursor;
        cursor += 1;
        let kind_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_alphabetic() {
            cursor += 1;
        }
        let kind = text[kind_start..cursor].to_ascii_lowercase();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || !matches!(bytes[cursor], b'{' | b'(') {
            return Err(WorkbenchError::invalid(
                "BibTeX entry needs an opening brace or parenthesis",
            ));
        }
        let open = bytes[cursor];
        let close = if open == b'{' { b'}' } else { b')' };
        cursor += 1;
        let body_start = cursor;
        let mut depth = 0i32;
        let mut quote = false;
        let mut escaped = false;
        while cursor < bytes.len() {
            let b = bytes[cursor];
            if escaped {
                escaped = false;
                cursor += 1;
                continue;
            }
            if b == b'\\' {
                escaped = true;
                cursor += 1;
                continue;
            }
            if b == b'"' && depth == 0 {
                quote = !quote;
                cursor += 1;
                continue;
            }
            if !quote {
                if b == close && depth == 0 {
                    break;
                }
                if b == b'{' {
                    depth += 1;
                } else if b == b'}' {
                    depth -= 1;
                }
            }
            cursor += 1;
        }
        if cursor >= bytes.len() {
            return Err(WorkbenchError::invalid("Unterminated BibTeX entry"));
        }
        let body = &text[body_start..cursor];
        cursor += 1;
        let raw = text[start..cursor].to_string();
        if matches!(kind.as_str(), "comment" | "preamble" | "string") {
            continue;
        }
        let parts = split_top_level(body, b',')?;
        let key = parts[0].trim().to_string();
        bounded(&key, 300)?;
        let mut fields = BTreeMap::new();
        let mut warnings = Vec::new();
        if !keys.insert(key.clone()) {
            warnings.push("Duplicate citation key; entries remain distinct".into());
        }
        for field in parts.iter().skip(1).filter(|s| !s.trim().is_empty()) {
            let (name, value) = field
                .split_once('=')
                .ok_or_else(|| WorkbenchError::invalid("BibTeX field lacks '='"))?;
            let name = name.trim().to_ascii_lowercase();
            bounded(&name, 100)?;
            let value = value.trim();
            let value = if value.starts_with('{') && value.ends_with('}')
                || value.starts_with('"') && value.ends_with('"')
            {
                value[1..value.len() - 1].to_string()
            } else {
                warnings.push(format!(
                    "{name}: bare value or macro retained without expansion"
                ));
                value.to_string()
            };
            if fields.insert(name.clone(), value).is_some() {
                return Err(WorkbenchError::invalid(format!(
                    "Duplicate field {name}; original bibliography remains unchanged"
                )));
            }
        }
        entries.push(BibEntry {
            id: hash(raw.as_bytes()),
            key,
            entry_type: kind,
            fields,
            raw,
            start,
            end: cursor,
            warnings,
        });
        if entries.len() > 2000 {
            return Err(WorkbenchError::invalid("Bibliography exceeds 2000 entries"));
        }
    }
    Ok(entries)
}
pub fn preview_bibliography(store: &Store, ws: &str, revision: &str) -> WorkbenchResult<Value> {
    let entries = parse_bibtex(&exact_text(store, ws, revision)?)?;
    let existing = records(store, ws, "bibliography")?;
    let duplicates = entries
        .iter()
        .filter(|e| existing.iter().any(|r| r.body["entry"]["key"] == e.key))
        .map(|e| e.key.clone())
        .collect::<BTreeSet<_>>();
    Ok(
        json!({"revisionId":revision,"entries":entries,"existingKeys":duplicates,"limitations":"Original directives, unknown fields and macro expressions are retained in the immutable bibliography. Macro expansion is not performed."}),
    )
}
pub(super) fn import_bibliography(
    store: &Store,
    ws: &str,
    revision: &str,
    selected: &[String],
) -> WorkbenchResult<Value> {
    let entries = parse_bibtex(&exact_text(store, ws, revision)?)?;
    if selected.is_empty() || selected.iter().any(|s| !entries.iter().any(|e| &e.id == s)) {
        return Err(WorkbenchError::invalid(
            "Select entries from the bibliography preview",
        ));
    }
    let mut result = Vec::new();
    for entry in entries.into_iter().filter(|e| selected.contains(&e.id)) {
        let object_id = format!("bib_{}", hash(format!("{ws}\0{}", entry.id).as_bytes()));
        if let Ok(old) = record(store, ws, &object_id, "bibliography") {
            result.push(old);
            continue;
        }
        let source = research::import_source(
            store,
            research::ImportSourceRequest {
                workspace_id: ws.into(),
                title: entry
                    .fields
                    .get("title")
                    .cloned()
                    .unwrap_or_else(|| entry.key.clone()),
                citation_key: Some(entry.key.clone()),
                identifiers: entry
                    .fields
                    .get("doi")
                    .map(|doi| json!({"doi":doi,"bibtexEntryHash":entry.id}))
                    .unwrap_or_else(|| json!({"bibtexEntryHash":entry.id})),
                version_label: entry.fields.get("year").cloned(),
                path: None,
                locator: entry.fields.get("url").cloned(),
                access_state: "metadata".into(),
                acquired_via: "local_bibtex".into(),
                operation_id: id("bib_source")?,
            },
        )?;
        result.push(put(store,ws,&object_id,"bibliography",0,&json!({"origin":"bibtex","revisionId":revision,"entry":entry,"source":source.source,"duplicateCandidates":source.duplicate_candidates,"projectNotesStoredSeparately":true}))?);
    }
    Ok(json!(result))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiteratureNote {
    pub question: String,
    pub statement: String,
    pub citation_key: String,
    pub source_version_id: Option<String>,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub quote: String,
    pub identity_checked: bool,
    pub support: String,
    pub method: String,
    pub related_version_ids: Vec<String>,
}
fn source_identity(store: &Store, ws: &str, version: &str) -> WorkbenchResult<Value> {
    type SourceIdentityRow = (String, String, String, Option<String>, Option<String>);
    let row:Option<SourceIdentityRow>=store.connection()?.query_row("SELECT s.title,v.access_state,v.acquired_via,v.content_hash,v.version_label FROM source_versions v JOIN sources s ON s.id=v.source_id WHERE v.id=?1 AND s.workspace_id=?2",params![version,ws],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(err)?;
    let (title, access, method, digest, label) = row
        .ok_or_else(|| WorkbenchError::invalid("Source version is unavailable in this project"))?;
    Ok(
        json!({"versionId":version,"title":title,"access":access,"acquiredVia":method,"contentHash":digest,"versionLabel":label}),
    )
}
pub(super) fn save_literature(
    store: &Store,
    ws: &str,
    object_id: Option<&str>,
    expected: i64,
    n: LiteratureNote,
) -> WorkbenchResult<ProjectRecord> {
    bounded(&n.question, 4000)?;
    bounded(&n.statement, 32000)?;
    if !matches!(
        n.support.as_str(),
        "unsupported" | "relevant" | "supports" | "contradicts"
    ) || !matches!(
        n.method.as_str(),
        "manual" | "model_assessment" | "deterministic_identity"
    ) || n.related_version_ids.len() > 30
    {
        return Err(WorkbenchError::invalid("Unknown source-support assessment"));
    }
    let mut source = None;
    if let Some(version) = &n.source_version_id {
        source = Some(source_identity(store, ws, version)?);
        if !n.quote.is_empty() {
            let start = n
                .start
                .ok_or_else(|| WorkbenchError::invalid("Passage needs an exact byte start"))?;
            let end = n
                .end
                .ok_or_else(|| WorkbenchError::invalid("Passage needs an exact byte end"))?;
            if end <= start || end - start > 64 * 1024 {
                return Err(WorkbenchError::invalid("Passage exceeds 64 KiB"));
            }
            let read = research::source_read(store, ws, version, start, end - start)?;
            if read.text != n.quote {
                return Err(WorkbenchError::invalid(
                    "Passage does not match this exact source version",
                ));
            }
        }
    }
    if n.support != "unsupported"
        && (n.quote.is_empty()
            || source
                .as_ref()
                .is_none_or(|s| matches!(s["access"].as_str(), Some("metadata" | "unavailable"))))
    {
        return Err(WorkbenchError::invalid("Support requires an exact accessible passage; metadata-only entries remain unsupported"));
    }
    for version in &n.related_version_ids {
        source_identity(store, ws, version)?;
    }
    put(
        store,
        ws,
        &new_or_id(object_id)?,
        "literature",
        expected,
        &json!({"note":n,"source":source,"checklist":{"identity":if n.identity_checked{"researcher_checked"}else{"unknown"},"access":source.as_ref().map(|s|s["access"].clone()).unwrap_or(json!("unavailable")),"passageRelevance":n.support,"assessmentMethod":n.method,"freshness":"exact retained version; later versions require a new assessment"}}),
    )
}
pub fn citation_navigation(store: &Store, ws: &str, revision: &str) -> WorkbenchResult<Value> {
    let text = exact_text(store, ws, revision)?;
    let re = regex::Regex::new(r"\\(?:[A-Za-z]*cite[A-Za-z]*)(?:\[[^\]]*\])*\{([^}]+)\}")
        .expect("citation regex");
    let bib = records(store, ws, "bibliography")?;
    let notes = records(store, ws, "literature")?;
    let mut citations = Vec::new();
    for capture in re.captures_iter(&text).take(2000) {
        let whole = capture.get(0).expect("whole citation");
        for key in capture[1].split(',').map(str::trim) {
            citations.push(json!({"key":key,"start":whole.start(),"end":whole.end(),"sources":bib.iter().filter(|b|b.body["entry"]["key"]==key).collect::<Vec<_>>(),"notes":notes.iter().filter(|n|n.body["note"]["citationKey"]==key).collect::<Vec<_>>()}));
        }
    }
    Ok(
        json!({"revisionId":revision,"citations":citations,"coverage":"Literal TeX citation commands; macros or generated citation keys may require manual lookup"}),
    )
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroPreview {
    pub server_id: Option<String>,
    pub collection: String,
    pub start: usize,
    pub items: Vec<Value>,
    pub has_more: bool,
}
/// Fixed loopback endpoint, GET only, bounded response; no credentials or redirects.
pub async fn zotero_preview(
    collection: Option<String>,
    start: usize,
    expected_server: Option<String>,
) -> WorkbenchResult<ZoteroPreview> {
    if start > 100_000 {
        return Err(WorkbenchError::invalid("Zotero page offset is too large"));
    }
    if collection.as_ref().is_some_and(|s| {
        s.len() != 8
            || !s
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    }) {
        return Err(WorkbenchError::invalid(
            "Choose an eight-character Zotero collection key",
        ));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(err)?;
    let endpoint = collection
        .as_ref()
        .map(|c| format!("users/0/collections/{c}/items/top"))
        .unwrap_or_else(|| "users/0/collections".into());
    let mut request = client
        .get(format!(
            "http://127.0.0.1:23119/api/{endpoint}?format=json&limit=100&start={start}"
        ))
        .header("Zotero-API-Version", "3");
    if let Some(server) = expected_server {
        bounded(&server, 200)?;
        request = request.header("Zotero-Server-ID", server);
    }
    let mut response=request.send().await.map_err(|_|WorkbenchError::invalid("Zotero local API is unavailable. Open Zotero and enable local application communication, or import BibTeX."))?;
    if !response.status().is_success() {
        return Err(WorkbenchError::invalid(format!(
            "Zotero returned {}. No library changes were requested.",
            response.status()
        )));
    }
    let server_id = response
        .headers()
        .get("Zotero-Server-ID")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if response
        .headers()
        .get("Zotero-API-Version")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v != "3")
    {
        return Err(WorkbenchError::invalid(
            "Zotero API version is not supported",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(err)? {
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
            return Err(WorkbenchError::invalid("Zotero preview exceeded 4 MiB"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let items: Vec<Value> = serde_json::from_slice(&bytes).map_err(err)?;
    if items.len() > 100 {
        return Err(WorkbenchError::invalid(
            "Zotero ignored the requested result limit",
        ));
    }
    Ok(ZoteroPreview {
        server_id,
        collection: collection.unwrap_or_default(),
        start,
        has_more: items.len() == 100,
        items,
    })
}
pub fn import_zotero(
    store: &Store,
    ws: &str,
    preview: ZoteroPreview,
    selected: &[String],
) -> WorkbenchResult<Value> {
    scope(store, ws)?;
    let _guard = lock(store, ws)?;
    if preview.collection.is_empty()
        || preview.items.len() > 100
        || serde_json::to_vec(&preview).map_err(err)?.len() > 4 * 1024 * 1024
    {
        return Err(WorkbenchError::invalid(
            "Select a bounded collection preview",
        ));
    }
    let mut result = Vec::new();
    for item in preview.items.iter().filter(|i| {
        i["key"]
            .as_str()
            .is_some_and(|k| selected.iter().any(|s| s == k))
    }) {
        let key = item["key"]
            .as_str()
            .ok_or_else(|| WorkbenchError::invalid("Zotero item lacks a key"))?;
        let version = item["version"]
            .as_u64()
            .ok_or_else(|| WorkbenchError::invalid("Zotero item lacks a version"))?;
        let object_id = format!(
            "zotero_{}",
            hash(
                format!(
                    "{}\0{}\0{key}\0{version}\0{}",
                    preview.server_id.as_deref().unwrap_or("legacy-local"),
                    item["library"],
                    hash(&serde_json::to_vec(item).map_err(err)?)
                )
                .as_bytes()
            )
        );
        if let Ok(old) = record(store, ws, &object_id, "bibliography") {
            result.push(old);
            continue;
        }
        let source = research::import_source(
            store,
            research::ImportSourceRequest {
                workspace_id: ws.into(),
                title: item["data"]["title"].as_str().unwrap_or(key).into(),
                citation_key: None,
                identifiers: json!({"zoteroKey":key,"zoteroVersion":version,"serverId":preview.server_id,"library":item["library"],"doi":item["data"]["DOI"]}),
                version_label: Some(format!("Zotero item version {version}")),
                path: None,
                locator: None,
                access_state: "metadata".into(),
                acquired_via: "manual".into(),
                operation_id: id("zotero_source")?,
            },
        )?;
        result.push(put(store,ws,&object_id,"bibliography",0,&json!({"origin":"zotero_local_preview","item":item,"serverId":preview.server_id,"collection":preview.collection,"source":source.source,"access":"metadata","attachmentAcquisition":"not_requested"}))?);
    }
    Ok(json!(result))
}

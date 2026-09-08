//! Disposable, local FTS5 projection. Exact-reference reads use authoritative records.
use super::{
    desk::{err, ResearchObjectRef},
    store::{Store, WorkbenchError, WorkbenchResult},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
const BATCH: usize = 12;
const CHUNK: usize = 8192;
const SOURCE_BATCH: usize = 256 * 1024;
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchObject {
    pub object: ResearchObjectRef,
    pub title: String,
    pub text: String,
    pub provenance: String,
    pub access: String,
    pub completeness: String,
    pub truncated: bool,
}
struct Source {
    key: String,
    object: ResearchObjectRef,
    title: String,
    body: String,
    path: Option<String>,
    provenance: String,
    access: String,
    completeness: String,
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Source> {
    Ok(Source {
        key: r.get(0)?,
        object: ResearchObjectRef {
            kind: r.get(1)?,
            id: r.get(2)?,
            revision: r.get(3)?,
            start: None,
            end: None,
        },
        title: r.get(4)?,
        body: r.get(5)?,
        path: r.get(6)?,
        provenance: r.get(7)?,
        access: r.get(8)?,
        completeness: r.get(9)?,
    })
}
const COLS: &str =
    "object_key,kind,id,revision,title,body,text_reference,provenance,access,completeness";
/// UTF-8 byte locators remain valid even around multibyte equations and characters.
pub(crate) fn prefix(text: &str, limit: usize) -> &str {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
fn source_text(
    store: &Store,
    source: &Source,
    start: usize,
    limit: usize,
) -> WorkbenchResult<(String, bool)> {
    if let Some(path) = &source.path {
        let path = Path::new(path);
        let canonical = path.canonicalize().map_err(err)?;
        if !canonical.starts_with(
            store
                .root_path()
                .join("blobs")
                .canonicalize()
                .map_err(err)?,
        ) {
            return Err(WorkbenchError::invalid(
                "Source escaped private blob storage",
            ));
        }
        let mut file = File::open(canonical).map_err(err)?;
        let total = file.metadata().map_err(err)?.len() as usize;
        file.seek(SeekFrom::Start(start as u64)).map_err(err)?;
        let mut bytes = Vec::new();
        file.take(limit as u64 + 4)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        let valid = match std::str::from_utf8(&bytes) {
            Ok(s) => s,
            Err(e) => {
                if e.error_len().is_some() {
                    return Err(WorkbenchError::invalid(
                        "Source is not valid UTF-8 at this locator",
                    ));
                }
                std::str::from_utf8(&bytes[..e.valid_up_to()]).map_err(err)?
            }
        };
        let text = prefix(valid, limit).to_string();
        let more = start + text.len() < total;
        return Ok((text, more));
    }
    if !source.body.is_char_boundary(start.min(source.body.len())) {
        return Err(WorkbenchError::invalid("Locator is not a UTF-8 boundary"));
    }
    let text = prefix(&source.body[start.min(source.body.len())..], limit).to_string();
    let more = start + text.len() < source.body.len();
    Ok((text, more))
}
fn permitted(store: &Store, ws: &str, source: &Source) -> WorkbenchResult<bool> {
    if source.object.kind == "note"
        && super::project::excluded_notes(store, ws)?.contains(&source.object.id)
    {
        return Ok(false);
    }
    if source.object.kind == "source" {
        let excluded:bool=store.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM desk_records a WHERE workspace_id=?1 AND kind='acquisition' AND json_extract(body_json,'$.sourceVersionId')=?2 AND json_extract(body_json,'$.state')='excluded' AND NOT EXISTS(SELECT 1 FROM desk_records n WHERE n.supersedes=a.id))",params![ws,source.object.id],|r|r.get(0)).map_err(err)?;
        if excluded {
            return Ok(false);
        }
    }
    if source.object.kind == "dataset" && !super::data::policy(store, ws)?.dictionary {
        return Ok(false);
    }
    Ok(true)
}
pub fn read_object(
    store: &Store,
    ws: &str,
    object: &ResearchObjectRef,
    limit: usize,
) -> WorkbenchResult<ResearchObject> {
    store.workspace(ws)?;
    for field in [&object.kind, &object.id, &object.revision] {
        super::desk::check_text(field, 500)?;
    }
    if object
        .end
        .is_some_and(|end| end <= object.start.unwrap_or(0))
    {
        return Err(WorkbenchError::invalid("Passage end must follow its start"));
    }
    let c = store.connection()?;
    let mut source=c.query_row(&format!("SELECT {COLS} FROM research_search_sources WHERE workspace_id=?1 AND kind=?2 AND id=?3 AND revision=?4 LIMIT 1"),params![ws,object.kind,object.id,object.revision],row).optional().map_err(err)?.ok_or_else(||WorkbenchError::invalid("Exact research object is unavailable in this project; it was not replaced with a newer revision"))?;
    if !permitted(store, ws, &source)? {
        return Err(WorkbenchError::invalid(
            "This object is excluded from research context and search",
        ));
    }
    if source.object.kind == "dataset" {
        source.body = super::data::dictionary_text(&source.body, &super::data::policy(store, ws)?)?;
    }
    let start = object.start.unwrap_or(0);
    let limit = object
        .end
        .map_or(limit, |e| limit.min(e.saturating_sub(start)))
        .clamp(1, 64 * 1024);
    let (text, truncated) = source_text(store, &source, start, limit)?;
    let mut object = object.clone();
    object.start = Some(start);
    object.end = Some(start + text.len());
    Ok(ResearchObject {
        object,
        title: source.title,
        text,
        provenance: source.provenance,
        access: source.access,
        completeness: source.completeness,
        truncated,
    })
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub generation: i64,
    pub complete: bool,
    pub cursor: String,
    pub byte_offset: usize,
}
pub fn rebuild(store: &Store, ws: &str) -> WorkbenchResult<IndexStatus> {
    store.workspace(ws)?;
    store.connection()?.execute("INSERT INTO research_index_state(workspace_id) VALUES(?1) ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0",[ws]).map_err(err)?;
    status(store, ws)
}
pub fn status(store: &Store, ws: &str) -> WorkbenchResult<IndexStatus> {
    store.workspace(ws)?;
    let c = store.connection()?;
    c.execute(
        "INSERT OR IGNORE INTO research_index_state(workspace_id) VALUES(?1)",
        [ws],
    )
    .map_err(err)?;
    c.query_row("SELECT generation,complete,cursor,byte_offset FROM research_index_state WHERE workspace_id=?1",[ws],|r|Ok(IndexStatus{generation:r.get(0)?,complete:r.get(1)?,cursor:r.get(2)?,byte_offset:r.get::<_,i64>(3)? as usize})).map_err(err)
}
/// Each call advances at most twelve sources / 256 KiB per source. Mutations
/// invalidate the generation atomically; mixed-generation results are never shown.
pub fn advance(store: &Store, ws: &str) -> WorkbenchResult<IndexStatus> {
    let state = status(store, ws)?;
    if state.complete {
        return Ok(state);
    }
    let c = store.connection()?;
    let mut q=c.prepare(&format!("SELECT {COLS} FROM research_search_sources WHERE workspace_id=?1 AND (object_key>?2 OR (object_key=?2 AND ?3>0)) ORDER BY object_key LIMIT ?4")).map_err(err)?;
    let sources = q
        .query_map(
            params![ws, state.cursor, state.byte_offset as i64, BATCH as i64],
            row,
        )
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let mut cursor = state.cursor.clone();
    let mut offset = 0;
    let mut entries = Vec::new();
    let count = sources.len();
    for mut source in sources {
        let start = if source.key == state.cursor {
            state.byte_offset
        } else {
            0
        };
        cursor = source.key.clone();
        if !permitted(store, ws, &source)? {
            continue;
        }
        if source.object.kind == "dataset" {
            source.body =
                super::data::dictionary_text(&source.body, &super::data::policy(store, ws)?)?;
        }
        let (text, more) = match source_text(store, &source, start, SOURCE_BATCH) {
            Ok(v) => v,
            Err(_) => {
                entries.push((
                    source.object.clone(),
                    source.title.clone(),
                    String::from("Captured text unavailable; inspect original source"),
                    source.provenance.clone(),
                    source.access.clone(),
                    String::from("unavailable"),
                    source.body.clone(),
                ));
                continue;
            }
        };
        let mut position = 0;
        // Metadata-only works still receive a searchable title.
        loop {
            let chunk = prefix(&text[position..], CHUNK);
            let mut object = source.object.clone();
            object.start = Some(start + position);
            object.end = Some(start + position + chunk.len());
            entries.push((
                object,
                source.title.clone(),
                chunk.to_string(),
                source.provenance.clone(),
                source.access.clone(),
                source.completeness.clone(),
                if source.object.kind == "source" {
                    source.body.clone()
                } else {
                    String::new()
                },
            ));
            position += chunk.len();
            if position >= text.len() {
                break;
            }
        }
        if more {
            offset = start + text.len();
            break;
        }
    }
    drop(q);
    drop(c);
    let mut c = store.connection()?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(err)?;
    let current: (i64, String, i64) = tx
        .query_row(
            "SELECT generation,cursor,byte_offset FROM research_index_state WHERE workspace_id=?1",
            [ws],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(err)?;
    if current
        != (
            state.generation,
            state.cursor.clone(),
            state.byte_offset as i64,
        )
    {
        drop(tx);
        return status(store, ws);
    }
    tx.execute(
        "DELETE FROM research_fts WHERE workspace_id=?1 AND generation<>?2",
        params![ws, state.generation],
    )
    .map_err(err)?;
    for (object, title, text, provenance, access, completeness, metadata) in entries {
        tx.execute("INSERT INTO research_fts(title,body,workspace_id,generation,object_json,provenance,access,completeness,metadata) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![title,text,ws,state.generation,serde_json::to_string(&object).map_err(err)?,provenance,access,completeness,metadata]).map_err(err)?;
    }
    tx.execute("UPDATE research_index_state SET cursor=?2,byte_offset=?3,complete=?4 WHERE workspace_id=?1",params![ws,cursor,offset as i64,count<BATCH && offset==0]).map_err(err)?;
    tx.commit().map_err(err)?;
    status(store, ws)
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub workspace_id: String,
    pub query: String,
    pub kind: Option<String>,
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub hits: Vec<ResearchObject>,
    pub next_cursor: Option<String>,
    pub index: IndexStatus,
    pub version: u32,
}
pub fn search(store: &Store, r: SearchRequest) -> WorkbenchResult<SearchPage> {
    super::desk::check_text(&r.query, 1000)?;
    let state = status(store, &r.workspace_id)?;
    let identity = super::desk::hash(format!("{}:{:?}", r.query, r.kind).as_bytes());
    let mut after = 0i64;
    if let Some(cursor) = r.cursor {
        let parts = cursor.split(':').collect::<Vec<_>>();
        if parts.len() != 3 || parts[0] != state.generation.to_string() || parts[1] != identity {
            return Err(WorkbenchError::conflict(
                "Search index or query changed; start a new search",
            ));
        }
        after = parts[2].parse().map_err(err)?;
    }
    let tokens = r
        .query
        .split_whitespace()
        .take(32)
        .map(|s| {
            let exact = format!("\"{}\"", s.replace('"', "\"\""));
            // Small, local academic abbreviations; no model or embedding call.
            let synonym = match s.to_ascii_lowercase().as_str() {
                "irf" | "irfs" => Some("impulse response"),
                "stderr" => Some("standard error"),
                "tfp" => Some("total factor productivity"),
                "iv" => Some("instrumental variable"),
                _ => None,
            };
            synonym.map_or(exact.clone(), |phrase| format!("({exact} OR \"{phrase}\")"))
        })
        .collect::<Vec<_>>();
    let query = tokens.join(" AND ");
    let limit = r.limit.unwrap_or(20).clamp(1, 50);
    let c = store.connection()?;
    let mut q=c.prepare("SELECT rowid,object_json,title,snippet(research_fts,1,'','', ' … ',40),provenance,access,completeness FROM research_fts WHERE research_fts MATCH ?1 AND workspace_id=?2 AND generation=?3 AND rowid>?4 AND (?5 IS NULL OR json_extract(object_json,'$.kind')=?5) ORDER BY rowid LIMIT ?6").map_err(err)?;
    let rows = q
        .query_map(
            params![
                query,
                r.workspace_id,
                state.generation,
                after,
                r.kind,
                (limit + 1) as i64
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    ResearchObject {
                        object: serde_json::from_str(&row.get::<_, String>(1)?).unwrap(),
                        title: row.get(2)?,
                        text: row.get(3)?,
                        provenance: row.get(4)?,
                        access: row.get(5)?,
                        completeness: row.get(6)?,
                        truncated: true,
                    },
                ))
            },
        )
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let next_cursor = if rows.len() > limit {
        Some(format!(
            "{}:{}:{}",
            state.generation,
            identity,
            rows[limit - 1].0
        ))
    } else {
        None
    };
    Ok(SearchPage {
        hits: rows.into_iter().take(limit).map(|(_, v)| v).collect(),
        next_cursor,
        index: state,
        version: 1,
    })
}

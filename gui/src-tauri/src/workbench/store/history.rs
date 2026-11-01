//! Bounded keyset pages; history size never prevents metadata or newest-page access.
use super::*;
const PAGE_ITEMS: usize = 200;
const PAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptCursor {
    pub created_at: String,
    pub id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptPage {
    pub items: Vec<TranscriptItem>,
    pub next_cursor: Option<TranscriptCursor>,
}

impl Store {
    pub fn transcript_page(
        &self,
        session_id: &str,
        before: Option<&TranscriptCursor>,
    ) -> WorkbenchResult<TranscriptPage> {
        validate_id("session id", session_id)?;
        let connection = self.connection()?;
        load_session(&connection, session_id)?;
        read_page(&connection, session_id, before, false)
    }
    pub(crate) fn export_transcript(
        &self,
        session_id: &str,
        mut visit: impl FnMut(TranscriptItem) -> WorkbenchResult<()>,
    ) -> WorkbenchResult<()> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction()
            .map_err(|e| WorkbenchError::storage("Failed to read transcript", e))?;
        load_session(&tx, session_id)?;
        let mut cursor = None;
        loop {
            let page = read_page(&tx, session_id, cursor.as_ref(), true)?;
            for item in page.items {
                visit(item)?;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(());
            }
        }
    }

    /// Hands each final message to `visit`, newest first, until it returns
    /// false. Tool and usage rows only move the cursor, so a long run of them
    /// neither hides older messages nor costs their payloads.
    pub(crate) fn visit_messages_newest_first(
        &self,
        session_id: &str,
        mut visit: impl FnMut(&str, &str) -> bool,
    ) -> WorkbenchResult<()> {
        validate_id("session id", session_id)?;
        let error = |e| WorkbenchError::storage("Failed to read conversation history", e);
        let mut connection = self.connection()?;
        let tx = connection.transaction().map_err(error)?;
        load_session(&tx, session_id)?;
        let mut statement = tx.prepare(&message_page_query()).map_err(error)?;
        let mut cursor: Option<TranscriptCursor> = None;
        loop {
            let mut rows = statement
                .query(params![
                    session_id,
                    cursor.as_ref().map(|c| &c.created_at),
                    cursor.as_ref().map(|c| &c.id),
                    PAGE_ITEMS as i64
                ])
                .map_err(error)?;
            let mut read = 0;
            while let Some(row) = rows.next().map_err(error)? {
                read += 1;
                cursor = Some(TranscriptCursor {
                    id: row.get(0).map_err(error)?,
                    created_at: row.get(1).map_err(error)?,
                });
                let kind = row.get_ref(2).map_err(error)?.as_str();
                let payload = row.get_ref(3).map_err(error)?.as_str_or_null();
                let (kind, payload) = (
                    kind.map_err(|e| WorkbenchError::invalid(e.to_string()))?,
                    payload.map_err(|e| WorkbenchError::invalid(e.to_string()))?,
                );
                if payload.is_some_and(|payload| !visit(kind, payload)) {
                    return Ok(());
                }
            }
            if read < PAGE_ITEMS {
                return Ok(());
            }
        }
    }
}

/// One page of a transcript. The page is chosen from the covering index alone
/// and payloads are joined afterwards, so a long conversation costs one page of
/// payloads rather than all of them.
fn page_query(comparison: &str, direction: &str) -> String {
    select_page("i.id,i.turn_id,i.provider_item_id,i.item_kind,i.payload_json,i.is_final,i.created_at,i.updated_at", comparison, direction)
}

/// A page of every item kind, so its last row can carry the cursor. The index
/// has no kind or final flag, and only final messages read their payloads.
fn message_page_query() -> String {
    select_page(
        "i.id,i.created_at,i.item_kind,CASE WHEN lower(i.item_kind) LIKE '%message%' AND i.is_final=1 THEN i.payload_json END",
        "<",
        "DESC",
    )
}

fn select_page(columns: &str, comparison: &str, direction: &str) -> String {
    format!("WITH page(item) AS (SELECT i.rowid FROM transcript_items i JOIN session_bindings b ON b.id=i.binding_id WHERE b.session_id=?1 AND (?2 IS NULL OR (i.created_at,i.id) {comparison} (?2,?3)) AND NOT EXISTS (SELECT 1 FROM transcript_items newer JOIN session_bindings nb ON nb.id=newer.binding_id WHERE nb.session_id=b.session_id AND newer.provider_item_id=i.provider_item_id AND nb.incarnation>b.incarnation) ORDER BY i.created_at {direction}, i.id {direction} LIMIT ?4) SELECT {columns} FROM page JOIN transcript_items i ON i.rowid=page.item ORDER BY i.created_at {direction}, i.id {direction}")
}

pub(super) fn read_page(
    connection: &Connection,
    session: &str,
    cursor: Option<&TranscriptCursor>,
    ascending: bool,
) -> WorkbenchResult<TranscriptPage> {
    if let Some(cursor) = cursor {
        validate_id("transcript cursor", &cursor.id)?;
        if cursor.created_at.len() > 128 {
            return Err(WorkbenchError::invalid("Invalid transcript cursor"));
        }
    }
    let (comparison, direction) = if ascending {
        (">", "ASC")
    } else {
        ("<", "DESC")
    };
    let sql = page_query(comparison, direction);
    let error = |e| WorkbenchError::storage("Failed to read transcript page", e);
    let mut statement = connection.prepare(&sql).map_err(error)?;
    let mut rows = statement
        .query(params![
            session,
            cursor.map(|c| &c.created_at),
            cursor.map(|c| &c.id),
            (PAGE_ITEMS + 1) as i64
        ])
        .map_err(error)?;
    let mut items: Vec<TranscriptItem> = Vec::new();
    let mut bytes = 0;
    let mut more = false;
    while let Some(row) = rows.next().map_err(error)? {
        let payload = row
            .get_ref(4)
            .map_err(error)?
            .as_str()
            .map_err(|e| WorkbenchError::invalid(e.to_string()))?;
        if payload.len() > PAGE_BYTES {
            return Err(WorkbenchError::invalid(
                "A transcript item exceeds the 8 MiB read limit",
            ));
        }
        if items.len() == PAGE_ITEMS || bytes + payload.len() > PAGE_BYTES {
            more = true;
            break;
        }
        bytes += payload.len();
        items.push(TranscriptItem {
            id: row.get(0).map_err(error)?,
            turn_id: row.get(1).map_err(error)?,
            provider_item_id: row.get(2).map_err(error)?,
            item_kind: row.get(3).map_err(error)?,
            payload: serde_json::from_str(payload).unwrap_or(Value::Null),
            is_final: row.get::<_, i64>(5).map_err(error)? != 0,
            created_at: row.get(6).map_err(error)?,
            updated_at: row.get(7).map_err(error)?,
        });
    }
    let next_cursor = more.then(|| {
        let last = items.last().expect("nonempty bounded page");
        TranscriptCursor {
            created_at: last.created_at.clone(),
            id: last.id.clone(),
        }
    });
    if !ascending {
        items.reverse();
    }
    Ok(TranscriptPage { items, next_cursor })
}

#[cfg(test)]
mod tests;

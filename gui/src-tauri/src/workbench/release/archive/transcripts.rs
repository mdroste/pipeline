//! Readable transcript projection for portable research archives.

use super::format::{MAX_ARCHIVE_BYTES, MAX_ENTRY_BYTES};
use crate::workbench::store::{WorkbenchError, WorkbenchResult};
use rusqlite::{types::Type, Connection};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn transcript_text(payload: &Value) -> Option<String> {
    for key in ["text", "message", "content"] {
        match payload.get(key) {
            Some(Value::String(text)) if !text.is_empty() => return Some(text.clone()),
            Some(Value::Array(parts)) => {
                let text = parts
                    .iter()
                    .filter_map(|part| {
                        part.as_str()
                            .or_else(|| part.get("text").and_then(Value::as_str))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    return Some(text);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn transcript_items(
    connection: &Connection,
    session_id: &str,
) -> WorkbenchResult<Vec<Value>> {
    let mut statement = connection.prepare(
        "SELECT i.id,i.turn_id,i.provider_item_id,i.item_kind,i.payload_json,i.is_final,i.created_at,i.updated_at \
         FROM transcript_items i JOIN session_bindings b ON b.id=i.binding_id \
         WHERE b.session_id=?1 AND NOT EXISTS (\
           SELECT 1 FROM transcript_items newer JOIN session_bindings nb ON nb.id=newer.binding_id \
           WHERE nb.session_id=b.session_id AND newer.provider_item_id=i.provider_item_id AND nb.incarnation>b.incarnation\
         ) ORDER BY i.created_at,i.id",
    ).map_err(|error| WorkbenchError::storage("Failed to prepare transcript export", error))?;
    let items = statement
        .query_map([session_id], |row| {
            let payload: String = row.get(4)?;
            let payload = serde_json::from_str::<Value>(&payload).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(4, Type::Text, Box::new(error))
            })?;
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "turnId": row.get::<_, Option<String>>(1)?,
                "providerItemId": row.get::<_, String>(2)?,
                "itemKind": row.get::<_, String>(3)?,
                "payload": payload,
                "isFinal": row.get::<_, bool>(5)?,
                "createdAt": row.get::<_, String>(6)?,
                "updatedAt": row.get::<_, String>(7)?,
            }))
        })
        .map_err(|error| WorkbenchError::storage("Failed to read transcript", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode transcript", error))?;
    Ok(items)
}

/// A transcript heading. The record doesn't say which system wrote a reply.
pub(crate) fn transcript_role(item_kind: &str) -> &'static str {
    if item_kind.to_ascii_lowercase().contains("user") {
        "You"
    } else {
        "Assistant"
    }
}

pub(super) fn transcript_markdown(title: &str, items: &[Value]) -> String {
    let mut body = format!("# {title}\n\n");
    for item in items {
        let kind = item.get("itemKind").and_then(Value::as_str).unwrap_or("");
        if !kind.to_ascii_lowercase().contains("message") {
            continue;
        }
        let text = item.get("payload").and_then(transcript_text);
        if let Some(text) = text {
            let role = transcript_role(kind);
            body.push_str(&format!("## {role}\n\n{text}\n\n"));
        }
    }
    body
}

/// One conversation's readable transcript, rendered into staging.
pub(super) struct StagedTranscript {
    pub(super) id: String,
    pub(super) markdown: PathBuf,
    pub(super) json: PathBuf,
}

/// Renders every transcript before the archive is written, so the space they
/// need is known exactly before anything else claims it.
pub(super) fn stage_transcripts(
    snapshot: &Connection,
    staging: &Path,
) -> WorkbenchResult<(Vec<StagedTranscript>, u64)> {
    let folder = staging.join("rendered-transcripts");
    fs::create_dir(&folder)
        .map_err(|error| WorkbenchError::storage("Failed to stage transcripts", error))?;
    let mut statement = snapshot
        .prepare("SELECT id,title FROM sessions ORDER BY id")
        .map_err(|error| WorkbenchError::storage("Failed to prepare transcript archive", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| WorkbenchError::storage("Failed to list transcripts", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode transcripts", error))?;
    let mut staged = Vec::with_capacity(rows.len());
    let mut bytes = 0u64;
    for (index, (id, title)) in rows.into_iter().enumerate() {
        let items = transcript_items(snapshot, &id)?;
        let md = transcript_markdown(&title, &items);
        let payload = json!({
            "schemaVersion": 1,
            "session": {"id": id, "title": title},
            "items": items,
        });
        let json = serde_json::to_string_pretty(&payload).map_err(|error| {
            WorkbenchError::invalid(format!("Failed to encode transcript: {error}"))
        })?;
        if md.len() as u64 > MAX_ENTRY_BYTES || json.len() as u64 > MAX_ENTRY_BYTES {
            return Err(WorkbenchError::invalid(
                "A readable transcript exceeds the 512 MiB archive-entry limit",
            ));
        }
        bytes = bytes
            .saturating_add(md.len() as u64)
            .saturating_add(json.len() as u64);
        if bytes > MAX_ARCHIVE_BYTES {
            return Err(WorkbenchError::invalid(
                "Expanded research archive exceeds 2 GiB",
            ));
        }
        let markdown = folder.join(format!("{index}.md"));
        let encoded = folder.join(format!("{index}.json"));
        fs::write(&markdown, md)
            .and_then(|()| fs::write(&encoded, json))
            .map_err(|error| WorkbenchError::storage("Failed to stage a transcript", error))?;
        staged.push(StagedTranscript {
            id,
            markdown,
            json: encoded,
        });
    }
    Ok((staged, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_is_labelled_by_role_and_never_as_one_provider() {
        let items = [
            json!({"itemKind":"userMessage","payload":{"text":"Question"}}),
            json!({"itemKind":"agentMessage","payload":{"text":"Answer"}}),
        ];
        let markdown = transcript_markdown("Paper", &items);
        assert!(markdown.contains("## You\n\nQuestion"), "{markdown}");
        assert!(markdown.contains("## Assistant\n\nAnswer"), "{markdown}");
        assert!(!markdown.contains("ChatGPT"));
    }
}

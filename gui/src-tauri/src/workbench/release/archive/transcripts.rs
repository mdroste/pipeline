//! Readable transcript projection for portable research archives.

use crate::workbench::store::{WorkbenchError, WorkbenchResult};
use rusqlite::{types::Type, Connection};
use serde_json::{json, Value};

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

pub(super) fn transcript_markdown(title: &str, items: &[Value]) -> String {
    let mut body = format!("# {title}\n\n");
    for item in items {
        let kind = item.get("itemKind").and_then(Value::as_str).unwrap_or("");
        if !kind.to_ascii_lowercase().contains("message") {
            continue;
        }
        let text = item.get("payload").and_then(transcript_text);
        if let Some(text) = text {
            let role = if kind.to_ascii_lowercase().contains("user") {
                "You"
            } else {
                "ChatGPT"
            };
            body.push_str(&format!("## {role}\n\n{text}\n\n"));
        }
    }
    body
}

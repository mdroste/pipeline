//! Bounded native event projection into durable turns and transcript items.

use super::{
    map_constraint, new_id, validate_provider_id, WorkbenchError, WorkbenchResult,
    MAX_PROJECTED_PAYLOAD_BYTES,
};
use rusqlite::{params, Connection, OptionalExtension as _};
use serde_json::Value;

pub(super) fn required_json_string<'a>(
    value: &'a Value,
    key: &str,
    context: &str,
) -> WorkbenchResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| WorkbenchError::invalid(format!("{context} omitted string field {key}")))
}

fn encode_projected_payload(value: &Value, context: &str) -> WorkbenchResult<String> {
    let encoded =
        serde_json::to_string(value).map_err(|error| WorkbenchError::storage(context, error))?;
    if encoded.len() > MAX_PROJECTED_PAYLOAD_BYTES {
        return Err(WorkbenchError::invalid(format!(
            "{context} exceeded {MAX_PROJECTED_PAYLOAD_BYTES} bytes"
        )));
    }
    Ok(encoded)
}

fn ensure_projected_turn(
    connection: &Connection,
    binding_id: &str,
    provider_turn_id: &str,
    timestamp: &str,
) -> WorkbenchResult<String> {
    if let Some(id) = connection
        .query_row(
            "SELECT id FROM turns WHERE binding_id = ?1 AND provider_turn_id = ?2",
            params![binding_id, provider_turn_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to inspect projected turn", error))?
    {
        return Ok(id);
    }
    let id = new_id("turn")?;
    let provisional_submission_id = format!("provider:{provider_turn_id}");
    connection
        .execute(
            "INSERT INTO turns (id, binding_id, client_submission_id, provider_turn_id, state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'inProgress', ?5, ?5)",
            params![id, binding_id, provisional_submission_id, provider_turn_id, timestamp],
        )
        .map_err(|error| map_constraint("Failed to create projected turn", error))?;
    Ok(id)
}

pub(super) fn upsert_projected_turn(
    connection: &Connection,
    binding_id: &str,
    provider_turn_id: &str,
    turn: &Value,
    timestamp: &str,
    terminal: bool,
) -> WorkbenchResult<()> {
    validate_provider_id("provider turn id", provider_turn_id)?;
    encode_projected_payload(turn, "Projected turn")?;
    let id = ensure_projected_turn(connection, binding_id, provider_turn_id, timestamp)?;
    let state = required_json_string(turn, "status", "projected turn")?;
    let usage = turn
        .get("usage")
        .filter(|value| !value.is_null())
        .map(|value| encode_projected_payload(value, "Projected turn usage"))
        .transpose()?;
    let error = turn
        .get("error")
        .filter(|value| !value.is_null())
        .map(|value| encode_projected_payload(value, "Projected turn error"))
        .transpose()?;
    connection
        .execute(
            "UPDATE turns SET state = ?1, usage_json = ?2, error_json = ?3, updated_at = ?4, terminal_at = CASE WHEN ?5 THEN ?4 ELSE terminal_at END WHERE id = ?6",
            params![state, usage, error, timestamp, terminal, id],
        )
        .map_err(|error| WorkbenchError::storage("Failed to update projected turn", error))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn upsert_projected_item(
    connection: &Connection,
    binding_id: &str,
    provider_turn_id: &str,
    provider_item_id: &str,
    item_kind: &str,
    item: &Value,
    is_final: bool,
    timestamp: &str,
) -> WorkbenchResult<()> {
    validate_provider_id("provider item id", provider_item_id)?;
    let payload = encode_projected_payload(item, "Projected transcript item")?;
    let turn_id = ensure_projected_turn(connection, binding_id, provider_turn_id, timestamp)?;
    let id = new_id("item")?;
    connection
        .execute(
            "INSERT INTO transcript_items (id, binding_id, turn_id, provider_item_id, item_kind, payload_json, is_final, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8) ON CONFLICT(binding_id, provider_item_id) DO UPDATE SET turn_id = excluded.turn_id, item_kind = excluded.item_kind, payload_json = excluded.payload_json, is_final = MAX(transcript_items.is_final, excluded.is_final), updated_at = excluded.updated_at",
            params![id, binding_id, turn_id, provider_item_id, item_kind, payload, is_final, timestamp],
        )
        .map_err(|error| WorkbenchError::storage("Failed to project transcript item", error))?;
    Ok(())
}

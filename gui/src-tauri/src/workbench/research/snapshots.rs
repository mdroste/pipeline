//! Durable per-turn configuration and context snapshots.

use super::harness::resolve_harness;
use super::{hash_bytes, new_id, now, open_connection, PreparedHarness, HARNESS_SCHEMA_VERSION};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, TransactionBehavior};
use serde_json::json;
use std::fs;

pub fn prepare_turn(store: &Store, session_id: &str) -> WorkbenchResult<PreparedHarness> {
    let effective = resolve_harness(store, session_id)?;
    let config_id = new_id("config")?;
    let context_id = new_id("context")?;
    let timestamp = now();
    let config_body = serde_json::to_string(&effective)
        .map_err(|error| WorkbenchError::storage("Failed to encode effective harness", error))?;
    let context_manifest = serde_json::to_string(&json!({"workspaceId":effective.workspace_id,"presetId":effective.preset.id,"fingerprint":effective.fingerprint,"truncated":effective.context_truncated,"bytes":effective.context_preview.len(),"selection":crate::workbench::desk::context(store,session_id)?})).map_err(|error| WorkbenchError::storage("Failed to encode context manifest", error))?;
    let body_reference = if effective.context_preview.is_empty() {
        None
    } else {
        let hash = hash_bytes(effective.context_preview.as_bytes());
        let path = store
            .root_path()
            .join("context")
            .join(format!("{hash}.txt"));
        if !path.exists() {
            fs::write(&path, effective.context_preview.as_bytes()).map_err(|error| {
                WorkbenchError::storage("Failed to persist context snapshot", error)
            })?;
        }
        Some(path.to_string_lossy().into_owned())
    };
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start turn snapshot", error))?;
    transaction.execute("INSERT INTO config_snapshots (id, session_id, schema_version, body_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)", params![config_id, session_id, HARNESS_SCHEMA_VERSION, config_body, timestamp]).map_err(|error| WorkbenchError::storage("Failed to persist configuration snapshot", error))?;
    transaction.execute("INSERT INTO context_snapshots (id, session_id, schema_version, manifest_json, body_reference, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![context_id, session_id, HARNESS_SCHEMA_VERSION, context_manifest, body_reference, timestamp]).map_err(|error| WorkbenchError::storage("Failed to persist context manifest", error))?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit turn snapshots", error))?;
    Ok(PreparedHarness {
        effective,
        config_snapshot_id: config_id,
        context_snapshot_id: context_id,
    })
}

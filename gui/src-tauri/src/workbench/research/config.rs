//! Workspace configuration inheritance and editable harness presets.

use super::catalog::builtin_presets;
use super::{
    append_change, harness_modules, json_object, new_id, now, open_connection, validate_id,
    validate_text, BasePromptUpdate, ClonePresetRequest, HarnessCatalog, HarnessPreset,
    InstructionSection, SaveWorkspaceConfigRequest, UpdatePresetRequest, WorkspaceConfig,
    HARNESS_SCHEMA_VERSION, MAX_INSTRUCTIONS_BYTES, TOOL_CATALOG_VERSION, WORKSPACE_PREAMBLE,
};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _, TransactionBehavior};
use serde_json::{json, Map, Value};

fn config_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceConfig> {
    let body: String = row.get(3)?;
    Ok(WorkspaceConfig {
        scope_key: row.get(0)?,
        workspace_id: row.get(1)?,
        schema_version: row.get(2)?,
        body: serde_json::from_str(&body).unwrap_or(Value::Null),
        revision: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

pub fn load_config(store: &Store, workspace_id: Option<&str>) -> WorkbenchResult<WorkspaceConfig> {
    let scope = workspace_id.unwrap_or("global");
    if let Some(id) = workspace_id {
        validate_id("workspace id", id)?;
    }
    let connection = open_connection(store)?;
    let found = connection.query_row("SELECT scope_key, workspace_id, schema_version, body_json, revision, updated_at FROM workspace_configs WHERE scope_key = ?1", [scope], config_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to read Workspace configuration", error))?;
    Ok(found.unwrap_or_else(|| WorkspaceConfig {
        scope_key: scope.to_string(),
        workspace_id: workspace_id.map(str::to_string),
        schema_version: HARNESS_SCHEMA_VERSION,
        body: json!({}),
        revision: 0,
        updated_at: now(),
    }))
}

pub fn save_config(
    store: &Store,
    request: SaveWorkspaceConfigRequest,
) -> WorkbenchResult<WorkspaceConfig> {
    let scope = request
        .workspace_id
        .as_deref()
        .unwrap_or("global")
        .to_string();
    if let Some(id) = request.workspace_id.as_deref() {
        validate_id("workspace id", id)?;
    }
    let (_body, encoded) = json_object("Workspace configuration", request.body, 256 * 1024)?;
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start configuration update", error))?;
    let current: Option<i64> = transaction
        .query_row(
            "SELECT revision FROM workspace_configs WHERE scope_key = ?1",
            [&scope],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to read configuration revision", error))?;
    if request.expected_revision.unwrap_or(current.unwrap_or(0)) != current.unwrap_or(0) {
        return Err(WorkbenchError::invalid(
            "Workspace configuration changed; refresh before saving",
        ));
    }
    let revision = current.unwrap_or(0) + 1;
    let timestamp = now();
    transaction.execute("INSERT INTO workspace_configs (scope_key, workspace_id, schema_version, body_json, revision, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(scope_key) DO UPDATE SET body_json = excluded.body_json, schema_version = excluded.schema_version, revision = excluded.revision, updated_at = excluded.updated_at", params![scope, request.workspace_id, HARNESS_SCHEMA_VERSION, encoded, revision, timestamp]).map_err(|error| WorkbenchError::storage("Failed to save Workspace configuration", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "workspace_config",
        &scope,
        (request.workspace_id.as_deref(), None),
        "updated",
        &json!({"revision":revision}),
    )?;
    transaction.commit().map_err(|error| {
        WorkbenchError::storage("Failed to commit Workspace configuration", error)
    })?;
    load_config(store, request.workspace_id.as_deref())
}

fn custom_presets(
    store: &Store,
    workspace_id: Option<&str>,
) -> WorkbenchResult<Vec<HarnessPreset>> {
    let connection = open_connection(store)?;
    let mut statement = connection.prepare("SELECT id, workspace_id, name, description, instructions, modules_json, source_preset_id, revision, base_instructions FROM presets WHERE archived_at IS NULL AND (workspace_id IS NULL OR workspace_id = ?1) ORDER BY name, id").map_err(|error| WorkbenchError::storage("Failed to prepare preset listing", error))?;
    let rows = statement
        .query_map([workspace_id], |row| {
            let modules: String = row.get(5)?;
            Ok(HarnessPreset {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                instructions: row.get(4)?,
                modules: serde_json::from_str(&modules).unwrap_or_default(),
                built_in: false,
                source_preset_id: row.get(6)?,
                revision: row.get(7)?,
                base_instructions: row.get(8)?,
            })
        })
        .map_err(|error| WorkbenchError::storage("Failed to list presets", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode presets", error))
}

pub fn harness_catalog(
    store: &Store,
    workspace_id: Option<&str>,
) -> WorkbenchResult<HarnessCatalog> {
    let mut presets = builtin_presets();
    presets.extend(custom_presets(store, workspace_id)?);
    Ok(HarnessCatalog {
        schema_version: HARNESS_SCHEMA_VERSION,
        tool_catalog_version: TOOL_CATALOG_VERSION,
        modules: harness_modules(),
        presets,
        prompt_layers: vec![InstructionSection {
            id: "preamble".into(),
            label: "Pipeline Workspace instructions".into(),
            text: WORKSPACE_PREAMBLE.into(),
        }],
    })
}

pub(super) fn find_preset(
    store: &Store,
    workspace_id: Option<&str>,
    id: &str,
) -> WorkbenchResult<HarnessPreset> {
    harness_catalog(store, workspace_id)?
        .presets
        .into_iter()
        .find(|preset| {
            preset.id == id
                && (preset.built_in
                    || preset.workspace_id.as_deref() == workspace_id
                    || preset.workspace_id.is_none())
        })
        .ok_or_else(|| WorkbenchError::invalid("Harness preset is unavailable in this workspace"))
}

pub fn clone_preset(store: &Store, request: ClonePresetRequest) -> WorkbenchResult<HarnessPreset> {
    let source = find_preset(
        store,
        request
            .source_workspace_id
            .as_deref()
            .or(request.workspace_id.as_deref()),
        &request.source_preset_id,
    )?;
    let name = validate_text("Preset name", &request.name, 300)?;
    let id = new_id("preset")?;
    let timestamp = now();
    let modules = serde_json::to_string(&source.modules)
        .map_err(|error| WorkbenchError::storage("Failed to encode preset modules", error))?;
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start preset clone", error))?;
    transaction.execute("INSERT INTO presets (id, workspace_id, name, description, instructions, modules_json, source_preset_id, revision, created_at, updated_at, base_instructions) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?8, ?9)", params![id, request.workspace_id, name, source.description, source.instructions, modules, source.id, timestamp, source.base_instructions]).map_err(|error| WorkbenchError::storage("Failed to clone preset", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "preset",
        &id,
        (request.workspace_id.as_deref(), None),
        "created",
        &json!({"sourcePresetId":source.id}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit preset clone", error))?;
    find_preset(store, request.workspace_id.as_deref(), &id)
}

pub fn update_preset(
    store: &Store,
    request: UpdatePresetRequest,
) -> WorkbenchResult<HarnessPreset> {
    validate_id("preset id", &request.preset_id)?;
    let name = validate_text("Preset name", &request.name, 300)?;
    let description = request.description.trim();
    if description.len() > 2_000 || description.contains('\0') {
        return Err(WorkbenchError::invalid(
            "Profile description must be at most 2000 bytes and contain no NUL characters",
        ));
    }
    if request.instructions.len() > MAX_INSTRUCTIONS_BYTES || request.instructions.contains('\0') {
        return Err(WorkbenchError::invalid(
            "Profile instructions must be at most 256 KiB and contain no NUL characters",
        ));
    }
    if let Some(BasePromptUpdate::Replace { text }) = &request.base_prompt {
        if text.trim().is_empty() || text.len() > MAX_INSTRUCTIONS_BYTES || text.contains('\0') {
            return Err(WorkbenchError::invalid(
                "Replacement base prompt must contain 1 to 256 KiB and no NUL characters",
            ));
        }
    }
    let known = harness_modules()
        .into_iter()
        .map(|module| module.id)
        .collect::<std::collections::HashSet<_>>();
    if request.modules.len() > 32
        || request
            .modules
            .iter()
            .any(|id| !known.contains(id.as_str()))
    {
        return Err(WorkbenchError::invalid(
            "Preset contains an unknown or excessive module list",
        ));
    }
    let modules = serde_json::to_string(&request.modules)
        .map_err(|error| WorkbenchError::storage("Failed to encode preset modules", error))?;
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start preset update", error))?;
    let workspace_id: Option<String> = transaction.query_row("SELECT workspace_id FROM presets WHERE id = ?1 AND revision = ?2 AND archived_at IS NULL", params![request.preset_id, request.expected_revision], |row| row.get(0)).optional().map_err(|error| WorkbenchError::storage("Failed to inspect preset", error))?.ok_or_else(|| WorkbenchError::invalid("Editable preset was not found at the expected revision"))?;
    let timestamp = now();
    transaction.execute("UPDATE presets SET name = ?2, description = ?3, instructions = ?4, modules_json = ?5, revision = revision + 1, updated_at = ?6 WHERE id = ?1", params![request.preset_id, name, description, request.instructions, modules, timestamp]).map_err(|error| WorkbenchError::storage("Failed to update preset", error))?;
    if let Some(change) = request.base_prompt {
        let text = match change {
            BasePromptUpdate::CodexDefault => None,
            BasePromptUpdate::Replace { text } => Some(text),
        };
        transaction
            .execute(
                "UPDATE presets SET base_instructions = ?2 WHERE id = ?1",
                params![request.preset_id, text],
            )
            .map_err(|error| WorkbenchError::storage("Failed to update base prompt", error))?;
    }
    append_change(
        &transaction,
        &request.operation_id,
        "preset",
        &request.preset_id,
        (workspace_id.as_deref(), None),
        "updated",
        &json!({"revision":request.expected_revision + 1}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit preset update", error))?;
    find_preset(store, workspace_id.as_deref(), &request.preset_id)
}

pub(super) fn merge_config(
    target: &mut Map<String, Value>,
    source: &Value,
    source_name: &str,
    sources: &mut Map<String, Value>,
) {
    if let Some(object) = source.as_object() {
        for (key, value) in object {
            if !value.is_null() {
                target.insert(key.clone(), value.clone());
                sources.insert(key.clone(), Value::String(source_name.to_string()));
            }
        }
    }
}

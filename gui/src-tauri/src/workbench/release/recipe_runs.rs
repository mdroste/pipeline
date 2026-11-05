//! Recipe input checks, durable runs, and completion assessment.

use super::recipes::{find_recipe, ResearchRecipe};
use super::{decode_recipe_json, new_id, now, validate_id};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InputCheck {
    pub input: String,
    pub available: bool,
    pub detail: String,
}

pub fn check_recipe_inputs(
    store: &Store,
    session_id: &str,
    recipe_id: &str,
) -> WorkbenchResult<Vec<InputCheck>> {
    validate_id("session id", session_id)?;
    let connection = store.connection()?;
    let (workspace_id, paper_id): (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT workspace_id, paper_id FROM sessions WHERE id=?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to inspect recipe session", error))?
        .ok_or_else(|| WorkbenchError::invalid("Conversation was not found"))?;
    let recipe = find_recipe(store, workspace_id.as_deref(), recipe_id)?;
    check_required_inputs(
        &connection,
        workspace_id.as_deref(),
        paper_id.as_deref(),
        &recipe.required_inputs,
    )
}

fn check_required_inputs(
    connection: &rusqlite::Connection,
    workspace_id: Option<&str>,
    paper_id: Option<&str>,
    required_inputs: &[String],
) -> WorkbenchResult<Vec<InputCheck>> {
    required_inputs.iter().map(|input| {
        let (available, detail) = match input.as_str() {
            "paper_revision" => (
                match paper_id {
                    Some(id) => connection.query_row("SELECT EXISTS(SELECT 1 FROM papers WHERE id=?1 AND current_revision_id IS NOT NULL)", [id], |row| row.get::<_, bool>(0)).map_err(|error| WorkbenchError::storage("Failed to check the recipe paper revision", error))?,
                    None => false,
                },
                "Select a paper with an immutable current revision",
            ),
            "tested_execution_profile" => (
                match workspace_id {
                    Some(id) => connection.query_row("SELECT EXISTS(SELECT 1 FROM execution_profiles WHERE workspace_id=?1 AND test_status='passed')", [id], |row| row.get::<_, bool>(0)).map_err(|error| WorkbenchError::storage("Failed to check recipe execution profiles", error))?,
                    None => false,
                },
                "Test at least one execution profile successfully",
            ),
            "primary_source" => (
                match workspace_id {
                    Some(id) => connection.query_row("SELECT EXISTS(SELECT 1 FROM sources s JOIN source_versions v ON v.source_id=s.id WHERE s.workspace_id=?1 AND v.access_state IN ('partial','full'))", [id], |row| row.get::<_, bool>(0)).map_err(|error| WorkbenchError::storage("Failed to check recipe source access", error))?,
                    None => false,
                },
                "Import at least one accessible source; confirm primary-source status in the completion card",
            ),
            _ => (false, "This custom required input has no automatic detector"),
        };
        Ok(InputCheck { input: input.clone(), available, detail: detail.to_string() })
    }).collect()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRecipeRequest {
    pub session_id: String,
    pub recipe_id: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecipeRun {
    pub id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub recipe: ResearchRecipe,
    pub status: String,
    pub artifacts: Vec<Value>,
    pub checks: Vec<Value>,
    pub unresolved_issues: Vec<String>,
    pub missing_evidence: Vec<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

pub(super) fn recipe_run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecipeRun> {
    let recipe: String = row.get(3)?;
    let artifacts: String = row.get(5)?;
    let checks: String = row.get(6)?;
    let unresolved: String = row.get(7)?;
    let missing: String = row.get(8)?;
    Ok(RecipeRun {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        session_id: row.get(2)?,
        recipe: decode_recipe_json(3, &recipe)?,
        status: row.get(4)?,
        artifacts: decode_recipe_json(5, &artifacts)?,
        checks: decode_recipe_json(6, &checks)?,
        unresolved_issues: decode_recipe_json(7, &unresolved)?,
        missing_evidence: decode_recipe_json(8, &missing)?,
        started_at: row.get(9)?,
        completed_at: row.get(10)?,
    })
}

pub fn start_recipe(store: &Store, request: StartRecipeRequest) -> WorkbenchResult<RecipeRun> {
    validate_id("operation id", &request.operation_id)?;
    validate_id("session id", &request.session_id)?;
    validate_id("recipe id", &request.recipe_id)?;
    let connection = store.connection()?;
    if let Some(existing) = connection.query_row("SELECT id, workspace_id, session_id, recipe_snapshot_json, status, artifacts_json, checks_json, unresolved_issues_json, missing_evidence_json, started_at, completed_at FROM recipe_runs WHERE operation_id=?1", [&request.operation_id], recipe_run_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to check recipe idempotency", error))? {
        if existing.session_id != request.session_id || existing.recipe.id != request.recipe_id {
            return Err(WorkbenchError::invalid(
                "Recipe operation id was already used for different inputs",
            ));
        }
        return Ok(existing);
    }
    let workspace_id: String = connection
        .query_row(
            "SELECT workspace_id FROM sessions WHERE id=?1",
            [&request.session_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to resolve recipe session", error))?
        .flatten()
        .ok_or_else(|| WorkbenchError::invalid("Recipes require a conversation in a Workspace"))?;
    let recipe = find_recipe(store, Some(&workspace_id), &request.recipe_id)?;
    let checks = check_recipe_inputs(store, &request.session_id, &request.recipe_id)?;
    let missing = checks
        .iter()
        .filter(|check| !check.available)
        .map(|check| check.detail.clone())
        .collect::<Vec<_>>();
    let id = new_id("recipe_run")?;
    let timestamp = now();
    let recipe_snapshot = serde_json::to_string(&recipe)
        .map_err(|error| WorkbenchError::storage("Failed to encode recipe snapshot", error))?;
    let checks = serde_json::to_string(&checks)
        .map_err(|error| WorkbenchError::storage("Failed to encode recipe checks", error))?;
    let missing = serde_json::to_string(&missing)
        .map_err(|error| WorkbenchError::storage("Failed to encode missing evidence", error))?;
    connection.execute("INSERT INTO recipe_runs (id, operation_id, workspace_id, session_id, recipe_id, recipe_version, recipe_snapshot_json, status, artifacts_json, checks_json, unresolved_issues_json, missing_evidence_json, started_at) VALUES (?1,?2,?3,?4,?5,?6,?7,'active','[]',?8,'[]',?9,?10)", params![id, request.operation_id, workspace_id, request.session_id, recipe.id, recipe.version, recipe_snapshot, checks, missing, timestamp]).map_err(|error| WorkbenchError::storage("Failed to start recipe", error))?;
    get_recipe_run(store, &id)
}

fn get_recipe_run(store: &Store, id: &str) -> WorkbenchResult<RecipeRun> {
    store.connection()?.query_row("SELECT id, workspace_id, session_id, recipe_snapshot_json, status, artifacts_json, checks_json, unresolved_issues_json, missing_evidence_json, started_at, completed_at FROM recipe_runs WHERE id=?1", [id], recipe_run_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to read recipe run", error))?.ok_or_else(|| WorkbenchError::invalid("Recipe run was not found"))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteRecipeRequest {
    pub recipe_run_id: String,
    pub artifacts: Vec<Value>,
    pub checks: Vec<Value>,
    pub unresolved_issues: Vec<String>,
    pub missing_evidence: Vec<String>,
}

pub fn complete_recipe(
    store: &Store,
    request: CompleteRecipeRequest,
) -> WorkbenchResult<RecipeRun> {
    validate_id("recipe run id", &request.recipe_run_id)?;
    let run = get_recipe_run(store, &request.recipe_run_id)?;
    if run.status != "active" {
        return Err(WorkbenchError::invalid(
            "Recipe run is missing or already complete",
        ));
    }
    if request.artifacts.len() > 100
        || request.checks.len() > 100
        || request.unresolved_issues.len() > 200
        || request.missing_evidence.len() > 200
    {
        return Err(WorkbenchError::invalid(
            "Recipe completion contains too many records",
        ));
    }
    let connection = store.connection()?;
    let mut execution_ids = HashSet::with_capacity(request.artifacts.len());
    let mut artifacts = Vec::with_capacity(request.artifacts.len());
    for artifact in &request.artifacts {
        let execution_id = artifact
            .get("executionId")
            .and_then(Value::as_str)
            .ok_or_else(|| WorkbenchError::invalid("Recipe artifacts require an executionId"))?;
        validate_id("execution id", execution_id)?;
        if !execution_ids.insert(execution_id) {
            return Err(WorkbenchError::invalid(
                "Recipe artifacts contain a duplicate execution",
            ));
        }
        let output_manifest: String = connection
            .query_row(
                "SELECT output_manifest_json FROM research_executions WHERE id=?1 AND workspace_id=?2 AND session_id=?3 AND outcome='completed'",
                params![execution_id, run.workspace_id, run.session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| {
                WorkbenchError::storage("Failed to validate recipe artifact", error)
            })?
            .ok_or_else(|| {
                WorkbenchError::invalid(
                    "Recipe artifacts must be completed executions from this conversation",
                )
            })?;
        let output_manifest = serde_json::from_str::<Value>(&output_manifest).map_err(|error| {
            WorkbenchError::invalid(format!("Execution output manifest is invalid: {error}"))
        })?;
        artifacts.push(json!({
            "executionId": execution_id,
            "outputManifest": output_manifest,
        }));
    }

    let expected_checks = run
        .recipe
        .expected_checks
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut seen_checks = HashSet::with_capacity(request.checks.len());
    let mut checks = Vec::with_capacity(request.checks.len());
    let mut missing_evidence = request.missing_evidence;
    for check in &request.checks {
        let name = check
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| WorkbenchError::invalid("Recipe check records require a name"))?;
        let status = check
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(|| WorkbenchError::invalid("Recipe check records require a status"))?;
        if !expected_checks.contains(name)
            || !seen_checks.insert(name)
            || !matches!(status, "recorded" | "not_recorded")
        {
            return Err(WorkbenchError::invalid(
                "Recipe check records must uniquely match the recipe snapshot",
            ));
        }
        if status == "not_recorded" {
            missing_evidence.push(format!("Check not recorded: {}", name.replace('_', " ")));
        }
        checks.push(json!({"name": name, "status": status}));
    }
    if seen_checks != expected_checks {
        return Err(WorkbenchError::invalid(
            "Recipe completion must record every expected check",
        ));
    }
    let paper_id: Option<String> = connection
        .query_row(
            "SELECT paper_id FROM sessions WHERE id=?1 AND workspace_id=?2",
            params![run.session_id, run.workspace_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            WorkbenchError::storage("Failed to validate the recipe conversation", error)
        })?
        .ok_or_else(|| WorkbenchError::invalid("Recipe conversation was not found"))?;
    for input in check_required_inputs(
        &connection,
        Some(&run.workspace_id),
        paper_id.as_deref(),
        &run.recipe.required_inputs,
    )? {
        if !input.available {
            missing_evidence.push(input.detail);
        }
    }
    missing_evidence.sort();
    missing_evidence.dedup();
    let unresolved_issues =
        validate_completion_notes("Recipe unresolved issues", request.unresolved_issues)?;
    let missing_evidence = validate_completion_notes("Recipe missing evidence", missing_evidence)?;
    let encoded = [
        serde_json::to_string(&artifacts).map_err(|error| {
            WorkbenchError::invalid(format!("Invalid recipe artifacts: {error}"))
        })?,
        serde_json::to_string(&checks)
            .map_err(|error| WorkbenchError::invalid(format!("Invalid recipe checks: {error}")))?,
        serde_json::to_string(&unresolved_issues).map_err(|error| {
            WorkbenchError::invalid(format!("Invalid unresolved issues: {error}"))
        })?,
        serde_json::to_string(&missing_evidence).map_err(|error| {
            WorkbenchError::invalid(format!("Invalid missing evidence: {error}"))
        })?,
    ];
    if encoded.iter().any(|value| value.len() > 1024 * 1024) {
        return Err(WorkbenchError::invalid(
            "Recipe completion fields exceed 1 MiB",
        ));
    }
    let status = if unresolved_issues.is_empty() && missing_evidence.is_empty() {
        "completed"
    } else {
        "incomplete"
    };
    let [artifacts, checks, unresolved_issues, missing_evidence] = encoded;
    let changed = connection.execute("UPDATE recipe_runs SET status=?2, artifacts_json=?3, checks_json=?4, unresolved_issues_json=?5, missing_evidence_json=?6, completed_at=?7 WHERE id=?1 AND status='active'", params![request.recipe_run_id, status, artifacts, checks, unresolved_issues, missing_evidence, now()]).map_err(|error| WorkbenchError::storage("Failed to complete recipe", error))?;
    if changed != 1 {
        return Err(WorkbenchError::invalid(
            "Recipe run is missing or already complete",
        ));
    }
    get_recipe_run(store, &request.recipe_run_id)
}

fn validate_completion_notes(label: &str, values: Vec<String>) -> WorkbenchResult<Vec<String>> {
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let value = value.trim();
        if value.is_empty()
            || value.len() > 16 * 1024
            || value.chars().any(|character| character.is_control())
        {
            return Err(WorkbenchError::invalid(format!(
                "{label} contain an empty, oversized, or invalid item"
            )));
        }
        normalized.push(value.to_string());
    }
    Ok(normalized)
}

pub fn list_recipe_runs(store: &Store, session_id: &str) -> WorkbenchResult<Vec<RecipeRun>> {
    let connection = store.connection()?;
    let mut statement = connection.prepare("SELECT id, workspace_id, session_id, recipe_snapshot_json, status, artifacts_json, checks_json, unresolved_issues_json, missing_evidence_json, started_at, completed_at FROM recipe_runs WHERE session_id=?1 ORDER BY started_at DESC LIMIT 100").map_err(|error| WorkbenchError::storage("Failed to prepare recipe runs", error))?;
    let rows = statement
        .query_map([session_id], recipe_run_from_row)
        .map_err(|error| WorkbenchError::storage("Failed to list recipe runs", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode recipe runs", error))?;
    Ok(rows)
}

//! Optional Review handoffs, research recipes, evaluation budgets, and portable archives.
//!
//! This module is intentionally downstream of the core conversation runtime.  It never
//! reads Review storage or configuration and imported archives never execute tools.

use super::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, types::Type, OptionalExtension as _};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const HANDOFF_VERSION: u32 = 1;
const MAX_HANDOFF_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_HANDOFF_ENTRIES: usize = 20_000;
const MAX_RECIPE_TEXT_BYTES: usize = 256 * 1024;

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn new_id(prefix: &str) -> WorkbenchResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| WorkbenchError::storage("Failed to generate release object id", error))?;
    Ok(format!(
        "{prefix}_{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

fn validate_id(label: &str, value: &str) -> WorkbenchResult<()> {
    if value.is_empty()
        || value.len() > 200
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(WorkbenchError::invalid(format!("{label} is invalid")));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchRecipe {
    pub id: String,
    pub workspace_id: Option<String>,
    pub source_recipe_id: Option<String>,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub required_inputs: Vec<String>,
    pub required_tools: Vec<String>,
    pub suggested_permission_mode: String,
    pub expected_checks: Vec<String>,
    pub version: i64,
    pub revision: i64,
    pub built_in: bool,
}

fn builtin_recipes() -> Vec<ResearchRecipe> {
    let recipe = |id: &str,
                  name: &str,
                  description: &str,
                  instructions: &str,
                  required_inputs: &[&str],
                  required_tools: &[&str],
                  mode: &str,
                  checks: &[&str]| ResearchRecipe {
        id: id.to_string(),
        workspace_id: None,
        source_recipe_id: None,
        name: name.to_string(),
        description: description.to_string(),
        instructions: instructions.to_string(),
        required_inputs: required_inputs
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        required_tools: required_tools
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        suggested_permission_mode: mode.to_string(),
        expected_checks: checks.iter().map(|value| (*value).to_string()).collect(),
        version: 1,
        revision: 1,
        built_in: true,
    };
    vec![
        recipe(
            "manuscript_consistency",
            "Manuscript consistency",
            "Check consistency between the paper’s text, tables, appendices, and notation.",
            "Build a consistency ledger before proposing edits. Check each reported quantitative claim against the selected revision and attached results. Record exact locators and leave unsupported statements unresolved.",
            &["paper_revision"],
            &["paper_tools", "research_ledger"],
            "inspect",
            &["cross_section_claims", "table_prose_consistency", "appendix_cross_references", "notation_consistency"],
        ),
        recipe(
            "empirical_result_audit",
            "Empirical results audit",
            "Audit estimands, samples, units, specifications, inference, and reported results.",
            "Identify the estimand and variation first. Match every audited statement to a structured result or immutable output. Report incompatible samples, units, transformations, or inference rather than coercing a comparison.",
            &["paper_revision", "tested_execution_profile"],
            &["paper_tools", "research_execution", "research_ledger"],
            "edit",
            &["estimand_definition", "sample_alignment", "unit_alignment", "result_prose_consistency", "inference_method"],
        ),
        recipe(
            "theory_audit_recipe",
            "Theory audit",
            "Audit primitives, timing, equilibrium conditions, accounting, and comparative statics.",
            "Write down primitives and timing before checking derivations. Verify optimization and market-clearing conditions, units, resource constraints, limiting cases, and the scope of numerical examples.",
            &["paper_revision"],
            &["paper_tools", "research_ledger"],
            "inspect",
            &["primitives_and_timing", "resource_constraints", "units", "limiting_cases", "comparative_statics"],
        ),
        recipe(
            "limiting_cases",
            "Limiting cases",
            "Check that stated results reduce correctly in limiting and degenerate cases.",
            "List the parameters and the limits the paper itself invokes or implies (zero, infinity, symmetry, no frictions, a known nested model). Evaluate each stated result in every limit against the known special case. Record each limit as analytical when derived, numerical when computed on stated instances, and unresolved when neither applies. A numerical limit check covers only the instances tested.",
            &["paper_revision"],
            &["paper_tools", "research_ledger"],
            "inspect",
            &["limits_enumerated", "nested_case_agreement", "degenerate_case_behavior", "scope_of_numerical_limits"],
        ),
        recipe(
            "dimensional_check",
            "Dimensional and unit check",
            "Verify units, dimensions, normalizations, and timing conventions in every stated equation.",
            "Assign units or dimensions to each primitive, then propagate through every displayed equation, first-order condition, and reported quantity. Flag additions of unlike quantities, mismatched normalizations, and rate/level confusions. State the convention used for time, prices, and shares when the paper leaves it implicit.",
            &["paper_revision"],
            &["paper_tools", "research_ledger"],
            "inspect",
            &["primitive_units", "equation_consistency", "normalization_conventions", "reported_quantity_units"],
        ),
        recipe(
            "accounting_identities",
            "Accounting identities",
            "Check resource constraints, budget constraints, market clearing, and adding-up identities.",
            "Write each identity in the paper's own notation before checking it. Verify that resource, budget, and market-clearing conditions hold at the stated equilibrium, that transfers and taxes net out, and that shares or decompositions sum as claimed. Record symbolic identities separately from numerical checks on calibrated values.",
            &["paper_revision"],
            &["paper_tools", "research_ledger"],
            "inspect",
            &["identities_stated", "resource_constraints", "budget_and_transfers", "adding_up"],
        ),
        recipe(
            "comparative_statics",
            "Comparative statics",
            "Check signs, monotonicity, and stated conditions of comparative-statics claims.",
            "For each comparative-statics claim, identify the equilibrium condition, the parameter, and the sufficient condition the paper gives. Derive or verify the sign under that condition; note where the claim relies on a numerical example, a special functional form, or an unstated regularity condition. Do not extend a result beyond the conditions actually established.",
            &["paper_revision"],
            &["paper_tools", "research_ledger"],
            "inspect",
            &["claims_enumerated", "sufficient_conditions", "sign_derivation", "functional_form_dependence"],
        ),
        recipe(
            "numerical_counterexample",
            "Numerical counterexample search",
            "Search a stated parameter domain for instances that violate a proposition or conjecture.",
            "State the proposition, its assumptions, and the parameter domain to search. Use the tested execution profile to evaluate candidate instances; report tolerance, precision, and every violating instance with its parameters. A search that finds no counterexample supports the statement only on the instances tested and never establishes a general proof.",
            &["paper_revision", "tested_execution_profile"],
            &["paper_tools", "research_execution", "research_ledger"],
            "edit",
            &["statement_and_domain", "search_coverage", "tolerance_and_precision", "violating_instances", "scope_statement"],
        ),
        recipe(
            "literature_precedent_review",
            "Literature and precedent review",
            "Check claims and originality against the primary sources you can access.",
            "Prefer primary sources. Record source access state and search coverage. Separate evidence for a mechanism from a merely related citation, and label novelty claims unresolved when coverage is incomplete.",
            &["paper_revision", "primary_source"],
            &["paper_tools", "research_ledger"],
            "inspect",
            &["source_identity", "claim_support", "mechanism_support", "coverage_statement", "novelty_scope"],
        ),
    ]
}

fn recipe_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchRecipe> {
    let required_inputs: String = row.get(6)?;
    let required_tools: String = row.get(7)?;
    let expected_checks: String = row.get(9)?;
    Ok(ResearchRecipe {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        source_recipe_id: row.get(2)?,
        name: row.get(3)?,
        description: row.get(4)?,
        instructions: row.get(5)?,
        required_inputs: decode_recipe_json(6, &required_inputs)?,
        required_tools: decode_recipe_json(7, &required_tools)?,
        suggested_permission_mode: row.get(8)?,
        expected_checks: decode_recipe_json(9, &expected_checks)?,
        version: row.get(10)?,
        revision: row.get(11)?,
        built_in: false,
    })
}

fn decode_recipe_json<T: DeserializeOwned>(column: usize, value: &str) -> rusqlite::Result<T> {
    serde_json::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
    })
}

pub fn list_recipes(
    store: &Store,
    workspace_id: Option<&str>,
) -> WorkbenchResult<Vec<ResearchRecipe>> {
    let mut recipes = builtin_recipes();
    let connection = store.connection()?;
    let mut statement = connection.prepare("SELECT id, workspace_id, source_recipe_id, name, description, instructions, required_inputs_json, required_tools_json, suggested_permission_mode, expected_checks_json, version, revision FROM recipe_definitions WHERE archived_at IS NULL AND (workspace_id IS NULL OR workspace_id = ?1) ORDER BY name, id").map_err(|error| WorkbenchError::storage("Failed to prepare recipe list", error))?;
    let custom = statement
        .query_map([workspace_id], recipe_from_row)
        .map_err(|error| WorkbenchError::storage("Failed to list recipes", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode recipes", error))?;
    recipes.extend(custom);
    Ok(recipes)
}

pub(crate) fn find_recipe(
    store: &Store,
    workspace_id: Option<&str>,
    id: &str,
) -> WorkbenchResult<ResearchRecipe> {
    list_recipes(store, workspace_id)?
        .into_iter()
        .find(|recipe| recipe.id == id)
        .ok_or_else(|| WorkbenchError::invalid("Recipe is unavailable in this Workspace"))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRecipeRequest {
    pub workspace_id: String,
    pub source_recipe_id: String,
    pub name: String,
}

pub fn clone_recipe(store: &Store, request: CloneRecipeRequest) -> WorkbenchResult<ResearchRecipe> {
    validate_id("workspace id", &request.workspace_id)?;
    let source = find_recipe(
        store,
        Some(&request.workspace_id),
        &request.source_recipe_id,
    )?;
    let name = request.name.trim();
    if name.is_empty() || name.len() > 300 {
        return Err(WorkbenchError::invalid(
            "Recipe name must contain 1 to 300 bytes",
        ));
    }
    let id = new_id("recipe")?;
    let timestamp = now();
    let required_inputs = serde_json::to_string(&source.required_inputs)
        .map_err(|error| WorkbenchError::storage("Failed to encode recipe inputs", error))?;
    let required_tools = serde_json::to_string(&source.required_tools)
        .map_err(|error| WorkbenchError::storage("Failed to encode recipe tools", error))?;
    let expected_checks = serde_json::to_string(&source.expected_checks)
        .map_err(|error| WorkbenchError::storage("Failed to encode recipe checks", error))?;
    store.connection()?.execute("INSERT INTO recipe_definitions (id, workspace_id, source_recipe_id, name, description, instructions, required_inputs_json, required_tools_json, suggested_permission_mode, expected_checks_json, version, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12, ?12)", params![id, request.workspace_id, source.id, name, source.description, source.instructions, required_inputs, required_tools, source.suggested_permission_mode, expected_checks, source.version, timestamp]).map_err(|error| WorkbenchError::storage("Failed to clone recipe", error))?;
    find_recipe(store, Some(&request.workspace_id), &id)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecipeRequest {
    pub recipe_id: String,
    pub expected_revision: i64,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub required_inputs: Vec<String>,
    pub required_tools: Vec<String>,
    pub suggested_permission_mode: String,
    pub expected_checks: Vec<String>,
}

fn encode_recipe_list(label: &str, values: &[String], max_items: usize) -> WorkbenchResult<String> {
    let normalized = values.iter().map(|value| value.trim()).collect::<Vec<_>>();
    let mut unique = HashSet::with_capacity(normalized.len());
    if normalized.len() > max_items
        || normalized.iter().any(|value| {
            value.is_empty()
                || value.len() > 200
                || value.chars().any(char::is_control)
                || !unique.insert(*value)
        })
    {
        return Err(WorkbenchError::invalid(format!(
            "{label} must contain at most {max_items} unique nonempty values of at most 200 bytes"
        )));
    }
    let encoded = serde_json::to_string(&normalized)
        .map_err(|error| WorkbenchError::invalid(format!("Invalid {label}: {error}")))?;
    if encoded.len() > 64 * 1024 {
        return Err(WorkbenchError::invalid(format!("{label} exceeds 64 KiB")));
    }
    Ok(encoded)
}

pub fn update_recipe(
    store: &Store,
    request: UpdateRecipeRequest,
) -> WorkbenchResult<ResearchRecipe> {
    validate_id("recipe id", &request.recipe_id)?;
    let required_inputs = encode_recipe_list("Required inputs", &request.required_inputs, 32)?;
    let required_tools = encode_recipe_list("Required tools", &request.required_tools, 32)?;
    let expected_checks = encode_recipe_list("Expected checks", &request.expected_checks, 64)?;
    if request.name.trim().is_empty()
        || request.name.len() > 300
        || request.description.len() > 4 * 1024
        || request.instructions.trim().is_empty()
        || request.instructions.len() > MAX_RECIPE_TEXT_BYTES
        || !matches!(
            request.suggested_permission_mode.as_str(),
            "inspect" | "edit"
        )
    {
        return Err(WorkbenchError::invalid(
            "Recipe fields exceed their bounds or permission mode is invalid",
        ));
    }
    let connection = store.connection()?;
    let changed = connection.execute("UPDATE recipe_definitions SET name=?2, description=?3, instructions=?4, required_inputs_json=?5, required_tools_json=?6, suggested_permission_mode=?7, expected_checks_json=?8, revision=revision+1, version=version+1, updated_at=?9 WHERE id=?1 AND revision=?10", params![request.recipe_id, request.name.trim(), request.description.trim(), request.instructions, required_inputs, required_tools, request.suggested_permission_mode, expected_checks, now(), request.expected_revision]).map_err(|error| WorkbenchError::storage("Failed to update recipe", error))?;
    if changed != 1 {
        return Err(WorkbenchError::invalid(
            "Recipe changed or is built in; refresh before editing",
        ));
    }
    let workspace_id: Option<String> = connection
        .query_row(
            "SELECT workspace_id FROM recipe_definitions WHERE id=?1",
            [&request.recipe_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to resolve recipe Workspace", error))?
        .flatten();
    find_recipe(store, workspace_id.as_deref(), &request.recipe_id)
}

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

fn recipe_run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecipeRun> {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceBudget {
    pub metric: String,
    pub unit: String,
    pub budget_value: f64,
    pub rationale: String,
}

pub fn performance_budgets() -> Vec<PerformanceBudget> {
    vec![
        PerformanceBudget {
            metric: "cold_route_load".into(),
            unit: "ms".into(),
            budget_value: 1200.0,
            rationale: "Workspace route becomes interactive on a release-class desktop".into(),
        },
        PerformanceBudget {
            metric: "warm_conversation_open".into(),
            unit: "ms".into(),
            budget_value: 250.0,
            rationale: "Bounded local hydration for the visible transcript window".into(),
        },
        PerformanceBudget {
            metric: "send_to_first_visible_state".into(),
            unit: "ms".into(),
            budget_value: 150.0,
            rationale: "Persisted user intent or visible pending state, excluding provider latency"
                .into(),
        },
        PerformanceBudget {
            metric: "stream_update_interval".into(),
            unit: "ms".into(),
            budget_value: 100.0,
            rationale: "Coalesced streaming remains perceptually continuous".into(),
        },
        PerformanceBudget {
            metric: "large_transcript_window".into(),
            unit: "ms".into(),
            budget_value: 100.0,
            rationale: "Render at most 200 transcript items initially".into(),
        },
        PerformanceBudget {
            metric: "context_assembly".into(),
            unit: "ms".into(),
            budget_value: 500.0,
            rationale: "Bounded 512 KiB deterministic context assembly".into(),
        },
        PerformanceBudget {
            metric: "cancel_visible".into(),
            unit: "ms".into(),
            budget_value: 250.0,
            rationale: "Cancellation request becomes visible independently of process exit".into(),
        },
        PerformanceBudget {
            metric: "idle_cpu".into(),
            unit: "percent".into(),
            budget_value: 2.0,
            rationale: "Five-minute idle median after route hydration on a release-class desktop"
                .into(),
        },
        PerformanceBudget {
            metric: "idle_memory".into(),
            unit: "mib".into(),
            budget_value: 350.0,
            rationale: "Resident memory after opening and idling a plain conversation".into(),
        },
    ]
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPerformanceRequest {
    pub metric: String,
    pub observed_value: f64,
    pub details: Value,
}

pub fn record_performance(
    store: &Store,
    request: RecordPerformanceRequest,
) -> WorkbenchResult<Value> {
    let budget = performance_budgets()
        .into_iter()
        .find(|candidate| candidate.metric == request.metric)
        .ok_or_else(|| WorkbenchError::invalid("Unknown performance metric"))?;
    if !request.observed_value.is_finite() || request.observed_value < 0.0 {
        return Err(WorkbenchError::invalid(
            "Observed duration cannot be negative",
        ));
    }
    let id = new_id("perf")?;
    let passed = request.observed_value <= budget.budget_value;
    store.connection()?.execute("INSERT INTO performance_samples (id, metric, unit, budget_value, observed_value, passed, details_json, measured_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![id, request.metric, budget.unit, budget.budget_value, request.observed_value, passed, request.details.to_string(), now()]).map_err(|error| WorkbenchError::storage("Failed to record performance sample", error))?;
    Ok(
        json!({"id":id,"metric":budget.metric,"unit":budget.unit,"budgetValue":budget.budget_value,"observedValue":request.observed_value,"passed":passed}),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationRecord {
    pub id: String,
    pub fixture_id: String,
    pub fixture_version: i64,
    pub variant: String,
    pub model: Option<String>,
    pub settings: Value,
    pub outcome: Value,
    pub latency_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordEvaluationRequest {
    pub fixture_id: String,
    pub fixture_version: i64,
    pub variant: String,
    pub model: Option<String>,
    pub settings: Value,
    pub outcome: Value,
    pub latency_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

fn evaluation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvaluationRecord> {
    let settings: String = row.get(5)?;
    let outcome: String = row.get(6)?;
    Ok(EvaluationRecord {
        id: row.get(0)?,
        fixture_id: row.get(1)?,
        fixture_version: row.get(2)?,
        variant: row.get(3)?,
        model: row.get(4)?,
        settings: serde_json::from_str(&settings).unwrap_or(Value::Null),
        outcome: serde_json::from_str(&outcome).unwrap_or(Value::Null),
        latency_ms: row.get(7)?,
        input_tokens: row.get(8)?,
        output_tokens: row.get(9)?,
        created_at: row.get(10)?,
    })
}

pub fn record_evaluation(
    store: &Store,
    request: RecordEvaluationRequest,
) -> WorkbenchResult<EvaluationRecord> {
    validate_id("fixture id", &request.fixture_id)?;
    if request.fixture_version < 1
        || !matches!(
            request.variant.as_str(),
            "recipe" | "plain_workspace" | "ordinary_codex"
        )
        || !request.settings.is_object()
        || !request.outcome.is_object()
        || request.latency_ms.is_some_and(|value| value < 0)
        || request.input_tokens.is_some_and(|value| value < 0)
        || request.output_tokens.is_some_and(|value| value < 0)
    {
        return Err(WorkbenchError::invalid(
            "Evaluation variant, fixture version, settings, outcome, latency, or usage is invalid",
        ));
    }
    let settings = serde_json::to_string(&request.settings).map_err(|error| {
        WorkbenchError::invalid(format!("Invalid evaluation settings: {error}"))
    })?;
    let outcome = serde_json::to_string(&request.outcome)
        .map_err(|error| WorkbenchError::invalid(format!("Invalid evaluation outcome: {error}")))?;
    if settings.len() > 256 * 1024 || outcome.len() > 1024 * 1024 {
        return Err(WorkbenchError::invalid(
            "Evaluation record exceeds its size limit",
        ));
    }
    let id = new_id("evaluation")?;
    let timestamp = now();
    let connection = store.connection()?;
    connection.execute("INSERT INTO research_evaluations (id,fixture_id,fixture_version,variant,model,settings_json,outcome_json,latency_ms,input_tokens,output_tokens,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![id,request.fixture_id,request.fixture_version,request.variant,request.model,settings,outcome,request.latency_ms,request.input_tokens,request.output_tokens,timestamp]).map_err(|error| WorkbenchError::storage("Failed to record research evaluation",error))?;
    connection.query_row("SELECT id,fixture_id,fixture_version,variant,model,settings_json,outcome_json,latency_ms,input_tokens,output_tokens,created_at FROM research_evaluations WHERE id=?1",[id],evaluation_from_row).map_err(|error| WorkbenchError::storage("Failed to read research evaluation",error))
}

pub fn list_evaluations(
    store: &Store,
    fixture_id: Option<&str>,
) -> WorkbenchResult<Vec<EvaluationRecord>> {
    let connection = store.connection()?;
    let mut statement=connection.prepare("SELECT id,fixture_id,fixture_version,variant,model,settings_json,outcome_json,latency_ms,input_tokens,output_tokens,created_at FROM research_evaluations WHERE (?1 IS NULL OR fixture_id=?1) ORDER BY created_at DESC LIMIT 500").map_err(|error| WorkbenchError::storage("Failed to prepare research evaluations",error))?;
    let records = statement
        .query_map([fixture_id], evaluation_from_row)
        .map_err(|error| WorkbenchError::storage("Failed to list research evaluations", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode research evaluations", error))?;
    Ok(records)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareReviewHandoffRequest {
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub paper_id: String,
    pub metadata: Value,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkReviewHandoffRequest {
    pub handoff_id: String,
    pub external_reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewHandoff {
    pub version: u32,
    pub id: String,
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub paper_id: String,
    pub revision_id: String,
    pub content_hash: String,
    pub media_kind: String,
    pub staged_path: String,
    pub input_interpretation: String,
    pub metadata: Value,
    pub external_reference: Option<String>,
    pub created_at: String,
    pub linked_at: Option<String>,
}

fn handoff_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewHandoff> {
    let metadata: String = row.get(9)?;
    Ok(ReviewHandoff {
        version: row.get::<_, i64>(0)? as u32,
        id: row.get(1)?,
        workspace_id: row.get(2)?,
        session_id: row.get(3)?,
        paper_id: row.get(4)?,
        revision_id: row.get(5)?,
        content_hash: row.get(6)?,
        media_kind: row.get(7)?,
        staged_path: row.get(8)?,
        metadata: serde_json::from_str(&metadata).unwrap_or(json!({})),
        input_interpretation: row.get(10)?,
        external_reference: row.get(11)?,
        created_at: row.get(12)?,
        linked_at: row.get(13)?,
    })
}

fn copy_tree(
    source: &Path,
    destination: &Path,
    count: &mut usize,
    bytes: &mut u64,
) -> WorkbenchResult<()> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| WorkbenchError::storage("Failed to inspect handoff input", error))?;
    if metadata.file_type().is_symlink() {
        return Err(WorkbenchError::invalid(
            "Review handoff input contains a symbolic link",
        ));
    }
    if metadata.is_file() {
        *count += 1;
        *bytes = bytes.saturating_add(metadata.len());
        if *count > MAX_HANDOFF_ENTRIES || *bytes > MAX_HANDOFF_BYTES {
            return Err(WorkbenchError::invalid(
                "Review handoff exceeds its file or size limit",
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                WorkbenchError::storage("Failed to create handoff staging directory", error)
            })?;
        }
        fs::copy(source, destination)
            .map_err(|error| WorkbenchError::storage("Failed to stage Review handoff", error))?;
    } else if metadata.is_dir() {
        fs::create_dir_all(destination).map_err(|error| {
            WorkbenchError::storage("Failed to create handoff directory", error)
        })?;
        for entry in fs::read_dir(source)
            .map_err(|error| WorkbenchError::storage("Failed to read handoff directory", error))?
        {
            let entry = entry
                .map_err(|error| WorkbenchError::storage("Failed to read handoff entry", error))?;
            copy_tree(
                &entry.path(),
                &destination.join(entry.file_name()),
                count,
                bytes,
            )?;
        }
    } else {
        return Err(WorkbenchError::invalid(
            "Review handoff input is not a regular file or directory",
        ));
    }
    Ok(())
}

pub fn prepare_review_handoff(
    store: &Store,
    request: PrepareReviewHandoffRequest,
) -> WorkbenchResult<ReviewHandoff> {
    validate_id("operation id", &request.operation_id)?;
    validate_id("workspace id", &request.workspace_id)?;
    validate_id("paper id", &request.paper_id)?;
    if let Some(session_id) = &request.session_id {
        validate_id("session id", session_id)?;
    }
    if !request.metadata.is_object() || request.metadata.to_string().len() > 64 * 1024 {
        return Err(WorkbenchError::invalid(
            "Handoff metadata must be an object no larger than 64 KiB",
        ));
    }
    let connection = store.connection()?;
    if let Some(existing) = connection.query_row("SELECT version,id,workspace_id,session_id,paper_id,revision_id,content_hash,media_kind,staged_path,metadata_json,input_interpretation,external_reference,created_at,linked_at FROM review_handoffs WHERE operation_id=?1", [&request.operation_id], handoff_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to check handoff idempotency", error))? {
        if existing.workspace_id != request.workspace_id
            || existing.session_id != request.session_id
            || existing.paper_id != request.paper_id
            || existing.metadata != request.metadata
        {
            return Err(WorkbenchError::invalid(
                "Review handoff operation id was already used for different inputs",
            ));
        }
        return Ok(existing);
    }
    if let Some(session_id) = &request.session_id {
        let belongs: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?1 AND workspace_id=?2)",
                params![session_id, request.workspace_id],
                |row| row.get(0),
            )
            .map_err(|error| {
                WorkbenchError::storage("Failed to validate handoff conversation", error)
            })?;
        if !belongs {
            return Err(WorkbenchError::invalid(
                "Review handoff conversation is not in the selected Workspace",
            ));
        }
    }
    let (revision_id, hash, kind, reference): (String,String,String,String) = connection.query_row("SELECT pr.id,pr.content_hash,a.media_kind,a.storage_reference FROM papers p JOIN paper_revisions pr ON pr.id=p.current_revision_id JOIN artifacts a ON a.workspace_id=p.workspace_id AND a.content_hash=pr.content_hash WHERE p.id=?1 AND p.workspace_id=?2 ORDER BY CASE WHEN a.media_kind=pr.input_kind THEN 0 ELSE 1 END LIMIT 1", params![request.paper_id, request.workspace_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional().map_err(|error| WorkbenchError::storage("Failed to resolve handoff revision", error))?.ok_or_else(|| WorkbenchError::invalid("The selected paper has no immutable current revision"))?;
    let source = PathBuf::from(reference);
    if !source.starts_with(store.root_path().join("blobs")) {
        return Err(WorkbenchError::invalid(
            "Paper revision is outside the Workspace immutable blob store",
        ));
    }
    let id = new_id("review_handoff")?;
    let destination = store
        .root_path()
        .join("handoffs")
        .join(&id)
        .join(source.file_name().unwrap_or_default());
    let mut count = 0;
    let mut bytes = 0;
    copy_tree(&source, &destination, &mut count, &mut bytes)?;
    let interpretation = if source.is_dir() {
        "source_tree"
    } else {
        "document"
    };
    let timestamp = now();
    connection.execute("INSERT INTO review_handoffs (id,operation_id,version,workspace_id,session_id,paper_id,revision_id,content_hash,media_kind,staged_path,input_interpretation,metadata_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)", params![id,request.operation_id,HANDOFF_VERSION,request.workspace_id,request.session_id,request.paper_id,revision_id,hash,kind,destination.to_string_lossy(),interpretation,request.metadata.to_string(),timestamp]).map_err(|error| WorkbenchError::storage("Failed to record Review handoff", error))?;
    connection.query_row("SELECT version,id,workspace_id,session_id,paper_id,revision_id,content_hash,media_kind,staged_path,metadata_json,input_interpretation,external_reference,created_at,linked_at FROM review_handoffs WHERE id=?1", [id], handoff_from_row).map_err(|error| WorkbenchError::storage("Failed to read Review handoff", error))
}

pub fn link_review_handoff(
    store: &Store,
    request: LinkReviewHandoffRequest,
) -> WorkbenchResult<ReviewHandoff> {
    if request.external_reference.trim().is_empty() || request.external_reference.len() > 2048 {
        return Err(WorkbenchError::invalid(
            "External Review reference must contain 1 to 2048 bytes",
        ));
    }
    let connection = store.connection()?;
    let current: Option<String> = connection
        .query_row(
            "SELECT external_reference FROM review_handoffs WHERE id=?1",
            [&request.handoff_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to inspect handoff link", error))?
        .flatten();
    if let Some(current) = current {
        if current != request.external_reference {
            return Err(WorkbenchError::invalid(
                "Handoff is already linked to a different external reference",
            ));
        }
    }
    connection.execute("UPDATE review_handoffs SET external_reference=COALESCE(external_reference,?2), linked_at=COALESCE(linked_at,?3) WHERE id=?1", params![request.handoff_id,request.external_reference,now()]).map_err(|error| WorkbenchError::storage("Failed to link Review handoff", error))?;
    connection.query_row("SELECT version,id,workspace_id,session_id,paper_id,revision_id,content_hash,media_kind,staged_path,metadata_json,input_interpretation,external_reference,created_at,linked_at FROM review_handoffs WHERE id=?1", [&request.handoff_id], handoff_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to read linked handoff", error))?.ok_or_else(|| WorkbenchError::invalid("Review handoff was not found"))
}

mod archive;
mod exchange;
mod retention;
mod workflow_draft;
pub use archive::{
    export_archive, import_archive, inspect_archive, refresh_retained_blobs, ArchiveInspection,
    ArchiveReport, ExportArchiveRequest, ImportArchiveRequest,
};
pub use exchange::*;
pub use retention::*;
pub use workflow_draft::*;
#[cfg(test)]
#[path = "release/tests.rs"]
mod tests;

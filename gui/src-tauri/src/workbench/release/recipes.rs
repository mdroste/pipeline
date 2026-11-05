//! Versioned research recipe definitions and their scoped persistence.

use super::{decode_recipe_json, new_id, now, validate_id};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const MAX_RECIPE_TEXT_BYTES: usize = 256 * 1024;

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

pub(super) fn builtin_recipes() -> Vec<ResearchRecipe> {
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

//! Optional Review handoffs, research recipes, evaluation budgets, and portable archives.
//!
//! This module is intentionally downstream of the core conversation runtime.  It never
//! reads Review storage or configuration and imported archives never execute tools.
//! Recipe definitions, runs, evaluations, and handoffs have separate owners below.

use super::store::{WorkbenchError, WorkbenchResult};
use rusqlite::types::Type;
use serde::de::DeserializeOwned;

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

fn decode_recipe_json<T: DeserializeOwned>(column: usize, value: &str) -> rusqlite::Result<T> {
    serde_json::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
    })
}

mod evaluations;
mod recipe_runs;
mod recipes;
mod review_handoff;

pub use evaluations::{
    list_evaluations, performance_budgets, record_evaluation, record_performance, EvaluationRecord,
    PerformanceBudget, RecordEvaluationRequest, RecordPerformanceRequest,
};
use recipe_runs::recipe_run_from_row;
pub use recipe_runs::{
    check_recipe_inputs, complete_recipe, list_recipe_runs, start_recipe, CompleteRecipeRequest,
    InputCheck, RecipeRun, StartRecipeRequest,
};
pub(crate) use recipes::find_recipe;
pub use recipes::{
    clone_recipe, list_recipes, update_recipe, CloneRecipeRequest, ResearchRecipe,
    UpdateRecipeRequest,
};
pub use review_handoff::{
    link_review_handoff, prepare_review_handoff, LinkReviewHandoffRequest,
    PrepareReviewHandoffRequest, ReviewHandoff,
};

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

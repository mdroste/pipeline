//! Research evaluation records and measured performance budgets.

use super::{new_id, now, validate_id};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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

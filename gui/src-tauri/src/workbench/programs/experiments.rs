use super::*;
use crate::workbench::research::{
    self,
    execution_plan::{self, ExecutionPlan},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Factor {
    pub name: String,
    pub values: Vec<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exclusion {
    pub matches: BTreeMap<String, Value>,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Specification {
    pub ordinal: usize,
    pub factors: BTreeMap<String, Value>,
    pub plan_id: Option<String>,
    pub excluded_reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentPlan {
    pub schema_version: u32,
    pub question: String,
    pub base_plan_id: String,
    pub factors: Vec<Factor>,
    pub exclusions: Vec<Exclusion>,
    pub specifications: Vec<Specification>,
    pub selected_outputs: Vec<String>,
    pub interpretation: String,
    pub total_runs: usize,
    pub timeout_budget_seconds: u64,
    pub captured_input_bytes: u64,
    pub sources: Vec<ResearchObjectRef>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prepare {
    pub workspace_id: String,
    pub title: String,
    pub question: String,
    pub base_plan_id: String,
    pub factors: Vec<Factor>,
    pub exclusions: Vec<Exclusion>,
    pub interpretation: String,
    pub max_runs: usize,
    pub max_seconds: u64,
    pub operation_id: String,
}
fn expand(factors: &[Factor], limit: usize) -> WorkbenchResult<Vec<BTreeMap<String, Value>>> {
    if factors.is_empty() || factors.len() > 8 || limit == 0 || limit > 64 {
        return Err(WorkbenchError::invalid(
            "Use 1–8 factors and at most 64 specifications",
        ));
    }
    let mut result = vec![BTreeMap::new()];
    let mut names = std::collections::HashSet::new();
    for f in factors {
        bounded(&f.name, 100)?;
        let upper = f.name.to_uppercase();
        if ["KEY", "TOKEN", "PASSWORD", "SECRET", "CREDENTIAL"]
            .iter()
            .any(|s| upper.contains(s))
            || f.values.iter().any(|v| v.to_string().len() > 2000)
        {
            return Err(WorkbenchError::invalid(
                "Keep credentials and oversized values out of factor grids",
            ));
        }
        if !names.insert(&f.name)
            || f.values.is_empty()
            || f.values.len() > 32
            || f.values
                .iter()
                .any(|v| !v.is_number() && !v.is_boolean() && !v.is_string())
        {
            return Err(WorkbenchError::invalid(
                "Factors require unique names and 1–32 scalar values",
            ));
        }
        if result.len().saturating_mul(f.values.len()) > limit {
            return Err(WorkbenchError::invalid(
                "The complete expanded grid exceeds its run limit",
            ));
        }
        let mut next = Vec::new();
        for row in result {
            for value in &f.values {
                let mut row = row.clone();
                row.insert(f.name.clone(), value.clone());
                next.push(row);
            }
        }
        result = next;
    }
    Ok(result)
}
pub fn prepare(store: &Store, r: Prepare) -> WorkbenchResult<DeskRecord> {
    bounded(&r.title, 450)?;
    bounded(&r.operation_id, 150)?;
    bounded(&r.question, 4000)?;
    bounded(&r.interpretation, 16000)?;
    let (base, p): (DeskRecord, ExecutionPlan) =
        load(store, &r.workspace_id, &r.base_plan_id, "execution_plan")?;
    let expanded = expand(&r.factors, r.max_runs)?;
    if r.exclusions.len() > 64 {
        return Err(WorkbenchError::invalid("Too many exclusion rules"));
    }
    for x in &r.exclusions {
        bounded(&x.reason, 4000)?;
        if x.matches.is_empty()
            || x.matches
                .keys()
                .any(|k| !r.factors.iter().any(|f| &f.name == k))
        {
            return Err(WorkbenchError::invalid(
                "Exclusions must name existing factors",
            ));
        }
    }
    let eligible = expanded
        .iter()
        .filter(|row| {
            !r.exclusions
                .iter()
                .any(|x| x.matches.iter().all(|(k, v)| row.get(k) == Some(v)))
        })
        .count();
    let budget = (eligible as u64).saturating_mul(p.profile.timeout_seconds);
    if eligible == 0 || budget > r.max_seconds || r.max_seconds > 7 * 24 * 3600 {
        return Err(WorkbenchError::invalid(
            "The grid has no runs or exceeds its declared timeout budget",
        ));
    }
    for factors in &expanded {
        let mut parameters = p.parameters.clone();
        for (k, v) in factors {
            parameters[k] = v.clone();
        }
        if parameters.to_string().len() > 16000 {
            return Err(WorkbenchError::invalid("Expanded parameters exceed 16 KiB"));
        }
    }
    let mut specifications = Vec::new();
    let mut count = 0;
    for (ordinal, factors) in expanded.into_iter().enumerate() {
        let excluded = r
            .exclusions
            .iter()
            .find(|x| x.matches.iter().all(|(k, v)| factors.get(k) == Some(v)))
            .map(|x| x.reason.clone());
        let plan_id = if excluded.is_none() {
            count += 1;
            let mut derived = p.clone();
            for (k, v) in &factors {
                derived.parameters[k] = v.clone();
            }
            let record = desk::insert(
                store,
                &r.workspace_id,
                "execution_plan",
                &format!("{} · specification {}", r.title, ordinal + 1),
                serde_json::to_value(derived).map_err(err)?,
                None,
                &format!("{}-spec-{ordinal}", r.operation_id),
            )?;
            Some(record.id)
        } else {
            None
        };
        specifications.push(Specification {
            ordinal,
            factors,
            plan_id,
            excluded_reason: excluded,
        });
    }
    let bytes = p.input_manifest["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f["sizeBytes"].as_u64())
        .sum();
    let plan = ExperimentPlan {
        schema_version: 1,
        question: r.question,
        base_plan_id: base.id,
        factors: r.factors,
        exclusions: r.exclusions,
        specifications,
        selected_outputs: p.profile.outputs,
        interpretation: r.interpretation,
        total_runs: count,
        timeout_budget_seconds: budget,
        captured_input_bytes: bytes,
        sources: p.research_inputs,
    };
    let result = desk::insert(
        store,
        &r.workspace_id,
        "experiment_plan",
        &r.title,
        serde_json::to_value(plan).map_err(err)?,
        None,
        &r.operation_id,
    )?;
    store
        .connection()?
        .execute(
            "INSERT OR IGNORE INTO experiment_runs(plan_id,state,updated_at) VALUES(?1,'ready',?2)",
            params![result.id, desk::now()],
        )
        .map_err(err)?;
    Ok(result)
}
pub fn status(store: &Store, ws: &str, id: &str) -> WorkbenchResult<Value> {
    let (record, plan): (_, ExperimentPlan) = load(store, ws, id, "experiment_plan")?;
    let c = store.connection()?;
    let state:Value=c.query_row("SELECT state,revision,next_index,active_execution_id,reason,authorized_hash FROM experiment_runs WHERE plan_id=?1",[id],|r|Ok(json!({"state":r.get::<_,String>(0)?,"revision":r.get::<_,i64>(1)?,"nextIndex":r.get::<_,i64>(2)?,"activeExecutionId":r.get::<_,Option<String>>(3)?,"reason":r.get::<_,Option<String>>(4)?,"authorizedHash":r.get::<_,Option<String>>(5)?}))).map_err(err)?;
    let mut q=c.prepare("SELECT ordinal,execution_id,state,result_json FROM experiment_attempts WHERE plan_id=?1 ORDER BY ordinal").map_err(err)?;
    let attempts=q.query_map([id],|r|Ok(json!({"ordinal":r.get::<_,i64>(0)?,"executionId":r.get::<_,Option<String>>(1)?,"state":r.get::<_,String>(2)?,"receipt":serde_json::from_str::<Value>(&r.get::<_,String>(3)?).unwrap_or(Value::Null)}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    Ok(
        json!({"basePlan":desk::record(store,ws,&plan.base_plan_id)?,"record":record,"plan":plan,"run":state,"attempts":attempts}),
    )
}
pub fn control(
    store: &Store,
    ws: &str,
    id: &str,
    action: &str,
    fingerprint: &str,
) -> WorkbenchResult<Value> {
    let (record, plan): (_, ExperimentPlan) = load(store, ws, id, "experiment_plan")?;
    if record.content_hash != fingerprint {
        return Err(WorkbenchError::conflict(
            "The reviewed experiment plan changed",
        ));
    }
    match action {
        "start" => {
            let current = status(store, ws, id)?;
            if !matches!(current["run"]["state"].as_str(), Some("ready" | "paused")) {
                return Err(WorkbenchError::conflict("Only a ready or explicitly paused family can start; reconcile uncertain attempts first"));
            }
            for spec in &plan.specifications {
                if let Some(id) = &spec.plan_id {
                    let p = desk::record(store, ws, id)?;
                    execution_plan::authorize(store, ws, id, &p.content_hash)?;
                }
            }
            let current = status(store, ws, id)?;
            if current["run"]["state"] == "attention" {
                return Err(WorkbenchError::conflict(
                    "Reconcile the ambiguous attempt before resuming",
                ));
            }
            store.connection()?.execute("UPDATE experiment_runs SET state='running',authorized_hash=?2,process_instance=?3,revision=revision+1,reason=NULL,updated_at=?4 WHERE plan_id=?1 AND state IN ('ready','paused')",params![id,fingerprint,research::jobs::process_instance(),desk::now()]).map_err(err)?;
        }
        "pause" => {
            store.connection()?.execute("UPDATE experiment_runs SET state='paused',revision=revision+1,updated_at=?2 WHERE plan_id=?1 AND state='running'",params![id,desk::now()]).map_err(err)?;
        }
        "cancel" => {
            let current = status(store, ws, id)?;
            if let Some(execution) = current["run"]["activeExecutionId"].as_str() {
                research::cancel_execution(
                    store,
                    research::CancelExecutionRequest {
                        execution_id: execution.into(),
                    },
                )?;
            }
            store.connection()?.execute("UPDATE experiment_runs SET state='cancelled',revision=revision+1,updated_at=?2 WHERE plan_id=?1",params![id,desk::now()]).map_err(err)?;
        }
        "reconcile" => {
            reconcile(store, ws, id)?;
        }
        _ => return Err(WorkbenchError::invalid("Unknown experiment action")),
    }
    status(store, ws, id)
}
fn reconcile(store: &Store, ws: &str, id: &str) -> WorkbenchResult<()> {
    let current = status(store, ws, id)?;
    if let Some(execution) = current["run"]["activeExecutionId"].as_str() {
        let e = research::get_execution(store, execution)?;
        if ["queued", "running", "outcome_unknown"].contains(&e.outcome.as_str()) {
            return Err(WorkbenchError::invalid(
                "Inspect and reconcile this execution; its launch outcome is not established",
            ));
        }
        store
            .connection()?
            .execute(
                "UPDATE experiment_runs SET state='paused',reason=NULL WHERE plan_id=?1",
                [id],
            )
            .map_err(err)?;
    } else if current["run"]["state"] == "attention" {
        store
            .connection()?
            .execute(
                "UPDATE experiment_runs SET state='paused',reason=NULL WHERE plan_id=?1",
                [id],
            )
            .map_err(err)?;
    }
    Ok(())
}
/// Bounded serial producer. It never holds the database gate over a process wait.
pub fn tick(store: &Store, ws: &str, id: &str) -> WorkbenchResult<()> {
    let current = status(store, ws, id)?;
    let state = current["run"]["state"].as_str().unwrap_or("");
    if !["running", "paused", "cancelled"].contains(&state) {
        return Ok(());
    }
    let (_, plan): (_, ExperimentPlan) = load(store, ws, id, "experiment_plan")?;
    let instance: Option<String> = store
        .connection()?
        .query_row(
            "SELECT process_instance FROM experiment_runs WHERE plan_id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(err)?;
    if state == "running" && instance.as_deref() != Some(research::jobs::process_instance()) {
        store.connection()?.execute("UPDATE experiment_runs SET state='paused',reason='Pipeline restarted. Review the last receipt and explicitly resume remaining specifications.' WHERE plan_id=?1",[id]).map_err(err)?;
        return Ok(());
    }
    let mut index = current["run"]["nextIndex"].as_u64().unwrap_or(0) as usize;
    if let Some(id_run) = current["run"]["activeExecutionId"].as_str() {
        let e = research::get_execution(store, id_run)?;
        if ["queued", "running"].contains(&e.outcome.as_str()) {
            if !research::jobs::execution_active(id_run) {
                attention(store,id,"A prior process may have launched this attempt. Reconcile its receipt before continuing")?;
            }
            return Ok(());
        }
        if e.outcome == "outcome_unknown" {
            attention(store, id, "The active attempt has an unknown outcome")?;
            return Ok(());
        }
        store.connection()?.execute("UPDATE experiment_attempts SET state=?3,result_json=?4 WHERE plan_id=?1 AND ordinal=?2",params![id,index as i64,e.outcome,serde_json::to_string(&e).map_err(err)?]).map_err(err)?;
        index += 1;
        store.connection()?.execute("UPDATE experiment_runs SET next_index=?2,active_execution_id=NULL,revision=revision+1 WHERE plan_id=?1",params![id,index as i64]).map_err(err)?;
    }
    if state != "running" {
        return Ok(());
    }
    while index < plan.specifications.len() && plan.specifications[index].plan_id.is_none() {
        store.connection()?.execute("INSERT OR IGNORE INTO experiment_attempts(plan_id,ordinal,operation_id,state,result_json) VALUES(?1,?2,?3,'excluded',?4)",params![id,index as i64,format!("{id}-excluded-{index}"),json!({"reason":plan.specifications[index].excluded_reason}).to_string()]).map_err(err)?;
        index += 1;
    }
    if index == plan.specifications.len() {
        store.connection()?.execute("UPDATE experiment_runs SET state='finished',next_index=?2,revision=revision+1 WHERE plan_id=?1",params![id,index as i64]).map_err(err)?;
        return Ok(());
    }
    if research::jobs::workspace_active(ws) {
        return Ok(());
    }
    let child = plan.specifications[index].plan_id.as_ref().unwrap();
    let (_, p): (_, ExecutionPlan) = load(store, ws, child, "execution_plan")?;
    let operation = format!("grid-{}-{index}", &desk::hash(id.as_bytes())[..24]);
    let request = research::RunExecutionRequest {
        plan_id: Some(child.clone()),
        profile_id: p.profile.id,
        session_id: None,
        test_only: true,
        operation_id: operation.clone(),
    };
    match research::jobs::queue(store, request, "detached", None) {
        Ok(e) => {
            let mut c = store.connection()?;
            let tx = c.transaction().map_err(err)?;
            tx.execute("INSERT INTO experiment_attempts(plan_id,ordinal,operation_id,execution_id,state) VALUES(?1,?2,?3,?4,'queued') ON CONFLICT(plan_id,ordinal) DO UPDATE SET execution_id=?4",params![id,index as i64,operation,e.id]).map_err(err)?;
            tx.execute("UPDATE experiment_runs SET next_index=?2,active_execution_id=?3,revision=revision+1,updated_at=?4 WHERE plan_id=?1",params![id,index as i64,e.id,desk::now()]).map_err(err)?;
            tx.commit().map_err(err)?;
        }
        Err(e) => {
            if !e.message.contains("active")
                && !e.message.contains("already running")
                && !e.message.contains("write resources")
            {
                attention(store, id, &e.message)?;
            }
        }
    }
    Ok(())
}
fn attention(store: &Store, id: &str, reason: &str) -> WorkbenchResult<()> {
    store.connection()?.execute("UPDATE experiment_runs SET state='attention',reason=?2,revision=revision+1 WHERE plan_id=?1",params![id,reason]).map_err(err)?;
    Ok(())
}

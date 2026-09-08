use super::*;
use crate::workbench::{project, research, search};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Symbol {
    pub notation: String,
    pub scope: String,
    pub definition: String,
    pub domain: String,
    pub units: String,
    pub aliases: Vec<String>,
    pub sources: Vec<ResearchObjectRef>,
}
pub fn save(
    store: &Store,
    ws: &str,
    symbol: Symbol,
    supersedes: Option<&str>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    bounded(&symbol.notation, 200)?;
    bounded(&symbol.scope, 300)?;
    bounded(&symbol.definition, 8000)?;
    if symbol.domain.len() > 4000 || symbol.units.len() > 300 || symbol.aliases.len() > 20 {
        return Err(WorkbenchError::invalid("Symbol metadata exceeds its limit"));
    }
    for a in &symbol.aliases {
        bounded(a, 200)?;
    }
    refs(store, ws, &symbol.sources, 32)?;
    let title = format!("{} · {}", symbol.notation, symbol.scope);
    desk::insert(
        store,
        ws,
        "symbol",
        &title,
        serde_json::to_value(symbol).map_err(err)?,
        supersedes,
        operation,
    )
}
pub fn collisions(store: &Store, ws: &str) -> WorkbenchResult<Value> {
    let records = desk::records(store, ws, "symbol")?;
    let superseded = records
        .iter()
        .filter_map(|r| r.supersedes.as_ref())
        .collect::<std::collections::HashSet<_>>();
    let current = records
        .iter()
        .filter(|r| !superseded.contains(&r.id))
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    for (i, a) in current.iter().enumerate() {
        let sa: Symbol = serde_json::from_value(a.body.clone()).map_err(err)?;
        for b in current.iter().skip(i + 1) {
            let sb: Symbol = serde_json::from_value(b.body.clone()).map_err(err)?;
            let overlap = sa.scope == sb.scope || sa.scope == "global" || sb.scope == "global";
            if overlap
                && std::iter::once(&sa.notation)
                    .chain(sa.aliases.iter())
                    .any(|n| n == &sb.notation || sb.aliases.contains(n))
                && sa.definition != sb.definition
            {
                candidates.push(json!({"left":a.reference(),"right":b.reference(),"reason":"Overlapping scope and notation; inspect whether the definitions conflict"}));
                if candidates.len() == 100 {
                    return Ok(json!({"candidates":candidates,"truncated":true}));
                }
            }
        }
    }
    Ok(json!({"candidates":candidates,"truncated":false}))
}
pub fn candidates(store: &Store, ws: &str, source: ResearchObjectRef) -> WorkbenchResult<Value> {
    let view = search::read_object(store, ws, &source, 64 * 1024)?;
    let re = regex::Regex::new(
        r"\\[A-Za-z]+(?:_\{?[A-Za-z0-9]+\}?)?|\b[A-Za-z](?:_\{?[A-Za-z0-9]+\}?)?",
    )
    .unwrap();
    let mut seen = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    for m in re.find_iter(&view.text) {
        if seen.insert(m.as_str()) {
            result.push(json!({"notation":m.as_str(),"start":m.start(),"end":m.end()}));
            if result.len() == 200 {
                break;
            }
        }
    }
    Ok(
        json!({"source":source,"candidates":result,"truncated":view.truncated||result.len()==200,"notice":"Lexical candidates only. Text letters and TeX commands may not be symbols. Macro expansion, environments, and local scope require inspection of the original source."}),
    )
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Branch {
    pub workspace_id: String,
    pub source: ResearchObjectRef,
    pub title: String,
    pub assumptions: Vec<String>,
    pub question: String,
    pub operation_id: String,
}
pub fn branch(store: &Store, r: Branch) -> WorkbenchResult<DeskRecord> {
    if r.source.kind != "record" || r.source.start.is_some() || r.source.end.is_some() {
        return Err(WorkbenchError::invalid(
            "Choose a complete theory note revision",
        ));
    }
    let view = search::read_object(store, &r.workspace_id, &r.source, 64 * 1024)?;
    if view.truncated {
        return Err(WorkbenchError::invalid(
            "Theory branch source exceeds 64 KiB",
        ));
    }
    let mut note: project::TheoryNote = serde_json::from_str(&view.text).map_err(err)?;
    bounded(&r.title, 300)?;
    bounded(&r.question, 8000)?;
    note.title = r.title.clone();
    note.assumptions = r.assumptions.clone();
    note.assumption_ids.clear();
    note.status = "open".into();
    note.promotions.clear();
    note.rejection_reason.clear();
    note.origin = "manual".into();
    note.related_ids.push(r.source.id.clone());
    note.body = format!(
        "{}\n\nAlternative-assumption question: {}\nOriginal exact revision: {}",
        note.body, r.question, r.source.revision
    );
    let made = project::studio_mutate(
        store,
        project::StudioMutation {
            workspace_id: r.workspace_id.clone(),
            operation_id: format!("{}-note", r.operation_id),
            action: project::StudioAction::SaveTheory {
                id: None,
                expected_revision: 0,
                note,
            },
        },
    )?;
    desk::insert(
        store,
        &r.workspace_id,
        "assumption_branch",
        &r.title,
        json!({"sources":[r.source],"assumptions":r.assumptions,"question":r.question,"note":made,"status":"open","acceptedArgumentChanged":false}),
        None,
        &r.operation_id,
    )
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Check {
    pub workspace_id: String,
    pub theory: ResearchObjectRef,
    pub execution_id: String,
    pub expression: String,
    pub boundary_conditions: String,
    pub check: project::TheoryCheck,
    pub operation_id: String,
}
pub fn check(store: &Store, r: Check) -> WorkbenchResult<DeskRecord> {
    exact(store, &r.workspace_id, &r.theory)?;
    bounded(&r.expression, 16000)?;
    bounded(&r.boundary_conditions, 8000)?;
    if r.theory.kind != "record"
        || r.theory.id != r.check.theory_id
        || r.check.execution_id.as_deref() != Some(&r.execution_id)
    {
        return Err(WorkbenchError::invalid(
            "Check and execution must refer to the selected theory note",
        ));
    }
    if super::campaigns::freshness(store, &r.workspace_id, &r.theory) != "current" {
        return Err(WorkbenchError::conflict(
            "The theory note changed; select its current revision before attaching a new check",
        ));
    }
    let e = research::get_execution(store, &r.execution_id)?;
    if e.workspace_id != r.workspace_id || e.outcome != "completed" {
        return Err(WorkbenchError::invalid(
            "Choose a completed captured check execution",
        ));
    }
    let pid = e.input_manifest["planId"]
        .as_str()
        .ok_or_else(|| WorkbenchError::invalid("This check requires a captured execution plan"))?;
    let (p, plan): (_, research::execution_plan::ExecutionPlan) =
        load(store, &r.workspace_id, pid, "execution_plan")?;
    let made = project::studio_mutate(
        store,
        project::StudioMutation {
            workspace_id: r.workspace_id.clone(),
            operation_id: format!("{}-check", r.operation_id),
            action: project::StudioAction::SaveCheck {
                id: None,
                expected_revision: 0,
                check: r.check,
            },
        },
    )?;
    desk::insert(
        store,
        &r.workspace_id,
        "assumption_branch",
        "Executable assumption check",
        json!({"sources":[r.theory,p.reference()],"expression":r.expression,"boundaryConditions":r.boundary_conditions,"parameters":plan.parameters,"randomSeed":plan.random_seed,"engine":plan.toolchain_version,"executionId":e.id,"check":made,"scientificValidity":"The recorded method and tested domain determine scope; an execution is not a general proof"}),
        None,
        &r.operation_id,
    )
}

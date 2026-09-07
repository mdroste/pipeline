//! Draft a portable Workflow definition from selected successful research
//! steps. The draft is a document for the researcher to inspect and import
//! through the ordinary Workflow validation and launch preview. Nothing here
//! installs, schedules or executes anything, and computations that Workflow
//! steps cannot perform are listed as explicit prerequisites.
use super::{recipe_run_from_row, RecipeRun};
use crate::pipeline_config::{ProfileData, WorkflowDocument};
use crate::workbench::project;
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const MAX_STEPS: usize = 12;

fn err(e: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::storage("Workflow draft failed", e)
}
fn slug(text: &str, fallback: &str) -> String {
    let s: String = text
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|p| !p.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("_");
    if s.is_empty() {
        fallback.into()
    } else {
        s
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DraftWorkflowRequest {
    pub workspace_id: String,
    pub name: String,
    #[serde(default)]
    pub recipe_run_ids: Vec<String>,
    #[serde(default)]
    pub task_ids: Vec<String>,
    #[serde(default)]
    pub theory_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftStep {
    pub id: String,
    pub label: String,
    pub source: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftWorkflow {
    pub name: String,
    pub canonical_json: String,
    pub fingerprint: String,
    pub steps: Vec<DraftStep>,
    pub unsupported: Vec<String>,
    pub notes: Vec<String>,
}

fn recipe_run(store: &Store, ws: &str, id: &str) -> WorkbenchResult<RecipeRun> {
    store.connection()?.query_row("SELECT id, workspace_id, session_id, recipe_snapshot_json, status, artifacts_json, checks_json, unresolved_issues_json, missing_evidence_json, started_at, completed_at FROM recipe_runs WHERE id=?1 AND workspace_id=?2", params![id, ws], recipe_run_from_row).optional().map_err(err)?.ok_or_else(|| WorkbenchError::invalid("Recipe run is not in this Workspace"))
}

pub fn draft_workflow(
    store: &Store,
    request: DraftWorkflowRequest,
) -> WorkbenchResult<DraftWorkflow> {
    let ws = &request.workspace_id;
    store.workspace(ws)?;
    let name = request.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(WorkbenchError::invalid(
            "Name the draft workflow (1–120 characters)",
        ));
    }
    let total = request.recipe_run_ids.len() + request.task_ids.len() + request.theory_ids.len();
    if total == 0 || total > MAX_STEPS {
        return Err(WorkbenchError::invalid(format!(
            "Select 1–{MAX_STEPS} research steps to draft from"
        )));
    }
    let mut steps: Vec<Value> = Vec::new();
    let mut drafted = Vec::new();
    let mut unsupported = Vec::new();
    let mut notes = vec![
        "Every step reads the primary document only; Workflow steps cannot read Workspace notes, executions or evidence.".into(),
        "Review the prompts in the Workflow editor before launching; the draft carries no provider, model or credential choices.".into(),
    ];
    let mut used = std::collections::HashSet::new();
    let mut unique = |base: String| {
        let mut id = base.clone();
        let mut n = 2;
        while !used.insert(id.clone()) {
            id = format!("{base}_{n}");
            n += 1;
        }
        id
    };
    for run_id in &request.recipe_run_ids {
        let run = recipe_run(store, ws, run_id)?;
        if run.status != "completed" {
            unsupported.push(format!(
                "Recipe run {run_id} is {}; only completed runs are drafted",
                run.status
            ));
            continue;
        }
        if run
            .recipe
            .required_tools
            .iter()
            .any(|t| t == "research_execution")
            || !run.artifacts.is_empty()
        {
            unsupported.push(format!(
                "Recipe '{}' relied on local execution; Workflow steps cannot run computations. Rerun the execution profile before launching and attach its outputs as a named input.",
                run.recipe.name
            ));
        }
        let id = unique(slug(&run.recipe.name, "recipe"));
        let checks = run
            .recipe
            .expected_checks
            .iter()
            .map(|c| format!("- {}", c.replace('_', " ")))
            .collect::<Vec<_>>()
            .join("\n");
        let recorded = run
            .checks
            .iter()
            .filter_map(|c| c.get("name").and_then(Value::as_str))
            .map(|c| c.replace('_', " "))
            .collect::<Vec<_>>();
        let mut prompt = format!(
            "{}\n\nState explicitly whether each of these checks was performed and what it found:\n{checks}\n\nReport only concrete findings with exact locators. Do not summarize what the document does well.",
            run.recipe.instructions.trim()
        );
        if !recorded.is_empty() {
            prompt.push_str(&format!(
                "\n\nIn the original Workspace run the following checks were recorded: {}.",
                recorded.join(", ")
            ));
        }
        steps.push(json!({"id":id,"label":run.recipe.name,"prompt":prompt,"enabled":true,"phase":"parallel","agents":[],"context":{"include":[{"kind":"primary","parts":["text"]}]}}));
        drafted.push(DraftStep {
            id: id.clone(),
            label: run.recipe.name.clone(),
            source: format!("recipe run {run_id}"),
        });
    }
    for task_id in &request.task_ids {
        let record = project::record(store, ws, task_id, "task")?;
        let task: project::ResearchTask = serde_json::from_value(record.body).map_err(err)?;
        if task.status != "completed" {
            unsupported.push(format!(
                "Task {task_id} is {}; only completed tasks are drafted",
                task.status
            ));
            continue;
        }
        let id = unique(slug(&task.objective, "task"));
        let label: String = task
            .objective
            .lines()
            .next()
            .unwrap_or("Task")
            .chars()
            .take(60)
            .collect();
        let checks = if task.expected_checks.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nChecks to perform and report:\n{}",
                task.expected_checks
                    .iter()
                    .map(|c| format!("- {c}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        if task.expected_checks.iter().any(|c| {
            let l = c.to_ascii_lowercase();
            l.contains("run ")
                || l.contains("compile")
                || l.contains("stata")
                || l.contains("execute")
        }) {
            unsupported.push(format!(
                "Task '{label}' expects an execution or build; that step remains a prerequisite outside the Workflow."
            ));
        }
        steps.push(json!({"id":id,"label":label,"prompt":format!("{}{checks}\n\nReport concrete findings with exact locators.", task.objective.trim()),"enabled":true,"phase":"parallel","agents":[],"context":{"include":[{"kind":"primary","parts":["text"]}]}}));
        drafted.push(DraftStep {
            id,
            label,
            source: format!("task {task_id}"),
        });
    }
    for theory_id in &request.theory_ids {
        let record = project::record(store, ws, theory_id, "theory")?;
        let note: project::TheoryNote = serde_json::from_value(record.body).map_err(err)?;
        if note.status == "abandoned" {
            unsupported.push(format!(
                "Theory note '{}' is abandoned and is not drafted",
                note.title
            ));
            continue;
        }
        let id = unique(slug(&note.title, "theory"));
        let mut prompt = format!(
            "Check the following {} against the document.\n\nStatement: {}\n",
            note.kind.replace('_', " "),
            note.statement.trim()
        );
        if !note.assumptions.is_empty() {
            prompt.push_str(&format!(
                "Assumptions (do not change them): {}\n",
                note.assumptions.join("; ")
            ));
        }
        if !note.unresolved_steps.is_empty() {
            prompt.push_str(&format!(
                "Unresolved steps to examine: {}\n",
                note.unresolved_steps.join("; ")
            ));
        }
        prompt.push_str("\nDistinguish analytical arguments from numerical checks on specific instances; a numerical check never establishes the general statement. Report exact locators.");
        steps.push(json!({"id":id,"label":note.title,"prompt":prompt,"enabled":true,"phase":"parallel","agents":[],"context":{"include":[{"kind":"primary","parts":["text"]}]}}));
        drafted.push(DraftStep {
            id,
            label: note.title.clone(),
            source: format!("theory note {theory_id}"),
        });
    }
    if steps.is_empty() {
        return Err(WorkbenchError::invalid(
            "None of the selected steps can be drafted; see the unsupported list",
        ));
    }
    let include: Vec<Value> = steps
        .iter()
        .map(|s| json!({"kind":"step","step":s["id"],"parts":["report"]}))
        .collect();
    steps.push(json!({"id":"synthesis","label":"Consolidate findings","prompt":"Consolidate the independent analyses into one prioritized list of issues. Merge duplicates, keep exact locators, and preserve disagreements explicitly.\n\n{prior_outputs}","enabled":true,"phase":"sequential","agents":[],"context":{"include":include}}));
    let profile: ProfileData = serde_json::from_value(json!({
        "name": name,
        "steps": steps,
        "outputs": {"primary_step": "synthesis"}
    }))
    .map_err(|e| WorkbenchError::invalid(format!("Draft is not a valid profile: {e}")))?;
    let document = WorkflowDocument::from_profile_data(profile)
        .map_err(|e| WorkbenchError::invalid(format!("Draft failed Workflow validation: {e}")))?;
    if !unsupported.is_empty() {
        notes.push("Unsupported operations remain explicit prerequisites; prompt text never substitutes for a job scheduler.".into());
    }
    Ok(DraftWorkflow {
        name: document.name,
        canonical_json: document.canonical_json,
        fingerprint: document.fingerprint,
        steps: drafted,
        unsupported,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::project::{ProjectAction, ProjectMutation};
    use crate::workbench::store::CreateWorkspaceRequest;

    #[test]
    fn draft_validates_and_lists_unsupported_computation() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path()).unwrap();
        let ws = store
            .create_workspace(CreateWorkspaceRequest {
                name: "Draft".into(),
                root: None,
                operation_id: "create".into(),
            })
            .unwrap()
            .record
            .id;
        let make_task = |objective: &str, checks: Vec<&str>, status: &str| {
            let record: project::ProjectRecord = serde_json::from_value(
                project::mutate(
                    &store,
                    ProjectMutation {
                        workspace_id: ws.clone(),
                        operation_id: format!("t_{}", objective.len()),
                        action: ProjectAction::CreateTask {
                            objective: objective.into(),
                            anchor_id: None,
                            expected_outputs: vec![],
                            expected_checks: checks.into_iter().map(String::from).collect(),
                        },
                    },
                )
                .unwrap(),
            )
            .unwrap();
            project::mutate(
                &store,
                ProjectMutation {
                    workspace_id: ws.clone(),
                    operation_id: format!("u_{}", objective.len()),
                    action: ProjectAction::UpdateTask {
                        task_id: record.id.clone(),
                        expected_revision: record.revision,
                        status: status.into(),
                    },
                },
            )
            .unwrap();
            record.id
        };
        let done = make_task(
            "Check identification argument",
            vec!["Compare with appendix B"],
            "completed",
        );
        let compute = make_task(
            "Rerun regressions",
            vec!["Run the Stata do-file"],
            "completed",
        );
        let open = make_task("Not finished", vec![], "open");
        let draft = draft_workflow(
            &store,
            DraftWorkflowRequest {
                workspace_id: ws.clone(),
                name: "Referee prep".into(),
                recipe_run_ids: vec![],
                task_ids: vec![done, compute, open],
                theory_ids: vec![],
            },
        )
        .unwrap();
        assert_eq!(draft.steps.len(), 2);
        assert!(draft
            .unsupported
            .iter()
            .any(|u| u.contains("Not finished") || u.contains("open")));
        assert!(draft.unsupported.iter().any(|u| u.contains("prerequisite")));
        assert!(draft.fingerprint.starts_with("sha256:"));
        let parsed =
            crate::pipeline_config::parse_workflow_document_strict(&draft.canonical_json).unwrap();
        assert_eq!(parsed.name, "Referee prep");
        assert!(parsed.config.steps.iter().any(|s| s.id == "synthesis"));
        assert!(draft_workflow(
            &store,
            DraftWorkflowRequest {
                workspace_id: ws,
                name: "Empty".into(),
                recipe_run_ids: vec![],
                task_ids: vec![],
                theory_ids: vec![],
            }
        )
        .is_err());
    }
}

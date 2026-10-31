//! Immutable adapter preparation and owning-runtime availability.
use crate::orchestration::{
    definition::{Action, Chain, Step},
    store::{err, Result, Scope},
};
use crate::workbench::{commands::run_store, tasks};
use serde_json::Value;

pub async fn prepare(chain: &Chain, session: Option<String>, inputs: &Value) -> Result<Scope> {
    crate::orchestration::definition::validate(chain)?;
    let mut scope = Scope::default();
    if let Some(session) = session {
        let binding = run_store(move |s| tasks::binding(&s, &session))
            .await
            .map_err(|e| e.message)?;
        scope.root_identity = Some(binding.root_identity);
        scope.session_id = Some(binding.session_id);
        scope.workspace_id = binding.workspace_id;
        scope.session_cursor = Some(binding.cursor);
        scope.harness_fingerprint = Some(binding.harness_fingerprint);
        scope.connection_id = binding.connection_id;
        scope.runtime_root = Some(binding.runtime_root);
    }
    let mut all = Vec::new();
    flatten(&chain.steps, &mut all);
    for step in all {
        match &step.action {
            Action::LiteratureLookup { .. } => {
                let ws = scope
                    .workspace_id
                    .clone()
                    .ok_or("Literature lookup requires a project")?;
                if !run_store(move |s| crate::workbench::acquisition::network_enabled(&s, &ws))
                    .await
                    .map_err(|e| e.message)?
                {
                    return Err(
                        "Enable the project's literature acquisition before preparing this lookup"
                            .into(),
                    );
                }
            }
            Action::Workspace { .. } | Action::Deliver { .. } if scope.session_id.is_none() => {
                return Err("Choose a Workspace conversation for this chain".into())
            }
            Action::Review { profile_id, .. } => {
                let key = profile_id.clone();
                let pin =
                    tokio::task::spawn_blocking(move || crate::commands::orchestration::pin(&key))
                        .await
                        .map_err(err)??;
                if let Action::Review { variables, .. } = &step.action {
                    let document =
                        crate::pipeline_config::parse_workflow_document_strict(&pin.workflow_json)?;
                    if document
                        .config
                        .variables
                        .iter()
                        .any(|v| v.secret && variables.contains_key(&v.key))
                    {
                        return Err("Secret Review variables cannot be stored in task definitions. Configure them in Review settings.".into());
                    }
                }
                scope.profiles.insert(profile_id.clone(), pin);
            }
            Action::Check { profile_id } => {
                let workspace = scope
                    .workspace_id
                    .clone()
                    .ok_or("Checks require a Workspace project")?;
                let profile = profile_id.clone();
                let preview = run_store(move |s| {
                    if !crate::workbench::research::list_execution_profiles(&s, &workspace)?
                        .iter()
                        .any(|p| p.id == profile)
                    {
                        return Err(crate::workbench::store::WorkbenchError::invalid(
                            "Check belongs to another project",
                        ));
                    }
                    crate::workbench::research::preview_host_execution(&s, &profile)
                })
                .await
                .map_err(|e| e.message)?;
                if !preview.authorized {
                    return Err(format!(
                        "Authorize the selected check '{}' in Research tools first",
                        profile_id
                    ));
                }
                scope.checks.insert(profile_id.clone(), preview.fingerprint);
            }
            Action::CapturedCheck { plan_id } => {
                let workspace = scope
                    .workspace_id
                    .clone()
                    .ok_or("Captured checks require a Workspace project")?;
                let plan = plan_id.clone();
                let status = run_store(move |s| {
                    crate::workbench::research::execution_plan::status(&s, &workspace, &plan)
                })
                .await
                .map_err(|e| e.message)?;
                if !status.authorized {
                    return Err("Authorize this exact captured plan in Research tools first".into());
                }
                scope
                    .captured_checks
                    .insert(plan_id.clone(), status.record.content_hash);
            }
            _ => {}
        }
    }
    if !inputs.is_object() {
        return Err("Task inputs must be an object".into());
    }
    Ok(scope)
}

fn flatten<'a>(steps: &'a [Step], out: &mut Vec<&'a Step>) {
    for step in steps {
        out.push(step);
        match &step.action {
            Action::If {
                then_steps,
                else_steps,
                ..
            } => {
                flatten(then_steps, out);
                flatten(else_steps, out);
            }
            Action::Repeat { steps, .. }
            | Action::While { steps, .. }
            | Action::ForEach { steps, .. } => flatten(steps, out),
            Action::Parallel { branches } => {
                for branch in branches {
                    flatten(branch, out);
                }
            }
            Action::Chain { chain } => flatten(&chain.steps, out),
            _ => {}
        }
    }
}

pub fn task_binding(scope: &Scope) -> Result<tasks::TaskBinding> {
    Ok(tasks::TaskBinding {
        connection_id: scope.connection_id,
        session_id: scope.session_id.clone().ok_or("No conversation selected")?,
        workspace_id: scope.workspace_id.clone(),
        cursor: scope.session_cursor.clone().unwrap_or_default(),
        harness_fingerprint: scope
            .harness_fingerprint
            .clone()
            .ok_or("No research settings snapshot")?,
        root_identity: scope
            .root_identity
            .clone()
            .ok_or("Task root identity is missing; refresh context")?,
        runtime_root: scope
            .runtime_root
            .clone()
            .ok_or("No Workspace runtime root")?,
    })
}

pub fn available(action: &Action, scope: &Scope) -> bool {
    match action {
        Action::Workspace { .. } | Action::Check { .. } | Action::CapturedCheck { .. } => {
            crate::workbench::commands::task_turn_available()
                && !scope
                    .workspace_id
                    .as_deref()
                    .is_some_and(crate::workbench::research::jobs::workspace_active)
        }
        Action::Review { .. } => crate::commands::orchestration::available(),
        _ => true,
    }
}

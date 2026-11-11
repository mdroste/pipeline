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
        let (binding, display) = run_store(move |s| {
            let binding = tasks::binding(&s, &session)?;
            let snapshot = s.session_snapshot(&session)?;
            Ok((binding, snapshot))
        })
        .await
        .map_err(|e| e.message)?;
        scope.conversation_title = Some(display.session.title);
        scope.project_name = display.workspace.map(|workspace| workspace.name);
        scope.root_identity = Some(binding.root_identity);
        scope.session_id = Some(binding.session_id);
        scope.workspace_id = binding.workspace_id;
        scope.session_cursor = Some(binding.cursor);
        scope.harness_fingerprint = Some(binding.harness_fingerprint);
        scope.connection_id = binding.connection_id;
        scope.runtime_root = Some(binding.runtime_root);
        scope.permission_profile = binding.permission_profile;
        scope.preset_id = binding.preset_id;
        scope.access = binding.access;
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
                let document =
                    crate::pipeline_config::parse_workflow_document_strict(&pin.workflow_json)?;
                let mut config = document.config.clone();
                crate::pipeline_config::apply_agent_defaults(&mut config, &pin.settings);
                let roster = crate::commands::model_roster::model_roster(
                    &pin.settings,
                    &config,
                    &document.config,
                    false,
                    false,
                    false,
                );
                scope.review_models.insert(
                    profile_id.clone(),
                    serde_json::to_value(roster).map_err(err)?,
                );
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
                        "Authorize the selected check '{}' in the project's Results view first",
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
                    return Err(
                        "Authorize this exact captured plan in the project's Results view first"
                            .into(),
                    );
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

/// Pins each check to what it would run now, but only where the researcher has
/// approved exactly that. A check they have not approved keeps its old pin, so
/// the step stays refused, and its id is returned.
pub async fn repin_checks(scope: &mut Scope) -> Result<Vec<String>> {
    let Some(workspace) = scope.workspace_id.clone() else {
        return Ok(Vec::new());
    };
    let checks: Vec<_> = scope.checks.keys().cloned().collect();
    let plans: Vec<_> = scope.captured_checks.keys().cloned().collect();
    if checks.is_empty() && plans.is_empty() {
        return Ok(Vec::new());
    }
    let (approved_checks, approved_plans) = run_store(move |s| {
        let profiles = crate::workbench::research::list_execution_profiles(&s, &workspace)?;
        let mut approved_checks = Vec::new();
        for id in checks
            .into_iter()
            .filter(|id| profiles.iter().any(|p| &p.id == id))
        {
            // A declared input that is missing makes the preview an error.
            match crate::workbench::research::preview_host_execution(&s, &id) {
                Ok(preview) if preview.authorized => {
                    approved_checks.push((id, preview.fingerprint))
                }
                _ => {}
            }
        }
        let mut approved_plans = Vec::new();
        for id in plans {
            match crate::workbench::research::execution_plan::status(&s, &workspace, &id) {
                Ok(status) if status.authorized => {
                    approved_plans.push((id, status.record.content_hash))
                }
                _ => {}
            }
        }
        Ok((approved_checks, approved_plans))
    })
    .await
    .map_err(|e| e.message)?;
    let mut refused: Vec<_> = scope.checks.keys().cloned().collect();
    refused.extend(scope.captured_checks.keys().cloned());
    refused.retain(|id| {
        !approved_checks
            .iter()
            .chain(&approved_plans)
            .any(|(approved, _)| approved == id)
    });
    scope.checks.extend(approved_checks);
    scope.captured_checks.extend(approved_plans);
    Ok(refused)
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
        permission_profile: scope.permission_profile.clone(),
        preset_id: scope.preset_id.clone(),
        access: scope.access.clone(),
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

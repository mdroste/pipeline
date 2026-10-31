//! One cancellation and publication envelope for all action bodies.
use super::{
    artifact_path, atomic_json, normalize_review, snapshot, task_binding, terminal_failure,
};
use crate::orchestration::{
    definition::{Action, Binding},
    state::Ready,
    store::{err, Result, Store, TaskRun},
};
use crate::workbench::{commands::run_store, tasks};
use serde_json::{json, Value};

fn resolve(binding: &Binding, run: &TaskRun, ready: &Ready) -> Result<Value> {
    binding
        .resolve(&run.progress.outputs, &ready.inputs)
        .ok_or_else(|| format!("{}: required output is unavailable", ready.step.label))
}

/// Completed adapter results are journaled before the coordinator consumes them.
pub async fn execute(
    app: Option<tauri::AppHandle>,
    store: Store,
    run: TaskRun,
    ready: Ready,
    operation: String,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<Value> {
    if *stop.borrow() {
        return Err("Task stopped before dispatch".into());
    }
    let journal = store
        .root
        .join("actions")
        .join(&operation)
        .join("result.json");
    if journal.exists() {
        let bytes = std::fs::read(&journal).map_err(err)?;
        return serde_json::from_slice(&bytes).map_err(err);
    }
    #[cfg(all(feature = "e2e", debug_assertions))]
    if matches!(ready.step.action, Action::Workspace { .. }) {
        if let Some(result) =
            crate::orchestration::discovery::qualification::scripted(&store, &run)?
        {
            atomic_json(&journal, &result)?;
            return Ok(result);
        }
        if let Some(result) = crate::orchestration::missions::qualification::scripted(&store, &run)?
        {
            atomic_json(&journal, &result)?;
            return Ok(result);
        }
    }
    let result = match &ready.step.action {
        Action::LiteratureLookup { query } => {
            crate::workbench::discovery::acquire(
                run.scope
                    .workspace_id
                    .clone()
                    .ok_or("Literature lookup needs a project")?,
                query.clone(),
                operation.clone(),
            )
            .await?
        }
        Action::Snapshot {
            input,
            filename,
            require_change,
        } => {
            let value = resolve(input, &run, &ready)?;
            let local = store.clone();
            let scope = run.scope.clone();
            let filename = filename.clone();
            let result =
                tokio::task::spawn_blocking(move || snapshot(&local, &scope, value, &filename))
                    .await
                    .map_err(err)??;
            if *require_change
                && run.progress.outputs.as_object().is_some_and(|o| {
                    o.values()
                        .any(|v| v.get("hash").is_some() && v["hash"] == result["hash"])
                })
            {
                return Err(
                    "The revised artifact is unchanged. Inspect the revision before retrying."
                        .into(),
                );
            }
            result
        }
        Action::Workspace {
            prompt,
            model,
            effort,
        } => {
            let text = crate::orchestration::definition::render_prompt(
                prompt,
                &run.progress.outputs,
                &ready.inputs,
            )?;
            let binding = task_binding(&run.scope)?;
            let session = binding.session_id.clone();
            let submission = format!("task-{operation}");
            let previous_session = session.clone();
            let previous_op = submission.clone();
            let previous = run_store(move |s| tasks::outcome(&s, &previous_session, &previous_op))
                .await
                .map_err(|e| e.message)?;
            let (thread, turn) = if let Some(previous) = previous {
                if previous["state"] == "completed" || terminal_failure(&previous) {
                    atomic_json(&journal, &previous)?;
                    return Ok(previous);
                }
                (
                    previous["threadId"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    previous["turnId"].as_str().unwrap_or_default().to_string(),
                )
            } else {
                let sent = crate::workbench::commands::submit_turn(
                    app.clone()
                        .ok_or("Workspace execution requires the desktop runtime")?,
                    crate::workbench::commands::SendTurnRequest {
                        session_id: session.clone(),
                        text,
                        client_submission_id: submission.clone(),
                        model: model.clone(),
                        effort: effort.clone(),
                    },
                    Some(binding),
                    crate::workbench::submission::Control(Some(stop.clone())),
                )
                .await
                .map_err(|e| {
                    serde_json::to_value(e)
                        .ok()
                        .and_then(|v| v["message"].as_str().map(str::to_string))
                        .unwrap_or_else(|| "Workspace submission outcome is unknown".into())
                })?;
                (sent.thread_id, sent.turn_id)
            };
            atomic_json(
                &store
                    .root
                    .join("actions")
                    .join(&operation)
                    .join("child.json"),
                &json!({"kind":"workspace","sessionId":session,"submissionId":submission,"threadId":thread,"turnId":turn}),
            )?;
            loop {
                if *stop.borrow() {
                    let _ = crate::workbench::commands::workbench_codex_interrupt_turn(
                        thread.clone(),
                        turn.clone(),
                    )
                    .await;
                    return Err("Task stopped; Workspace turn interrupted".into());
                }
                let ss = session.clone();
                let op = submission.clone();
                if let Some(value) = run_store(move |s| tasks::outcome(&s, &ss, &op))
                    .await
                    .map_err(|e| e.message)?
                {
                    if matches!(
                        value["state"].as_str(),
                        Some("completed" | "failed" | "interrupted" | "cancelled")
                    ) {
                        break value;
                    }
                }
                if let Some(question) =
                    crate::workbench::commands::task_pending_request(&thread, &turn)
                {
                    let _ = crate::workbench::commands::workbench_codex_interrupt_turn(
                        thread.clone(),
                        turn.clone(),
                    )
                    .await;
                    return Err(format!("Workspace needs your input: {question}. The turn was interrupted; answer in the conversation, then refresh task context."));
                }
                tokio::select! {_=tokio::time::sleep(std::time::Duration::from_millis(500))=>{},_=stop.changed()=>{}}
            }
        }
        Action::Review {
            profile_id,
            input,
            interpretation,
            variables,
        } => {
            let artifact = resolve(input, &run, &ready)?;
            let path = artifact_path(&store, &artifact)?;
            atomic_json(
                &store
                    .root
                    .join("actions")
                    .join(&operation)
                    .join("review-input.json"),
                &artifact,
            )?;
            let pinned = run
                .scope
                .profiles
                .get(profile_id)
                .cloned()
                .ok_or("Review profile was not prepared")?;
            let future = crate::commands::orchestration::execute(
                operation.clone(),
                pinned,
                path,
                interpretation.clone(),
                variables.clone(),
                crate::emit::background(&crate::emit::from_app(
                    app.clone()
                        .ok_or("Review execution requires the desktop runtime")?,
                )),
                stop.clone(),
            );
            tokio::pin!(future);
            let value = tokio::select! {v=&mut future=>v?,_=stop.changed()=>{crate::commands::orchestration::cancel(&operation);let _=future.await;return Err("Task review stopped".into());}};
            atomic_json(
                &store
                    .root
                    .join("actions")
                    .join(&operation)
                    .join("review-raw.json"),
                &value,
            )?;
            normalize_review(value, artifact)?
        }
        Action::Check { .. } | Action::CapturedCheck { .. } => {
            let workspace = run
                .scope
                .workspace_id
                .clone()
                .ok_or("Checks require a Workspace project")?;
            let (profile, plan, expected) = match &ready.step.action {
                Action::Check { profile_id } => (
                    profile_id.clone(),
                    None,
                    run.scope
                        .checks
                        .get(profile_id)
                        .cloned()
                        .ok_or("Check was not prepared")?,
                ),
                Action::CapturedCheck { plan_id } => (
                    String::new(),
                    Some(plan_id.clone()),
                    run.scope
                        .captured_checks
                        .get(plan_id)
                        .cloned()
                        .ok_or("Captured check was not prepared")?,
                ),
                _ => unreachable!(),
            };
            let session = run.scope.session_id.clone();
            let op = operation.clone();
            let receipt=run_store(move|s| {
                if let Some(plan_id) = plan {
                    let status = crate::workbench::research::execution_plan::status(&s,&workspace,&plan_id)?;
                    if !status.authorized || status.record.content_hash != expected { return Err(crate::workbench::store::WorkbenchError::conflict("Captured check inputs or authorization changed")); }
                    let captured: crate::workbench::research::execution_plan::ExecutionPlan = serde_json::from_value(status.record.body).map_err(|e|crate::workbench::store::WorkbenchError::invalid(e.to_string()))?;
                    return crate::workbench::research::jobs::queue(&s,crate::workbench::research::RunExecutionRequest{plan_id:Some(plan_id),profile_id:captured.profile.id,session_id:session,test_only:true,operation_id:op},"detached",None);
                }
                if !crate::workbench::research::list_execution_profiles(&s,&workspace)?.iter().any(|p|p.id==profile) {return Err(crate::workbench::store::WorkbenchError::invalid("Check belongs to another project"));}
                let preview=crate::workbench::research::preview_host_execution(&s,&profile)?;
                if preview.fingerprint!=expected||!preview.authorized{return Err(crate::workbench::store::WorkbenchError::conflict("Check inputs or authorization changed. Reauthorize this check before continuing."));}
                crate::workbench::research::jobs::queue(&s,crate::workbench::research::RunExecutionRequest{plan_id:None,profile_id:profile,session_id:session,test_only:false,operation_id:op},"detached",None)
            }).await.map_err(|e|e.message)?;
            crate::workbench::research::jobs::launch_pending();
            let execution = receipt.id.clone();
            atomic_json(
                &store
                    .root
                    .join("actions")
                    .join(&operation)
                    .join("child.json"),
                &json!({"kind":"check","executionId":execution}),
            )?;
            loop {
                if *stop.borrow() {
                    let ex = execution.clone();
                    let _ = run_store(move |s| {
                        crate::workbench::research::cancel_execution(
                            &s,
                            crate::workbench::research::CancelExecutionRequest { execution_id: ex },
                        )
                    })
                    .await;
                    return Err("Task check stopped".into());
                }
                let ex = execution.clone();
                let value = run_store(move |s| crate::workbench::research::get_execution(&s, &ex))
                    .await
                    .map_err(|e| e.message)?;
                if !["queued", "running"].contains(&value.outcome.as_str()) {
                    break json!({"passed":value.outcome=="completed","receipt":value});
                }
                tokio::select! {_=tokio::time::sleep(std::time::Duration::from_millis(500))=>{},_=stop.changed()=>{}}
            }
        }
        Action::Deliver { input } => {
            let value = resolve(input, &run, &ready)?;
            crate::workbench::commands::deliver_task(
                task_binding(&run.scope)?,
                operation.clone(),
                value,
            )
            .await?
        }
        _ => return Err("Control steps must execute in the coordinator".into()),
    };
    if result.to_string().len() > 1024 * 1024 {
        return Err("Task action output exceeds 1 MiB; select a smaller result".into());
    }
    atomic_json(&journal, &result)?;
    Ok(result)
}

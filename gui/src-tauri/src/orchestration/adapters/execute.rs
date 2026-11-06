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

/// A reply at the turn limit, held twice for its two output fields and escaped
/// as JSON, fits, so only a result that no turn can produce is refused here.
const MAX_ACTION_OUTPUT: usize = 3 * 1024 * 1024;

/// How long a turn may have no live owner before it counts as lost. A finished
/// turn gives up its owner just before its ending reaches the store.
const LOST_TURN_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// Closes what the runtime left open for this conversation. API and Claude turns
/// settle in process; a Codex turn needs the desktop to reach App Server.
pub(in crate::orchestration) async fn settle_turn(app: Option<tauri::AppHandle>, session: String) {
    let local = crate::workbench::commands::reconcile_local_session(session.clone()).await;
    if let (Ok(false), Some(app)) = (local, app) {
        let _ = crate::workbench::commands::workbench_codex_reconcile_session(app, session).await;
    }
}

async fn read_outcome(session: &str, submission: &str) -> Result<Option<Value>> {
    let (session, submission) = (session.to_string(), submission.to_string());
    let value = run_store(move |s| tasks::outcome(&s, &session, &submission))
        .await
        .map_err(|e| e.message)?;
    Ok(value.filter(|v| {
        matches!(
            v["state"].as_str(),
            Some("completed" | "failed" | "interrupted" | "cancelled")
        )
    }))
}

/// Nobody can answer a request during a step, and interrupting the turn ends the
/// request with it. What is left is to run the step again. Research automation
/// roles decline such requests themselves, so this names the plain controls.
fn unanswered_request(question: &str) -> String {
    format!(
        "The assistant asked for a decision during this step, and the turn was stopped: {question}. That request can no longer be answered. To run the step again, choose Retry unfinished actions. To give your answer first, reply in the conversation, then choose Use current conversation context and Retry unfinished actions."
    )
}

/// Waits for a submitted turn to end. A connection lost mid-turn releases the
/// turn's owner and nothing else, so the store would read as running until the
/// time limit, with this action holding the conversation slot. Once no owner
/// has come back, the turn is settled and takes the ending its runtime recorded.
pub(in crate::orchestration) async fn await_turn(
    app: &Option<tauri::AppHandle>,
    session: &str,
    submission: &str,
    thread: &str,
    turn: &str,
    stop: &mut tokio::sync::watch::Receiver<bool>,
    grace: std::time::Duration,
) -> Result<Value> {
    use crate::workbench::commands as workspace;
    let mut unowned_since = None;
    loop {
        if *stop.borrow() {
            let _ = workspace::interrupt_runtime_turn(thread.to_string(), turn.to_string()).await;
            return Err("Task stopped; the conversation turn was interrupted".into());
        }
        if let Some(value) = read_outcome(session, submission).await? {
            return Ok(value);
        }
        if let Some(question) = workspace::task_pending_request(thread, turn) {
            let _ = workspace::interrupt_runtime_turn(thread.to_string(), turn.to_string()).await;
            return Err(unanswered_request(&question));
        }
        if workspace::turn_has_live_owner(thread, turn) || workspace::turn_setup_in_progress() {
            unowned_since = None;
        } else if unowned_since
            .get_or_insert_with(std::time::Instant::now)
            .elapsed()
            >= grace
        {
            tokio::select! {
                _ = settle_turn(app.clone(), session.to_string()) => {}
                _ = stop.changed() => {}
            }
            return read_outcome(session, submission).await?.ok_or_else(|| {
                "The connection ended during this step, and how the turn finished is unknown. Check saved results, or retry the step.".to_string()
            });
        }
        tokio::select! {_=tokio::time::sleep(std::time::Duration::from_millis(500))=>{},_=stop.changed()=>{}}
    }
}

/// Completed adapter results are journaled before the coordinator consumes them.
pub async fn execute(
    app: Option<tauri::AppHandle>,
    store: Store,
    run: TaskRun,
    ready: Ready,
    operation: String,
    stop: tokio::sync::watch::Receiver<bool>,
) -> Result<Value> {
    let meter = super::super::allowance::meter(&store, &run, &ready, &operation)?;
    crate::usage_allowance::CURRENT
        .scope(
            Some(meter),
            execute_inner(app, store, run, ready, operation, stop),
        )
        .await
}

async fn execute_inner(
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
    let mark = || {
        super::contract::mark(
            &store,
            &operation,
            super::contract::Kind::of(&ready.step.action),
        )
    };
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
            mark()?;
            tokio::select! { result=crate::workbench::discovery::acquire(
                run.scope
                    .workspace_id
                    .clone()
                    .ok_or("Literature lookup needs a project")?,
                query.clone(),
                operation.clone(),
            ) => result?, _ = stop.changed() => return Err("Literature lookup stopped".into()) }
        }
        Action::Snapshot {
            input,
            filename,
            require_change,
        } => {
            let replacement = run
                .progress
                .corrected_snapshots
                .get(&ready.address)
                .cloned();
            let value = resolve(input, &run, &ready)?;
            let local = store.clone();
            let scope = run.scope.clone();
            let filename = filename.clone();
            mark()?;
            let result = tokio::task::spawn_blocking(move || {
                if let Some(corrected) = replacement {
                    artifact_path(&local, &corrected)?;
                    Ok(corrected)
                } else {
                    snapshot(&local, &scope, value, &filename)
                }
            })
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
            atomic_json(&journal.with_file_name("snapshot.json"), &result)?;
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
            if let Some(count) =
                crate::orchestration::discovery::storage::idea_team(&store, &run.id)?
            {
                super::contract::mark_team(&store, &operation)?;
                for _ in 0..=count {
                    crate::usage_allowance::Hold::reserve(
                        crate::usage_allowance::current().as_ref(),
                        crate::usage_allowance::Class::Subscription,
                        "",
                        0,
                        0,
                    )?;
                }
                let result = super::idea_team::run(
                    store
                        .root
                        .join("actions")
                        .join(&operation)
                        .join("progress.json"),
                    binding,
                    text,
                    model.clone(),
                    effort.clone(),
                    count,
                    stop.clone(),
                )
                .await?;
                atomic_json(&journal.with_file_name("team-result.json"), &result)?;
                atomic_json(&journal, &result)?;
                return Ok(result);
            }
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
                mark()?;
                let sent = crate::workbench::commands::submit_turn(
                    app.clone(),
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
            await_turn(
                &app,
                &session,
                &submission,
                &thread,
                &turn,
                &mut stop,
                LOST_TURN_GRACE,
            )
            .await?
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
            mark()?;
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
            mark()?;
            let receipt=run_store(move|s| {
                if let Some(plan_id) = plan {
                    let status = crate::workbench::research::execution_plan::status(&s,&workspace,&plan_id)?;
                    if !status.authorized || status.record.content_hash != expected { return Err(crate::workbench::store::WorkbenchError::conflict("This captured check changed or lost its approval. Approve it in the project's Results view, then retry.")); }
                    let captured: crate::workbench::research::execution_plan::ExecutionPlan = serde_json::from_value(status.record.body).map_err(|e|crate::workbench::store::WorkbenchError::invalid(e.to_string()))?;
                    return crate::workbench::research::jobs::queue(&s,crate::workbench::research::RunExecutionRequest{plan_id:Some(plan_id),profile_id:captured.profile.id,session_id:session,test_only:true,operation_id:op},"detached",None);
                }
                if !crate::workbench::research::list_execution_profiles(&s,&workspace)?.iter().any(|p|p.id==profile) {return Err(crate::workbench::store::WorkbenchError::invalid("Check belongs to another project"));}
                let preview=crate::workbench::research::preview_host_execution(&s,&profile)?;
                if preview.fingerprint!=expected||!preview.authorized{return Err(crate::workbench::store::WorkbenchError::conflict("This check's inputs or approval changed. Approve it again in the project's Results view, then retry."));}
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
            let source = store.clone();
            let binding = task_binding(&run.scope)?;
            let input = input.clone();
            let (value, files) = tokio::task::spawn_blocking(move || {
                super::delivery::action_files(&source, &run, &input, &value)
                    .map(|files| (value, files))
            })
            .await
            .map_err(err)??;
            if *stop.borrow() {
                return Err("Task stopped before delivery".into());
            }
            mark()?;
            crate::workbench::commands::deliver_task(binding, operation.clone(), value, files)
                .await?
        }
        _ => return Err("Control steps must execute in the coordinator".into()),
    };
    if result.to_string().len() > MAX_ACTION_OUTPUT {
        return Err("This step's output exceeds 3 MiB; select a smaller result".into());
    }
    atomic_json(&journal, &result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::unanswered_request;

    #[test]
    fn a_stopped_request_offers_only_controls_that_exist() {
        let message = unanswered_request("Run pdftotext?");
        let controls = include_str!("../../../../src/components/tasks/Detail.tsx");
        for label in [
            "Retry unfinished actions",
            "Use current conversation context",
        ] {
            assert!(message.contains(label), "{message}");
            assert!(controls.contains(label), "{label}");
        }
        assert!(message.contains("Run pdftotext?"));
        assert!(!message.contains("refresh task context"));
    }
}

use super::*;
use definition::Chain;
use serde::Deserialize;
use triggers::Trigger;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareTask {
    pub chain: Chain,
    pub session_id: Option<String>,
    pub inputs: Value,
    pub operation_id: String,
    pub trigger: Trigger,
}
#[tauri::command]
pub async fn task_prepare(app: tauri::AppHandle, request: PrepareTask) -> Result<TaskRun> {
    request.trigger.validate()?;
    let scope = adapters::prepare(&request.chain, request.session_id, &request.inputs).await?;
    let m = manager(app).await?;
    let _gate = m.gate.lock().await;
    let due = match request.trigger {
        Trigger::Now => now(),
        Trigger::Once { at } => at,
        _ => request
            .trigger
            .next(now() - 1)?
            .ok_or("Schedule has no upcoming occurrence")?,
    };
    let mut inputs = request.inputs;
    inputs["__trigger"] = serde_json::to_value(request.trigger).map_err(err)?;
    let value = m
        .db(move |s| {
            s.create(
                &request.operation_id,
                request.chain,
                scope,
                inputs,
                due,
                false,
            )
        })
        .await?;
    m.notify();
    Ok(value)
}
#[tauri::command]
pub async fn task_list(
    app: tauri::AppHandle,
    view: Option<String>,
    session_id: Option<String>,
    offset: Option<u32>,
) -> Result<Vec<Summary>> {
    let m = manager(app).await?;
    m.db(move |s| {
        s.list(
            view.as_deref().unwrap_or("active"),
            session_id.as_deref(),
            offset.unwrap_or(0),
        )
    })
    .await
}
#[tauri::command]
pub async fn task_get(app: tauri::AppHandle, id: String) -> Result<TaskRun> {
    manager(app).await?.db(move |s| s.get(&id)).await
}
#[tauri::command]
pub async fn task_events(
    app: tauri::AppHandle,
    id: String,
    after: Option<i64>,
) -> Result<Vec<Event>> {
    manager(app)
        .await?
        .db(move |s| s.events(&id, after.unwrap_or(0)))
        .await
}
#[tauri::command]
pub async fn task_control(
    app: tauri::AppHandle,
    id: String,
    revision: i64,
    action: String,
) -> Result<TaskRun> {
    let m = manager(app).await?;
    let _gate = m.gate.lock().await;
    let child_id = id.clone();
    if m.db(move |s| super::missions::storage::owner(&s, &child_id))
        .await?
        .is_some()
    {
        return Err("Use the owning mission's controls for this managed task".into());
    }
    let mut run = m.db(move |s| s.get(&id)).await?;
    if revision != run.revision {
        return Err("Task changed; reload before continuing".into());
    }
    match action.as_str() {
        "start" if run.state == "draft" => {
            let trigger: Trigger =
                serde_json::from_value(run.inputs["__trigger"].clone()).map_err(err)?;
            if trigger.recurring() {
                let mut copy = run.clone();
                run = m
                    .db(move |s| {
                        s.activate_schedule(&mut copy, trigger)?;
                        Ok(copy)
                    })
                    .await?;
                m.notify();
                return Ok(run);
            } else {
                run.state = "queued".into();
                if matches!(trigger, Trigger::Now) {
                    run.due_at = Some(now());
                    run.deadline_at = now() + run.chain.limits.deadline_hours as i64 * 3600;
                }
            }
        }
        "pause" if matches!(run.state.as_str(), "queued" | "running" | "waiting") => {
            run.state = "paused".into();
            run.reason = Some("Paused; active work may finish".into());
        }
        "stop" if !matches!(run.state.as_str(), "finished" | "cancelled") => {
            m.stop_children(&run.id);
            run.state = if run.progress.receipts.values().any(|r| r.state == "running") {
                "cancelling"
            } else {
                "cancelled"
            }
            .into();
            run.due_at = None;
            run.reason = Some("Stopped by you".into());
        }
        "resume" if run.state == "paused" => {
            run.state = "queued".into();
            run.due_at = Some(now());
            run.reason = None;
        }
        "reconcile" if run.state == "attention" => {
            m.recover_results(&mut run).await?;
            if !run.progress.receipts.values().any(|r| r.state == "unknown") {
                run.state = "queued".into();
                run.due_at = Some(now());
                run.reason = None;
            }
        }
        "refreshContext" if run.state == "attention" || run.state == "paused" => {
            let session = run
                .scope
                .session_id
                .clone()
                .ok_or("Task has no conversation")?;
            let fresh = crate::workbench::commands::run_store(move |s| {
                crate::workbench::tasks::binding(&s, &session)
            })
            .await
            .map_err(|e| e.message)?;
            if fresh.workspace_id != run.scope.workspace_id {
                return Err(
                    "The conversation moved to another project. Prepare a new task for that scope."
                        .into(),
                );
            }
            run.scope.session_cursor = Some(fresh.cursor);
            run.scope.harness_fingerprint = Some(fresh.harness_fingerprint);
            run.scope.runtime_root = Some(fresh.runtime_root);
            run.scope.root_identity = Some(fresh.root_identity);
            run.reason = Some(
                "Context refreshed. Reconcile or explicitly retry an interrupted action.".into(),
            );
        }
        "retry" if run.state == "attention" => {
            let next = m
                .db(move |s| {
                    s.retry(&mut run)?;
                    Ok(run)
                })
                .await?;
            m.notify();
            return Ok(next);
        }
        _ => return Err("This action is unavailable in the current task state".into()),
    }
    m.save(&mut run, &action, "Task updated").await?;
    Ok(run)
}
#[tauri::command]
pub async fn task_input(
    app: tauri::AppHandle,
    id: String,
    address: String,
    operation_id: String,
    value: Value,
) -> Result<TaskRun> {
    let m = manager(app).await?;
    let _gate = m.gate.lock().await;
    let run = m
        .db(move |s| s.signal(&id, &address, &operation_id, value))
        .await?;
    m.notify();
    Ok(run)
}
#[tauri::command]
pub async fn task_schedules(app: tauri::AppHandle) -> Result<Vec<Schedule>> {
    manager(app).await?.db(|s| s.schedules()).await
}
#[tauri::command]
pub async fn task_update_schedule(
    app: tauri::AppHandle,
    id: String,
    revision: i64,
    enabled: bool,
    trigger: Option<Trigger>,
) -> Result<Schedule> {
    let m = manager(app).await?;
    let _gate = m.gate.lock().await;
    let result = m
        .db(move |s| {
            let mut schedule = s
                .schedules()?
                .into_iter()
                .find(|v| v.id == id)
                .ok_or("Schedule not found")?;
            schedule.enabled = enabled;
            if let Some(trigger) = trigger {
                schedule.trigger = trigger;
            }
            schedule.next_due_at = schedule.trigger.next(now() - 1)?;
            s.save_schedule(schedule, Some(revision))
        })
        .await?;
    m.notify();
    Ok(result)
}
#[tauri::command]
pub fn task_preview_times(trigger: Trigger, after: i64) -> Result<Vec<i64>> {
    let mut cursor = after;
    let mut times = Vec::new();
    for _ in 0..3 {
        if let Some(t) = trigger.next(cursor)? {
            times.push(t);
            cursor = t;
        } else {
            break;
        }
    }
    Ok(times)
}
#[tauri::command]
pub async fn task_saved_chains(app: tauri::AppHandle) -> Result<Vec<Value>> {
    manager(app).await?.db(|s| s.chains()).await
}
#[tauri::command]
pub async fn task_save_chain(app: tauri::AppHandle, chain: Chain) -> Result<String> {
    manager(app).await?.db(move |s| s.save_chain(&chain)).await
}
#[tauri::command]
pub fn task_validate_chain(json: String) -> Result<Chain> {
    if json.len() > 256 * 1024 {
        return Err("Chain exceeds 256 KiB".into());
    }
    let chain: Chain = serde_json::from_str(&json).map_err(err)?;
    definition::validate(&chain)?;
    Ok(chain)
}
#[tauri::command]
pub async fn task_background(app: tauri::AppHandle, enabled: Option<bool>) -> Result<bool> {
    let m = manager(app).await?;
    if let Some(value) = enabled {
        if value {
            super::background::install(m.app.as_ref().ok_or("The desktop is unavailable")?)?;
        }
        m.db(move |s| s.set_preference("background", &json!(value)))
            .await?;
        m.background.store(value, Ordering::Release);
    }
    Ok(m.background.load(Ordering::Acquire))
}

#[tauri::command]
pub async fn task_sessions() -> Result<Vec<Value>> {
    crate::workbench::commands::run_store(|s| crate::workbench::tasks::session_choices(&s))
        .await
        .map_err(|e| e.message)
}
#[tauri::command]
pub async fn task_proposals(session_id: String) -> Result<Vec<Value>> {
    crate::workbench::commands::run_store(move |s| {
        crate::workbench::tasks::proposals(&s, &session_id)
    })
    .await
    .map_err(|e| e.message)
}
#[tauri::command]
pub async fn task_prepare_proposal(
    app: tauri::AppHandle,
    session_id: String,
    proposal_id: String,
) -> Result<TaskRun> {
    let session = session_id.clone();
    let id = proposal_id.clone();
    let proposal = crate::workbench::commands::run_store(move |s| {
        crate::workbench::tasks::proposal(&s, &session, &id)
    })
    .await
    .map_err(|e| e.message)?;
    let result = task_prepare(
        app,
        PrepareTask {
            chain: serde_json::from_value(proposal["chain"].clone()).map_err(err)?,
            session_id: Some(session_id.clone()),
            inputs: proposal["inputs"].clone(),
            trigger: serde_json::from_value(proposal["trigger"].clone()).map_err(err)?,
            operation_id: proposal_id.clone(),
        },
    )
    .await?;
    let task = result.id.clone();
    crate::workbench::commands::run_store(move |s| {
        crate::workbench::tasks::resolve_proposal(&s, &session_id, &proposal_id, &task)
    })
    .await
    .map_err(|e| e.message)?;
    Ok(result)
}
#[tauri::command]
pub async fn task_export_chain(app: tauri::AppHandle, chain: Chain) -> Result<Option<String>> {
    definition::validate(&chain)?;
    crate::commands::save_via_dialog(
        &app,
        &format!("{}.chain.json", chain.name),
        "Pipeline chain",
        "json",
        serde_json::to_vec_pretty(&chain).map_err(err)?,
    )
    .await
}

#[tauri::command]
pub async fn task_step_output(
    app: tauri::AppHandle,
    id: String,
    address: String,
    operation: Option<String>,
) -> Result<Value> {
    manager(app)
        .await?
        .db(move |s| s.step_output(&id, &address, operation.as_deref()))
        .await
}
#[tauri::command]
pub async fn task_export_artifact(
    app: tauri::AppHandle,
    id: String,
    address: String,
    operation: Option<String>,
) -> Result<Option<String>> {
    let m = manager(app.clone()).await?;
    let (filename, bytes) = m
        .db(move |s| {
            let output = s.step_output(&id, &address, operation.as_deref())?;
            let mut value = &output;
            if let Some(result) = value.get("result") {
                value = result;
            }
            if let Some(artifact) = value.get("reviewedArtifact") {
                value = artifact;
            }
            let path = adapters::artifact_path(&s, value)?;
            let filename = value["filename"].as_str().unwrap_or("paper.md").to_string();
            if value["kind"] == "artifactTree" {
                use std::io::Write;
                let files = crate::workbench::project::task_capture(
                    std::path::Path::new(&path),
                    std::path::Path::new(&path),
                )
                .map_err(|e| e.message)?;
                let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
                for (name, bytes) in files {
                    zip.start_file(name, zip::write::SimpleFileOptions::default())
                        .map_err(err)?;
                    zip.write_all(&bytes).map_err(err)?;
                }
                Ok((
                    format!("{filename}.zip"),
                    zip.finish().map_err(err)?.into_inner(),
                ))
            } else {
                Ok((filename, std::fs::read(path).map_err(err)?))
            }
        })
        .await?;
    let extension = std::path::Path::new(&filename)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("md")
        .to_string();
    crate::commands::save_via_dialog(&app, &filename, "Task artifact", &extension, bytes).await
}

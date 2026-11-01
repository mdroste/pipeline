use super::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prepare {
    pub definition: Definition,
    pub session_id: String,
    pub operation_id: String,
}
#[tauri::command]
pub async fn mission_prepare(app: tauri::AppHandle, request: Prepare) -> Result<Mission> {
    validate::definition(&request.definition)?;
    validate::text(&request.operation_id, 128)?;
    let coordinator = super::super::manager(app).await?;
    let _gate = coordinator.gate.lock().await;
    let fingerprint = store::hash(&(&request.definition, &request.session_id))?;
    let op = request.operation_id.clone();
    let fp = fingerprint.clone();
    if let Some(m) = coordinator
        .db(move |s| storage::previous(&s, &op, &fp))
        .await?
    {
        return Ok(m);
    }
    let session = request.session_id.clone();
    let source = run_store(move |s| crate::workbench::tasks::binding(&s, &session))
        .await
        .map_err(|e| e.message)?;
    let ws = source
        .workspace_id
        .clone()
        .ok_or("Choose a project conversation")?;
    let policy = request.definition.policy.clone();
    let workspace_id = ws.clone();
    let capabilities = run_store(move |s| workspace::capabilities(&s, &workspace_id, &policy))
        .await
        .map_err(|e| e.message)?;
    let workspace_id = ws.clone();
    let available_methods = coordinator
        .db(move |s| storage::methods(&s, &workspace_id))
        .await?;
    let mut methods = Vec::new();
    for id in &request.definition.method_ids {
        methods.push(
            available_methods
                .iter()
                .find(|m| &m.id == id)
                .cloned()
                .ok_or("A selected method is no longer retained in this project")?,
        );
    }
    let pin = if let Some(profile) = request.definition.policy.review_profile_id.clone() {
        Some(
            tokio::task::spawn_blocking(move || crate::commands::orchestration::pin(&profile))
                .await
                .map_err(store::err)??,
        )
    } else {
        None
    };
    let operation = store::hash(&request.operation_id)?[..32].to_string();
    let name = request.definition.name.clone();
    let policy = request.definition.policy.clone();
    let (mut scopes, authority_fingerprints, context) =
        run_store(move |s| workspace::roles(&s, &source, &operation, &name, &policy))
            .await
            .map_err(|e| e.message)?;
    let mut scope = scopes.remove(0);
    let planner_scope = scopes.remove(0);
    let challenger_scope = scopes.remove(0);
    for c in &capabilities {
        if c.kind == "check" {
            scope.checks.insert(c.id.clone(), c.fingerprint.clone());
        } else {
            scope
                .captured_checks
                .insert(c.id.clone(), c.fingerprint.clone());
        }
    }
    if let (Some(id), Some(pin)) = (&request.definition.policy.review_profile_id, pin) {
        scope.profiles.insert(id.clone(), pin);
    }
    let workspace_id = ws.clone();
    let watch_cursor = run_store(move |s| {
        s.connection()?
            .query_row(
                "SELECT COALESCE(MAX(rowid),0) FROM research_attention WHERE workspace_id=?1",
                [workspace_id],
                |r| r.get(0),
            )
            .map_err(|e| {
                crate::workbench::store::WorkbenchError::storage(
                    "Research automation watch baseline",
                    e,
                )
            })
    })
    .await
    .map_err(|e| e.message)?;
    let workspace_id = ws.clone();
    let monitors = request.definition.policy.monitor_ids.clone();
    let watch_snapshots =
        run_store(move |s| workspace::watch_snapshots(&s, &workspace_id, &monitors))
            .await
            .map_err(|e| e.message)?;
    let time = now();
    let mut mission = Mission {
        id: store::id(),
        revision: 0,
        definition: request.definition,
        state: "draft".into(),
        reason:
            "Review this automation's remit, role conversations, access and limits before starting"
                .into(),
        source_session_id: request.session_id,
        workspace_id: ws,
        scope,
        planner_scope,
        challenger_scope,
        authority_fingerprints,
        capabilities,
        context,
        goals: Vec::new(),
        rounds: Vec::new(),
        questions: Vec::new(),
        methods: Vec::new(),
        selected_methods: methods,
        active_child: None,
        child_ids: Vec::new(),
        stop_outcome: None,
        phase: Phase::Plan,
        actions_reserved: 0,
        active_seconds: 0,
        stagnant_rounds: 0,
        created_at: time,
        updated_at: time,
        deadline_at: None,
        due_at: None,
        watch_cursor,
        watch_snapshots,
        watch_epoch: 0,
        changes: Vec::new(),
        brief: String::new(),
        attention_count: 0,
    };
    prompts::plan(&mission)?;
    mission.brief = brief::render(&mission);
    let m = coordinator
        .db(move |s| storage::create(&s, mission, &request.operation_id, &fingerprint))
        .await?;
    notify(&coordinator, &m, "");
    Ok(m)
}
#[tauri::command]
pub async fn mission_choices(session_id: String) -> Result<Value> {
    run_store(move |s| workspace::choices(&s, &session_id))
        .await
        .map_err(|e| e.message)
}
#[tauri::command]
pub async fn mission_list(
    app: tauri::AppHandle,
    workspace_id: Option<String>,
    offset: Option<u32>,
) -> Result<Vec<Summary>> {
    super::super::manager(app)
        .await?
        .db(move |s| storage::list(&s, workspace_id.as_deref(), offset.unwrap_or(0)))
        .await
}
#[tauri::command]
pub async fn mission_get(app: tauri::AppHandle, id: String) -> Result<Mission> {
    super::super::manager(app)
        .await?
        .db(move |s| storage::get(&s, &id))
        .await
}
#[tauri::command]
pub async fn mission_events(
    app: tauri::AppHandle,
    id: String,
    after: Option<i64>,
) -> Result<Vec<store::Event>> {
    super::super::manager(app)
        .await?
        .db(move |s| storage::events(&s, &id, after.unwrap_or(0)))
        .await
}
#[tauri::command]
pub async fn mission_methods(app: tauri::AppHandle, workspace_id: String) -> Result<Vec<Method>> {
    super::super::manager(app)
        .await?
        .db(move |s| storage::methods(&s, &workspace_id))
        .await
}

#[tauri::command]
pub async fn mission_control(
    app: tauri::AppHandle,
    id: String,
    revision: i64,
    action: String,
) -> Result<Mission> {
    let coordinator = super::super::manager(app).await?;
    let _gate = coordinator.gate.lock().await;
    let mut m = coordinator.db(move |s| storage::get(&s, &id)).await?;
    if m.revision != revision {
        return Err("This research automation changed; reload before continuing".into());
    }
    let previous = m.state.clone();
    match action.as_str() {
        "start" if m.state == "draft" => {
            // Validate all role permissions before granting any captured host plans.
            for scope in [&m.scope, &m.planner_scope, &m.challenger_scope] {
                let expected =
                    m.authority_fingerprints[scope.session_id.as_deref().unwrap_or("")].clone();
                let scope = scope.clone();
                run_store(move |s| workspace::refresh_scope(&s, &scope, &expected))
                    .await
                    .map_err(|e| e.message)?;
            }
            let ws = m.workspace_id.clone();
            let capabilities = m.capabilities.clone();
            run_store(move |s| workspace::authorize_experiments(&s, &ws, &capabilities))
                .await
                .map_err(|e| e.message)?;
            m.state = "queued".into();
            m.reason = "Research automation authorized; preparing the agenda".into();
            m.deadline_at = Some(now() + m.definition.budget.deadline_hours as i64 * 3600);
            m.due_at = Some(now());
        }
        "pause" if matches!(m.state.as_str(), "queued" | "running" | "waiting") => {
            if let Some(id) = m.active_child.clone() {
                coordinator
                    .db(move |s| {
                        let mut child = s.get(&id)?;
                        if child.state == "queued"
                            && !child
                                .progress
                                .receipts
                                .values()
                                .any(|r| r.state == "running")
                        {
                            child.state = "paused".into();
                            s.save(
                                &mut child,
                                "missionPause",
                                "Owning automation paused queued work",
                            )?;
                        }
                        Ok(())
                    })
                    .await?;
            }
            m.state = "paused".into();
            m.reason =
                "Paused; active work may finish, and no next investigation will start".into();
            m.due_at = m.active_child.as_ref().map(|_| now() + 2);
        }
        "resume" if m.state == "paused" => {
            if let Some(id) = m.active_child.clone() {
                coordinator
                    .db(move |s| {
                        let mut child = s.get(&id)?;
                        if child.state == "paused" {
                            child.state = "queued".into();
                            child.due_at = Some(now());
                            s.save(&mut child, "missionResume", "Owning automation resumed")?;
                        }
                        Ok(())
                    })
                    .await?;
                m.state = "running".into();
            } else {
                m.state = "queued".into();
            }
            m.reason = "Resuming within the remaining budget".into();
            m.due_at = Some(now());
        }
        "stop" if !m.terminal() && m.state != "stopping" => {
            m.reason =
                "Stopped by you; completed work and unresolved outcomes remain available".into();
            if let Some(id) = m.active_child.clone() {
                stop_child(&coordinator, &id).await?;
                m.state = "stopping".into();
                m.stop_outcome = Some("stopped".into());
                m.due_at = Some(now());
            } else {
                m.state = "stopped".into();
                m.due_at = None;
            }
        }
        "reconcile" if m.state == "attention" && m.active_child.is_some() => {
            reconcile(&coordinator, &mut m).await?;
        }
        "retry" if m.state == "attention" => retry(&coordinator, &mut m).await?,
        _ => return Err("This action is unavailable in the automation's current state".into()),
    }
    save(&coordinator, &mut m, &action).await?;
    notify(&coordinator, &m, &previous);
    Ok(m)
}

/// Settles and reads the interrupted action, replaying nothing. A result on
/// record is adopted through the action, as if it had just finished.
pub(super) async fn reconcile(coordinator: &Coordinator, m: &mut Mission) -> Result<()> {
    let id = m.active_child.clone().ok_or("No action to check")?;
    let child = coordinator.db(move |s| s.get(&id)).await?;
    let mut child = coordinator.recover_child(&child).await?;
    if child
        .progress
        .receipts
        .values()
        .any(|r| r.state == "failed")
    {
        return Err(
            "This step did not finish. Choose Retry interrupted action to run it again.".into(),
        );
    }
    if child
        .progress
        .receipts
        .values()
        .any(|r| r.state == "unknown")
    {
        return Err("No saved result was found for this step. Inspect the action, then choose Retry interrupted action.".into());
    }
    child.state = "queued".into();
    child.due_at = Some(now());
    coordinator
        .save(
            &mut child,
            "missionReconcile",
            "Adopted recorded action results",
        )
        .await?;
    let id = m.id.clone();
    *m = coordinator.db(move |s| storage::get(&s, &id)).await?;
    m.state = "running".into();
    m.reason = "Recorded results reconciled; validating the research response".into();
    m.due_at = Some(now());
    Ok(())
}

pub(super) async fn retry(coordinator: &Coordinator, m: &mut Mission) -> Result<()> {
    if exhausted(m) {
        return Err(
            "This automation has used its budget. Start a new research automation to do more work."
                .into(),
        );
    }
    if let Some(id) = m.active_child.clone() {
        let child = coordinator.db(move |s| s.get(&id)).await?;
        if child
            .progress
            .receipts
            .values()
            .any(|r| r.state == "running")
        {
            return Err("The previous step is still stopping. Try again in a moment.".into());
        }
        account(m, &child);
        // Explicit retry may adopt the researcher's intervening reply, but never
        // altered permissions, roots, selected context or execution grants.
        let target = match m.phase {
            Phase::Plan => &mut m.planner_scope,
            Phase::Challenge => &mut m.challenger_scope,
            _ => &mut m.scope,
        };
        let session = target
            .session_id
            .clone()
            .ok_or("This automation's conversation is missing")?;
        let expected = m.authority_fingerprints.get(&session).cloned().ok_or(
            "This automation's permission record is unavailable. Start a new research automation.",
        )?;
        let old = target.clone();
        *target = run_store(move |s| workspace::readopt_scope(&s, &old, &expected, true))
            .await
            .map_err(|e| e.message)?;
        m.active_child = None;
        m.state = "queued".into();
        m.due_at = Some(now());
        m.reason =
            "Retry authorized with current conversation input and the original resource limits"
                .into();
        let mut copy = m.clone();
        let child_id = child.id.clone();
        *m = coordinator
            .db(move |s| {
                storage::adopted(&s, &mut copy, &child_id)?;
                Ok(copy)
            })
            .await?;
    } else {
        // Attention without a child (stagnation, a scope failure during
        // dispatch): the planner conversation may have received a
        // steering message meanwhile, so re-adopt its cursor exactly as
        // the active-child branch does, or every Retry fails again.
        m.phase = Phase::Plan;
        let session = m
            .planner_scope
            .session_id
            .clone()
            .ok_or("This automation's conversation is missing")?;
        let expected = m.authority_fingerprints.get(&session).cloned().ok_or(
            "This automation's permission record is unavailable. Start a new research automation.",
        )?;
        let old = m.planner_scope.clone();
        m.planner_scope = run_store(move |s| workspace::readopt_scope(&s, &old, &expected, true))
            .await
            .map_err(|e| e.message)?;
        m.state = "queued".into();
        m.due_at = Some(now());
        m.reason = "Reconsidering the agenda within the remaining budget".into();
        m.stagnant_rounds = 0;
    }
    Ok(())
}

#[tauri::command]
pub async fn mission_answer(
    app: tauri::AppHandle,
    id: String,
    question_id: String,
    operation_id: String,
    answer: String,
) -> Result<Mission> {
    let coordinator = super::super::manager(app).await?;
    let _gate = coordinator.gate.lock().await;
    let mut m = coordinator.db(move |s| storage::get(&s, &id)).await?;
    let previous = m.state.clone();
    m = coordinator
        .db(move |s| {
            storage::answer(&s, &mut m, &question_id, &operation_id, &answer)?;
            Ok(m)
        })
        .await?;
    notify(&coordinator, &m, &previous);
    Ok(m)
}
#[tauri::command]
pub async fn mission_retain_method(
    app: tauri::AppHandle,
    id: String,
    revision: i64,
    method_id: String,
    retained: bool,
) -> Result<Mission> {
    let coordinator = super::super::manager(app).await?;
    let _gate = coordinator.gate.lock().await;
    let mut m = coordinator.db(move |s| storage::get(&s, &id)).await?;
    if m.revision != revision {
        return Err("This research automation changed; reload before retaining a method".into());
    }
    m.methods
        .iter_mut()
        .find(|method| method.id == method_id)
        .ok_or("Method was not found")?
        .retained = retained;
    save(&coordinator, &mut m, "method").await?;
    notify(&coordinator, &m, &m.state);
    Ok(m)
}
#[tauri::command]
pub async fn mission_export(
    app: tauri::AppHandle,
    id: String,
    format: String,
) -> Result<Option<String>> {
    let m = mission_get(app.clone(), id).await?;
    let (extension, bytes) = match format.as_str() {
        "markdown" => ("md", brief::render(&m).into_bytes()),
        "json" => ("json", serde_json::to_vec_pretty(&m).map_err(store::err)?),
        _ => return Err("Choose Markdown or JSON".into()),
    };
    crate::commands::save_via_dialog(
        &app,
        &format!("research-automation.{extension}"),
        "Research automation",
        extension,
        bytes,
    )
    .await
}
#[tauri::command]
pub async fn mission_evidence(
    app: tauri::AppHandle,
    id: String,
    evidence_id: String,
) -> Result<Value> {
    super::super::manager(app)
        .await?
        .db(move |s| evidence(&s, &id, &evidence_id))
        .await
}
pub(super) fn evidence(s: &store::Store, id: &str, evidence_id: &str) -> Result<Value> {
    let m = storage::get(s, id)?;
    let value = m
        .rounds
        .iter()
        .find_map(|r| r.evidence.get(evidence_id))
        .cloned()
        .ok_or("Evidence identity is not part of this research automation")?;
    if value["kind"] == "artifact" {
        let path = adapters::artifact_path(s, &value)?;
        use std::io::Read;
        let mut bytes = Vec::new();
        crate::safety::open_regular_file(std::path::Path::new(&path))?
            .take(EVIDENCE_PREVIEW as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(store::err)?;
        return Ok(
            json!({"record":value,"text":preview_text(&bytes),"truncated":bytes.len()>EVIDENCE_PREVIEW}),
        );
    }
    Ok(value)
}
const EVIDENCE_PREVIEW: usize = 128 * 1024;
/// The readable start of a file. The cut may fall inside a character, which
/// is no reason to withhold the text; a binary file has none to show.
fn preview_text(bytes: &[u8]) -> Option<String> {
    let cut = bytes.len() > EVIDENCE_PREVIEW;
    let shown = &bytes[..bytes.len().min(EVIDENCE_PREVIEW)];
    match std::str::from_utf8(shown) {
        Ok(text) => Some(text.into()),
        Err(error) if cut && error.error_len().is_none() => {
            String::from_utf8(shown[..error.valid_up_to()].to_vec()).ok()
        }
        Err(_) => None,
    }
}

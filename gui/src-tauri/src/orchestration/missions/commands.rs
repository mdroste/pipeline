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
    let watch_cursor = run_store(move |s| workspace::watch_baseline(&s, &workspace_id))
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
    lookahead: Option<bool>,
) -> Result<Vec<Summary>> {
    super::super::manager(app)
        .await?
        .db(move |s| {
            storage::list(
                &s,
                workspace_id.as_deref(),
                offset.unwrap_or(0),
                lookahead.unwrap_or(false),
            )
        })
        .await
}
#[tauri::command]
pub async fn mission_get(app: tauri::AppHandle, id: String) -> Result<Mission> {
    super::super::manager(app)
        .await?
        .db(move |s| {
            let mut m = storage::get(&s, &id)?;
            if m.rounds
                .iter()
                .any(|r| r.challenge.is_some() && r.claim_record.is_none())
            {
                m.brief = brief::render(&m);
            }
            Ok(m)
        })
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
    let mut previous = m.state.clone();
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
            resume(&coordinator, &mut m).await?;
            // Going back to a wait the researcher already knows about is no news.
            if m.state == "waiting" {
                previous = m.state.clone();
            }
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
        "reconsider" if m.state == "attention" => reconsider(&coordinator, &mut m).await?,
        _ => return Err("This action is unavailable in the automation's current state".into()),
    }
    save(&coordinator, &mut m, &action).await?;
    notify(&coordinator, &m, &previous);
    Ok(m)
}

/// Takes up where the pause left off. A wait that is still open stays a wait, so
/// nothing is spent asking the same question again, and the next check reads any
/// tracked change made during the pause.
pub(super) async fn resume(coordinator: &Coordinator, m: &mut Mission) -> Result<()> {
    m.reason = "Resuming within the remaining budget".into();
    m.due_at = Some(now());
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
    } else if let Some(wait) = planning::open_wait(m) {
        m.state = "waiting".into();
        m.reason = wait.into();
        m.due_at = planning::wait_due(m);
    } else {
        m.state = "queued".into();
    }
    Ok(())
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

/// Takes up any check the researcher has approved again, and marks the ones still
/// waiting for that approval. Permissions never widen here: an unapproved check
/// keeps its old pin and stays refused. Returns the checks newly marked.
pub(super) async fn refresh_checks(m: &mut Mission) -> Result<Vec<String>> {
    let refused = adapters::repin_checks(&mut m.scope).await?;
    let mut newly_refused = Vec::new();
    for capability in &mut m.capabilities {
        let pinned = m
            .scope
            .checks
            .get(&capability.id)
            .or_else(|| m.scope.captured_checks.get(&capability.id));
        if let Some(pinned) = pinned {
            capability.fingerprint = pinned.clone();
        }
        let unavailable = refused.contains(&capability.id);
        if unavailable && !capability.unavailable {
            newly_refused.push(capability.name.clone());
        }
        capability.unavailable = unavailable;
    }
    Ok(newly_refused)
}

/// The check the next action needs, if it is still waiting for approval.
fn refused_selection(m: &Mission) -> Option<String> {
    let selected = m.rounds.last().and_then(|r| r.selected.as_ref());
    selected
        .filter(|_| m.phase == Phase::Investigate)
        .and_then(|c| c.capability_id.as_ref())
        .and_then(|id| m.capabilities.iter().find(|c| &c.id == id))
        .filter(|c| c.unavailable)
        .map(|c| c.name.clone())
}

/// The child must have stopped before the automation can move past it.
async fn stopped_child(coordinator: &Coordinator, id: String) -> Result<TaskRun> {
    let child = coordinator.db(move |s| s.get(&id)).await?;
    if child
        .progress
        .receipts
        .values()
        .any(|r| r.state == "running")
    {
        return Err("The previous step is still stopping. Try again in a moment.".into());
    }
    Ok(child)
}

async fn close_child(coordinator: &Coordinator, m: &mut Mission, child: &str) -> Result<()> {
    let mut copy = m.clone();
    let child = child.to_string();
    *m = coordinator
        .db(move |s| {
            storage::adopted(&s, &mut copy, &child)?;
            Ok(copy)
        })
        .await?;
    Ok(())
}

pub(super) async fn retry(coordinator: &Coordinator, m: &mut Mission) -> Result<()> {
    if let Some(id) = m.active_child.clone() {
        let child = stopped_child(coordinator, id).await?;
        if child
            .progress
            .receipts
            .values()
            .any(|r| r.state == "unknown")
        {
            coordinator.settle_turns(&child).await;
            let mut recovered = child;
            if coordinator.recover_results(&mut recovered).await? {
                return Err(
                    "The original action is still running. Check saved results before retrying."
                        .into(),
                );
            }
            if recovered
                .progress
                .receipts
                .values()
                .all(|r| r.state == "completed")
            {
                return reconcile(coordinator, m).await;
            }
        }
    }
    if exhausted(m) {
        return Err(
            "This automation has used its budget. Start a new research automation to do more work."
                .into(),
        );
    }
    let Some(id) = m.active_child.clone() else {
        return reconsider(coordinator, m).await;
    };
    let child = stopped_child(coordinator, id).await?;
    // Accounting takes back the child's scope, so checks are read after it. On
    // a copy, because a refused retry spends nothing: it could only fail again.
    let mut next = m.clone();
    account(&mut next, &child);
    refresh_checks(&mut next).await?;
    if let Some(name) = refused_selection(&next) {
        return Err(format!(
            "'{name}' changed and needs your approval. Approve it in the project's Results view and retry, or choose Reconsider the agenda."
        ));
    }
    *m = next;
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
        "Retry authorized with current conversation input and the original resource limits".into();
    close_child(coordinator, m, &child.id).await
}

/// Goes back to the planner, leaving any stuck action behind. The action that
/// could not run may belong to any role, and a role's conversation may have
/// moved on (a steering message, a turn that settled later), so every role takes
/// up its own conversation again or the next dispatch fails the same way.
pub(super) async fn reconsider(coordinator: &Coordinator, m: &mut Mission) -> Result<()> {
    if exhausted(m) {
        return Err(
            "This automation has used its budget. Start a new research automation to do more work."
                .into(),
        );
    }
    let child = match m.active_child.clone() {
        Some(id) => Some(stopped_child(coordinator, id).await?),
        None => None,
    };
    // Accounting takes back the child's scope, so everything below reads after it.
    if let Some(child) = &child {
        account(m, child);
        m.active_child = None;
    }
    refresh_checks(m).await?;
    let mut scopes = Vec::new();
    for scope in [&m.scope, &m.planner_scope, &m.challenger_scope] {
        let session = scope
            .session_id
            .clone()
            .ok_or("This automation's conversation is missing")?;
        let expected = m.authority_fingerprints.get(&session).cloned().ok_or(
            "This automation's permission record is unavailable. Start a new research automation.",
        )?;
        let old = scope.clone();
        scopes.push(
            run_store(move |s| workspace::readopt_scope(&s, &old, &expected, true))
                .await
                .map_err(|e| e.message)?,
        );
    }
    m.challenger_scope = scopes.pop().ok_or("Role scope is missing")?;
    m.planner_scope = scopes.pop().ok_or("Role scope is missing")?;
    m.scope = scopes.pop().ok_or("Role scope is missing")?;
    m.phase = Phase::Plan;
    m.state = "queued".into();
    m.due_at = Some(now());
    m.reason = "Reconsidering the agenda within the remaining budget".into();
    m.stagnant_rounds = 0;
    if let Some(child) = child {
        close_child(coordinator, m, &child.id).await?;
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
pub async fn mission_accept_assessment(
    app: tauri::AppHandle,
    id: String,
    revision: i64,
    subject_version: String,
) -> Result<Mission> {
    let c = super::super::manager(app).await?;
    let _gate = c.gate.lock().await;
    let mut m = c.db(move |s| storage::get(&s, &id)).await?;
    if m.revision != revision {
        return Err("Assessment changed; reload before accepting".into());
    }
    let record = m
        .rounds
        .last_mut()
        .and_then(|r| r.claim_record.as_mut())
        .ok_or("No current claim assessment to accept")?;
    record.accept(&subject_version)?;
    save(&c, &mut m, "assessmentAccepted").await?;
    notify(&c, &m, &m.state);
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

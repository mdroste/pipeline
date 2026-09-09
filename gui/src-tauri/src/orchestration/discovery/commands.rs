use super::*;
use crate::workbench::store::{CreateSessionRequest, CreateWorkspaceRequest};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Start {
    pub definition: Definition,
    pub session_id: Option<String>,
    pub operation_id: String,
}
#[tauri::command]
pub async fn discovery_start(app: tauri::AppHandle, request: Start) -> Result<Value> {
    validate::definition(&request.definition)?;
    validate::text(&request.operation_id, 128)?;
    let c = super::super::manager(app).await?;
    let _gate = c.gate.lock().await;
    let fp = store::hash(&(&request.definition, &request.session_id))?;
    let op = request.operation_id.clone();
    let fingerprint = fp.clone();
    if let Some(r) = c
        .db(move |s| storage::previous(&s, &op, &fingerprint))
        .await?
    {
        return c
            .db(move |s| storage::load(&s, &r.id).map(|p| export::view(&p)))
            .await;
    }
    let operation = request.operation_id.clone();
    let selected = request.session_id.clone();
    let acquire = request.definition.acquire_literature;
    let title = request
        .definition
        .prompt
        .chars()
        .take(80)
        .collect::<String>();
    let (binding, authority, context) = run_store(move |s| {
        let session = if let Some(id) = selected {
            id
        } else {
            let ws = s
                .create_workspace(CreateWorkspaceRequest {
                    name: format!("Self-discovery: {title}"),
                    root: None,
                    operation_id: format!("discovery-project-{operation}"),
                })?
                .record;
            if acquire {
                crate::workbench::acquisition::set_network(&s, &ws.id, true)?;
            }
            s.create_session(CreateSessionRequest {
                workspace_id: Some(ws.id),
                title: "Self-discovery research".into(),
                operation_id: format!("discovery-source-{operation}"),
            })?
            .record
            .id
        };
        let b = crate::workbench::tasks::binding(&s, &session)?;
        crate::workbench::tasks::validate_binding(&s, &b)?;
        let h = crate::workbench::research::resolve_harness(&s, &session)?;
        Ok((b, research::authority(&s, &session)?, h.context_preview))
    })
    .await
    .map_err(|e| e.message)?;
    let ws = binding
        .workspace_id
        .clone()
        .ok_or("Choose a conversation in a project")?;
    if request.definition.acquire_literature {
        let ws = ws.clone();
        if !run_store(move |s| crate::workbench::acquisition::network_enabled(&s, &ws))
            .await
            .map_err(|e| e.message)?
        {
            return Err("Enable literature acquisition in this project's Library before choosing remote literature search".into());
        }
    }
    let mut scope = store::Scope {
        session_id: Some(binding.session_id.clone()),
        workspace_id: Some(ws.clone()),
        session_cursor: Some(binding.cursor),
        harness_fingerprint: Some(binding.harness_fingerprint),
        root_identity: Some(binding.root_identity),
        runtime_root: Some(binding.runtime_root),
        ..store::Scope::default()
    };
    if let Some(id) = &request.definition.review_profile_id {
        let key = id.clone();
        let pin = tokio::task::spawn_blocking(move || crate::commands::orchestration::pin(&key))
            .await
            .map_err(store::err)??;
        scope.profiles.insert(id.clone(), pin);
    }
    let paths = request.definition.input_paths.clone();
    let input_scope = scope.clone();
    let input_artifacts = c
        .db(move |s| {
            let mut inputs = BTreeMap::new();
            for (i, path) in paths.iter().enumerate() {
                let basename = std::path::Path::new(path)
                    .file_name()
                    .and_then(|p| p.to_str())
                    .ok_or("Input file name missing")?;
                let name = format!("input-{}-{basename}", i + 1);
                let artifact =
                    adapters::snapshot(&s, &input_scope, json!({"path":path}), basename)?;
                if artifact["kind"] != "artifact" {
                    return Err("Choose individual research input files, not a folder".into());
                }
                inputs.insert(name, artifact);
            }
            check_artifacts(&inputs)?;
            Ok(inputs)
        })
        .await?;
    let time = now();
    let r = Run {
        id: store::id(),
        revision: 0,
        deadline_at: time + i64::from(request.definition.deadline_hours) * 3600,
        definition: request.definition,
        workspace_id: ws,
        source_session_id: binding.session_id,
        source_scope: scope,
        source_authority: authority,
        source_context: context.chars().take(24000).collect(),
        input_artifacts,
        catalog_revision: crate::auto_review::catalog_revision().into(),
        state: "running".into(),
        reason: "Identifying the field and appropriate research standards".into(),
        phase: Phase::Orient,
        orientation: None,
        literature: Vec::new(),
        deep_candidates: Vec::new(),
        selection: None,
        selection_hash: None,
        selected: Vec::new(),
        ranking: Vec::new(),
        actions_reserved: 0,
        active_seconds: 0,
        active_child: None,
        active_scope: None,
        replacements: 0,
        repairs: 0,
        created_at: time,
        updated_at: time,
        due_at: Some(time),
        stop_outcome: None,
    };
    let copy = r.clone();
    c.db(move |s| storage::create(&s, &copy, &request.operation_id, &fp))
        .await?;
    notify(&c, &r);
    Ok(export::view(&Portfolio {
        run: r,
        candidates: Vec::new(),
        papers: Vec::new(),
    }))
}
#[tauri::command]
pub async fn discovery_list(
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
pub async fn discovery_get(app: tauri::AppHandle, id: String) -> Result<Value> {
    super::super::manager(app)
        .await?
        .db(move |s| storage::load(&s, &id).map(|p| export::view(&p)))
        .await
}
#[tauri::command]
pub async fn discovery_candidates(
    app: tauri::AppHandle,
    id: String,
    offset: Option<usize>,
) -> Result<Vec<Candidate>> {
    super::super::manager(app)
        .await?
        .db(move |s| {
            Ok(storage::load(&s, &id)?
                .candidates
                .into_iter()
                .skip(offset.unwrap_or(0))
                .take(25)
                .collect())
        })
        .await
}
#[tauri::command]
pub async fn discovery_paper(
    app: tauri::AppHandle,
    id: String,
    candidate_id: u32,
) -> Result<Value> {
    super::super::manager(app).await?.db(move|s|{
        let p=storage::load(&s,&id)?.papers.into_iter().find(|p|p.candidate_id==candidate_id).ok_or("Paper unavailable")?;
        Ok(json!({"candidateId":p.candidate_id,"state":p.state,"reason":p.reason,"rounds":p.rounds,"challenges":p.challenges,"evidence":p.evidence,"versions":p.versions,"assessment":p.assessment}))
    }).await
}
#[tauri::command]
pub async fn discovery_select(
    app: tauri::AppHandle,
    id: String,
    selection_hash: String,
    candidate_ids: Vec<u32>,
    operation_id: String,
) -> Result<Value> {
    let c = super::super::manager(app).await?;
    let _gate = c.gate.lock().await;
    let mut p = c.db(move |s| storage::load(&s, &id)).await?;
    p = c
        .db(move |s| {
            storage::select(&s, &mut p, &selection_hash, candidate_ids, &operation_id)?;
            Ok(p)
        })
        .await?;
    notify(&c, &p.run);
    Ok(export::view(&p))
}
#[tauri::command]
pub async fn discovery_control(
    app: tauri::AppHandle,
    id: String,
    revision: i64,
    action: String,
) -> Result<Value> {
    let c = super::super::manager(app).await?;
    let _gate = c.gate.lock().await;
    let mut p = c.db(move |s| storage::load(&s, &id)).await?;
    if p.run.revision != revision {
        return Err("Self-discovery changed; reload before continuing".into());
    }
    match action.as_str() {
        "pause" if matches!(p.run.state.as_str(), "running" | "awaitingSelection") => {
            p.run.state = "paused".into();
            p.run.reason = "Paused; active work will settle and be retained".into();
            if let Some(id) = p.run.active_child.clone() {
                c.db(move |s| {
                    let mut r = s.get(&id)?;
                    if r.state == "queued"
                        && !r.progress.receipts.values().any(|r| r.state == "running")
                    {
                        r.state = "paused".into();
                        r.due_at = None;
                        s.save(&mut r, "paused", "Owning automation paused")?;
                    }
                    Ok(())
                })
                .await?;
            }
            p.run.due_at = None;
        }
        "resume" if p.run.state == "paused" => {
            if p.run.deadline_at <= now() {
                return Err("This automation's elapsed deadline has passed".into());
            }
            p.run.state = if p.run.phase == Phase::Select {
                "awaitingSelection"
            } else {
                "running"
            }
            .into();
            p.run.reason = "Resuming within the original limits".into();
            p.run.due_at = Some(now());
            if let Some(id) = p.run.active_child.clone() {
                c.db(move |s| {
                    let mut r = s.get(&id)?;
                    if r.state == "paused" {
                        r.state = "queued".into();
                        r.due_at = Some(now());
                        s.save(&mut r, "resumed", "Owning automation resumed")?;
                    }
                    Ok(())
                })
                .await?;
            }
        }
        "stop" if !p.run.terminal() => {
            if let Some(id) = p.run.active_child.clone() {
                super::super::missions::stop_child(&c, &id).await?;
                p.run.state = "stopping".into();
                p.run.stop_outcome = Some("cancelled".into());
                p.run.due_at = Some(now() + 2);
            } else {
                p.run.state = "cancelled".into();
                p.run.due_at = None;
            }
            p.run.reason = "Stopped by the researcher; recorded work is retained".into();
        }
        _ => return Err("This control is unavailable in the current state".into()),
    }
    save(&c, &mut p, &action, None).await?;
    if p.run.terminal() {
        let id = p.run.id.clone();
        run_store(move |s| workspace::revoke(&s, &id))
            .await
            .map_err(|e| e.message)?;
    }
    Ok(export::view(&p))
}
#[tauri::command]
pub async fn discovery_export(
    app: tauri::AppHandle,
    id: String,
    format: String,
) -> Result<Option<String>> {
    let c = super::super::manager(app.clone()).await?;
    let (ext, bytes) = c
        .db(move |s| {
            let p = storage::load(&s, &id)?;
            match format.as_str() {
                "markdown" => Ok(("md", export::report(&p).into_bytes())),
                "zip" => Ok(("zip", export::bundle(&s, &p)?)),
                _ => Err("Choose Markdown or a research ZIP package".into()),
            }
        })
        .await?;
    crate::commands::save_via_dialog(
        &app,
        &format!("self-discovery.{ext}"),
        "Self-discovery research",
        ext,
        bytes,
    )
    .await
}
#[tauri::command]
pub async fn discovery_events(
    app: tauri::AppHandle,
    id: String,
    after: Option<i64>,
) -> Result<Vec<store::Event>> {
    super::super::manager(app)
        .await?
        .db(move |s| storage::events(&s, &id, after.unwrap_or(0)))
        .await
}

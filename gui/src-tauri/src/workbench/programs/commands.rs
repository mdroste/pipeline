//! Narrow host facade; only this module crosses the blocking-store boundary.
use super::*;
use crate::workbench::{commands as host, research};
use tauri::Emitter;
#[derive(Debug, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Action {
    Choices,
    ExperimentPrepare {
        request: experiments::Prepare,
    },
    ExperimentStatus {
        id: String,
    },
    ExperimentControl {
        id: String,
        control: String,
        fingerprint: String,
    },
    ExperimentExport {
        id: String,
    },
    Table {
        spec: assets::AssetSpec,
        supersedes: Option<String>,
        operation_id: String,
    },
    FigurePlan {
        request: assets::FigureRequest,
    },
    FigureAdopt {
        recipe_id: String,
        execution_id: String,
        operation_id: String,
    },
    Artifact {
        id: String,
    },
    ExportArtifact {
        id: String,
        path: String,
    },
    Stage {
        record_id: String,
        artifact_id: String,
        checkpoint_id: String,
        path: String,
        expected_hash: Option<String>,
    },
    SaveSymbol {
        symbol: theory::Symbol,
        supersedes: Option<String>,
        operation_id: String,
    },
    Symbols,
    SymbolCandidates {
        source: ResearchObjectRef,
    },
    AssumptionBranch {
        request: theory::Branch,
    },
    TheoryCheck {
        request: theory::Check,
    },
    SaveCampaign {
        title: String,
        campaign: campaigns::Campaign,
        supersedes: Option<String>,
        operation_id: String,
    },
    CampaignStatus {
        id: String,
    },
    CampaignReview {
        id: String,
        anchors: Vec<String>,
        dependencies: Vec<String>,
    },
    ExportDeliverable {
        id: String,
        path: String,
    },
    Kits,
    InstallKit {
        kit_id: String,
        instructions: String,
        operation_id: String,
    },
    Assemble {
        outline: delivery::Outline,
        supersedes: Option<String>,
        operation_id: String,
    },
    Role {
        role: String,
        object: ResearchObjectRef,
        supersedes: Option<String>,
        operation_id: String,
    },
    Coauthor {
        title: String,
        brief: delivery::CoauthorBrief,
        supersedes: Option<String>,
        operation_id: String,
    },
    CapsulePreview {
        request: capsule::Export,
    },
    CapsuleExport {
        request: capsule::Export,
        fingerprint: String,
    },
    CapsuleInspect {
        path: String,
    },
    CapsuleImport {
        path: String,
        fingerprint: String,
        operation_id: String,
    },
    CapsuleBind {
        request: capsule::Bind,
    },
    CapsuleVerify {
        binding_id: String,
        execution_id: String,
    },
    SaveMonitor {
        title: String,
        monitor: monitors::Monitor,
        operation_id: String,
    },
    Monitors,
    MonitorControl {
        id: String,
        revision: i64,
        control: String,
    },
}
fn scope(expected: &str, actual: &str) -> WorkbenchResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(WorkbenchError::invalid("Request project scope differs"))
    }
}
pub fn apply(store: &Store, ws: &str, action: Action) -> WorkbenchResult<Value> {
    store.workspace(ws)?;
    match action {
        Action::Choices => choices(store, ws),
        Action::ExperimentPrepare { request } => {
            scope(ws, &request.workspace_id)?;
            Ok(json!(experiments::prepare(store, request)?))
        }
        Action::ExperimentStatus { id } => experiments::status(store, ws, &id),
        Action::ExperimentControl {
            id,
            control,
            fingerprint,
        } => experiments::control(store, ws, &id, &control, &fingerprint),
        Action::ExperimentExport { id } => {
            let status = experiments::status(store, ws, &id)?;
            let artifact = blob(
                store,
                ws,
                &serde_json::to_vec_pretty(&status).map_err(err)?,
                "json",
            )?;
            Ok(json!(artifact))
        }
        Action::Table {
            spec,
            supersedes,
            operation_id,
        } => Ok(json!(assets::table(
            store,
            ws,
            spec,
            supersedes.as_deref(),
            &operation_id
        )?)),
        Action::FigurePlan { request } => {
            scope(ws, &request.workspace_id)?;
            Ok(json!(assets::figure_plan(store, request)?))
        }
        Action::FigureAdopt {
            recipe_id,
            execution_id,
            operation_id,
        } => Ok(json!(assets::adopt_figure(
            store,
            ws,
            &recipe_id,
            &execution_id,
            &operation_id
        )?)),
        Action::Artifact { id } => read_artifact(store, ws, &id),
        Action::ExportArtifact { id, path } => {
            export_artifact(store, ws, &id, &path)?;
            Ok(json!({"path":path}))
        }
        Action::Stage {
            record_id,
            artifact_id,
            checkpoint_id,
            path,
            expected_hash,
        } => assets::stage(
            store,
            ws,
            &record_id,
            &artifact_id,
            &checkpoint_id,
            &path,
            expected_hash.as_deref(),
        ),
        Action::SaveSymbol {
            symbol,
            supersedes,
            operation_id,
        } => Ok(json!(theory::save(
            store,
            ws,
            symbol,
            supersedes.as_deref(),
            &operation_id
        )?)),
        Action::Symbols => theory::collisions(store, ws),
        Action::SymbolCandidates { source } => theory::candidates(store, ws, source),
        Action::AssumptionBranch { request } => {
            scope(ws, &request.workspace_id)?;
            Ok(json!(theory::branch(store, request)?))
        }
        Action::TheoryCheck { request } => {
            scope(ws, &request.workspace_id)?;
            Ok(json!(theory::check(store, request)?))
        }
        Action::SaveCampaign {
            title,
            campaign,
            supersedes,
            operation_id,
        } => Ok(json!(campaigns::save(
            store,
            ws,
            &title,
            campaign,
            supersedes.as_deref(),
            &operation_id
        )?)),
        Action::CampaignStatus { id } => campaigns::completion(store, ws, &id),
        Action::CampaignReview {
            id,
            anchors,
            dependencies,
        } => campaigns::review(store, ws, &id, anchors, dependencies),
        Action::ExportDeliverable { id, path } => delivery::export_bundle(store, ws, &id, &path),
        Action::Kits => Ok(json!(delivery::kits())),
        Action::InstallKit {
            kit_id,
            instructions,
            operation_id,
        } => Ok(json!(delivery::install(
            store,
            ws,
            &kit_id,
            &instructions,
            &operation_id
        )?)),
        Action::Assemble {
            outline,
            supersedes,
            operation_id,
        } => Ok(json!(delivery::assemble(
            store,
            ws,
            outline,
            supersedes.as_deref(),
            &operation_id
        )?)),
        Action::Role {
            role,
            object,
            supersedes,
            operation_id,
        } => Ok(json!(delivery::role(
            store,
            ws,
            &role,
            object,
            supersedes.as_deref(),
            &operation_id
        )?)),
        Action::Coauthor {
            title,
            brief,
            supersedes,
            operation_id,
        } => Ok(json!(delivery::coauthor(
            store,
            ws,
            &title,
            brief,
            supersedes.as_deref(),
            &operation_id
        )?)),
        Action::CapsulePreview { request } => {
            scope(ws, &request.workspace_id)?;
            capsule::preview(store, &request)
        }
        Action::CapsuleExport {
            request,
            fingerprint,
        } => {
            scope(ws, &request.workspace_id)?;
            capsule::export(store, request, &fingerprint)
        }
        Action::CapsuleInspect { path } => capsule::inspect(&path),
        Action::CapsuleImport {
            path,
            fingerprint,
            operation_id,
        } => Ok(json!(capsule::import(
            store,
            ws,
            &path,
            &fingerprint,
            &operation_id
        )?)),
        Action::CapsuleBind { request } => {
            scope(ws, &request.workspace_id)?;
            capsule::bind(store, request)
        }
        Action::CapsuleVerify {
            binding_id,
            execution_id,
        } => capsule::verify(store, ws, &binding_id, &execution_id),
        Action::SaveMonitor {
            title,
            monitor,
            operation_id,
        } => Ok(json!(monitors::save(
            store,
            ws,
            &title,
            monitor,
            &operation_id
        )?)),
        Action::Monitors => monitors::list(store, ws),
        Action::MonitorControl {
            id,
            revision,
            control,
        } => monitors::control(store, ws, &id, revision, &control),
    }
}
#[tauri::command]
pub async fn workbench_program(
    app: tauri::AppHandle,
    workspace_id: String,
    action: Action,
) -> WorkbenchResult<Value> {
    start(app);
    let result = host::run_store(move |store| apply(&store, &workspace_id, action)).await;
    research::jobs::launch_pending();
    result
}
#[derive(Debug, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum QueueAction {
    List {
        #[serde(default)]
        history_offset: u32,
    },
    ReviewContext {
        id: String,
        revision: i64,
    },
    RefreshContext {
        id: String,
        revision: i64,
        fingerprint: String,
    },
    Enqueue {
        text: String,
        model: Option<String>,
        effort: Option<String>,
        operation_id: String,
    },
    Control {
        id: String,
        revision: i64,
        control: String,
    },
    Run {
        id: String,
        fingerprint: String,
    },
    Reconcile {
        id: String,
    },
    Branch {
        title: String,
        items: Vec<desk::ContextItem>,
        operation_id: String,
    },
}
#[tauri::command]
pub async fn workbench_followups(
    app: tauri::AppHandle,
    session_id: String,
    action: QueueAction,
) -> WorkbenchResult<Value> {
    if let QueueAction::Run { id, fingerprint } = action {
        let session = session_id.clone();
        let key = id.clone();
        let (request, binding) =
            host::run_store(move |s| followups::prepare_dispatch(&s, &session, &key, &fingerprint))
                .await?;
        let result = host::submit_turn(app, request, Some(binding)).await;
        let transport = match result {
            Ok(receipt) => json!({"accepted":receipt}),
            Err(error) => json!({"error":error}),
        };
        return host::run_store(move |s| {
            followups::reconcile(&s, &session_id, &id, Some(transport))
        })
        .await;
    }
    host::run_store(move |s| match action {
        QueueAction::List { history_offset } => {
            followups::list_history(&s, &session_id, history_offset)
        }
        QueueAction::ReviewContext { id, revision } => {
            serde_json::to_value(followups::review_context(&s, &session_id, &id, revision)?)
                .map_err(err)
        }
        QueueAction::RefreshContext {
            id,
            revision,
            fingerprint,
        } => followups::refresh_context(&s, &session_id, &id, revision, &fingerprint),
        QueueAction::Enqueue {
            text,
            model,
            effort,
            operation_id,
        } => followups::enqueue(&s, &session_id, &text, model, effort, &operation_id),
        QueueAction::Control {
            id,
            revision,
            control,
        } => followups::control(&s, &session_id, &id, revision, &control),
        QueueAction::Reconcile { id } => followups::reconcile(&s, &session_id, &id, None),
        QueueAction::Branch {
            title,
            items,
            operation_id,
        } => followups::branch(&s, &session_id, &title, items, &operation_id),
        QueueAction::Run { .. } => unreachable!(),
    })
    .await
}
pub fn choices(store: &Store, ws: &str) -> WorkbenchResult<Value> {
    let c = store.connection()?;
    let mut q=c.prepare("SELECT kind,id,revision,title,provenance FROM research_search_sources WHERE workspace_id=?1 AND kind NOT IN ('message','artifact') GROUP BY kind,id,revision ORDER BY title,revision DESC LIMIT 1000").map_err(err)?;
    let objects=q.query_map([ws],|r|Ok(json!({"reference":{"kind":r.get::<_,String>(0)?,"id":r.get::<_,String>(1)?,"revision":r.get::<_,String>(2)?,"start":null,"end":null},"title":r.get::<_,String>(3)?,"provenance":r.get::<_,String>(4)?}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    Ok(
        json!({"objects":objects,"plans":desk::records(store,ws,"execution_plan")?,"profiles":research::list_execution_profiles(store,ws)?,"executions":research::list_executions(store,ws)?,"checkpoints":crate::workbench::project::records(store,ws,"checkpoint")?,"responses":crate::workbench::project::records(store,ws,"response")?,"theory":crate::workbench::project::records(store,ws,"theory")?}),
    )
}
static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
pub fn start(app: tauri::AppHandle) {
    if STARTED.set(()).is_err() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            let plans=host::run_store(|s|{let c=s.connection()?;let mut q=c.prepare("SELECT d.workspace_id,d.id FROM experiment_runs r JOIN desk_records d ON d.id=r.plan_id WHERE r.state IN ('running','paused','cancelled') AND (r.state='running' OR r.active_execution_id IS NOT NULL) ORDER BY r.updated_at LIMIT 4").map_err(err)?;let ids=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;for (ws,id) in ids{experiments::tick(&s,&ws,&id)?;}Ok(())}).await;
            if let Err(e) = plans {
                let _ = app.emit("workbench-program-error", json!({"message":e.message}));
            }
            research::jobs::launch_pending();
            if let Ok(checks) =
                host::run_store(|s| monitors::due(&s, chrono::Utc::now().timestamp())).await
            {
                for (record, revision) in checks {
                    let Ok(m) = serde_json::from_value::<monitors::Monitor>(record.body) else {
                        continue;
                    };
                    let ws = record.workspace_id.clone();
                    let id = record.id.clone();
                    let result = if m.kind == "metadata_query" {
                        let consent = m.network_consent;
                        let allowed =
                            host::run_store(move |s| acquisition_allowed(&s, &ws, consent))
                                .await
                                .unwrap_or(false);
                        if allowed {
                            crate::workbench::acquisition::crossref(&m.target)
                                .await
                                .map(|items| {
                                    let mut works = items
                                        .into_iter()
                                        .map(|i| json!({"doi":i.doi,"title":i.title,"year":i.year}))
                                        .collect::<Vec<_>>();
                                    works.sort_by_key(Value::to_string);
                                    json!({"works":works})
                                })
                        } else {
                            Err(WorkbenchError::invalid(
                                "Project network access or this query's consent is disabled",
                            ))
                        }
                    } else {
                        host::run_store(move |s| monitors::local(&s, &ws, &m)).await
                    };
                    let ws = record.workspace_id.clone();
                    let notify = host::run_store(move |s| {
                        monitors::record_outcome(
                            &s,
                            &ws,
                            &id,
                            revision,
                            result,
                            chrono::Utc::now().timestamp(),
                        )
                    })
                    .await
                    .unwrap_or(false);
                    if notify {
                        let _ = app.emit(
                            "workbench-research-attention",
                            json!({"workspaceId":record.workspace_id,"checkId":record.id}),
                        );
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}
fn acquisition_allowed(store: &Store, ws: &str, consent: bool) -> WorkbenchResult<bool> {
    Ok(consent && crate::workbench::acquisition::network_enabled(store, ws)?)
}
#[tauri::command]
pub async fn workbench_research_activity() -> WorkbenchResult<Value> {
    let pending = host::workbench_codex_pending_requests();
    let mut activity=host::run_store(|s|{let c=s.connection()?;
        let mut q=c.prepare("SELECT s.id,s.workspace_id,s.title,b.provider_thread_id,t.provider_turn_id,t.state FROM turns t JOIN session_bindings b ON b.id=t.binding_id JOIN sessions s ON s.id=b.session_id WHERE t.terminal_at IS NULL ORDER BY t.created_at DESC LIMIT 100").map_err(err)?;
        let turns=q.query_map([],|r|Ok(json!({"sessionId":r.get::<_,String>(0)?,"workspaceId":r.get::<_,Option<String>>(1)?,"title":r.get::<_,String>(2)?,"threadId":r.get::<_,String>(3)?,"turnId":r.get::<_,Option<String>>(4)?,"state":r.get::<_,String>(5)?}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        let mut q=c.prepare("SELECT e.id,e.workspace_id,w.name,e.adapter,e.outcome,j.owner FROM research_executions e JOIN workspaces w ON w.id=e.workspace_id LEFT JOIN execution_jobs j ON j.execution_id=e.id WHERE e.outcome IN ('queued','running','outcome_unknown') ORDER BY e.created_at DESC LIMIT 100").map_err(err)?;
        let jobs=q.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"workspaceId":r.get::<_,String>(1)?,"title":r.get::<_,String>(2)?,"adapter":r.get::<_,String>(3)?,"state":r.get::<_,String>(4)?,"owner":r.get::<_,Option<String>>(5)?}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        let count:i64=c.query_row("SELECT COUNT(*) FROM research_attention WHERE acknowledged_at IS NULL",[],|r|r.get(0)).map_err(err)?;
        Ok(json!({"turns":turns,"jobs":jobs,"researchAttention":count}))
    }).await?;
    activity["pendingRequests"] = json!(pending);
    Ok(activity)
}

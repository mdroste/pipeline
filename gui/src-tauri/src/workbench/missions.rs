//! Workspace-owned services used by the mission coordinator. No coordinator DB access.
use super::{desk, research, store::*, tasks};
use crate::orchestration::{
    missions::model::{Capability, Policy},
    store::Scope,
};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn error(e: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::storage("Research mission", e)
}

pub fn authority(store: &Store, session: &str) -> WorkbenchResult<String> {
    let h = research::resolve_harness(store, session)?;
    let s = store.session_snapshot(session)?.session;
    // Dynamic research context may evolve. Permissions, configuration, selected exact
    // objects, presets, model overrides and task-copy identity may not silently change.
    let v = json!({"preset":h.preset,"mode":h.mode,"network":h.command_network,
        "permission":h.permission_profile,"modules":h.enabled_modules,"tools":h.dynamic_tools,
        "overrides":s.overrides,"paperId":s.paper_id,"context":desk::context(store,session)?,
        "budget":h.context_budget_bytes,"globalConfig":research::load_config(store,None)?.body,
        "workspaceConfig":research::load_config(store,s.workspace_id.as_deref())?.body,
        "dataPolicy":s.workspace_id.as_deref().map(|ws|super::data::policy(store,ws)).transpose()?});
    Ok(desk::hash(v.to_string().as_bytes()))
}

pub fn refresh_scope(store: &Store, scope: &Scope, fingerprint: &str) -> WorkbenchResult<Scope> {
    let session = scope
        .session_id
        .as_deref()
        .ok_or_else(|| WorkbenchError::invalid("Mission conversation is missing"))?;
    if authority(store, session)? != fingerprint {
        return Err(WorkbenchError::conflict("Mission permissions or selected context changed. Prepare a new mission for that scope."));
    }
    let actual = tasks::binding(store, session)?;
    if Some(&actual.cursor) != scope.session_cursor.as_ref()
        || actual.workspace_id != scope.workspace_id
        || Some(&actual.runtime_root) != scope.runtime_root.as_ref()
        || Some(&actual.root_identity) != scope.root_identity.as_ref()
    {
        return Err(WorkbenchError::conflict("The mission conversation or folder changed outside its recorded work. Inspect the conversation before continuing."));
    }
    tasks::validate_binding(store, &actual)?;
    let mut result = scope.clone();
    result.harness_fingerprint = Some(actual.harness_fingerprint);
    Ok(result)
}

fn existing(store: &Store, operation: &str, kind: &str) -> WorkbenchResult<Option<String>> {
    store
        .connection()?
        .query_row(
            "SELECT entity_id FROM change_log WHERE operation_id=?1 AND entity_type=?2",
            params![operation, kind],
            |r| r.get(0),
        )
        .optional()
        .map_err(error)
}

pub fn roles(
    store: &Store,
    source: &tasks::TaskBinding,
    operation: &str,
    name: &str,
    policy: &Policy,
) -> WorkbenchResult<(Vec<Scope>, BTreeMap<String, String>, String)> {
    let original = store.session_snapshot(&source.session_id)?;
    let h = research::resolve_harness(store, &source.session_id)?;
    if original.session.workspace_id.is_none() {
        return Err(WorkbenchError::invalid(
            "Select a conversation filed in a research project",
        ));
    }
    if original
        .session
        .overrides
        .get("projectCheckpointId")
        .is_some()
        && (!policy.check_profile_ids.is_empty() || !policy.experiment_ids.is_empty())
    {
        return Err(WorkbenchError::invalid("Isolated task conversations use their native sandbox for computation. To delegate host checks or captured experiments, select a project conversation without a task-copy binding."));
    }
    if policy.allow_edits
        && (h.mode != "edit"
            || original
                .session
                .overrides
                .get("projectCheckpointId")
                .and_then(Value::as_str)
                .is_none())
    {
        return Err(WorkbenchError::invalid("Mission editing requires a source conversation already using an isolated project task in edit mode"));
    }
    if policy.command_network && !h.command_network {
        return Err(WorkbenchError::invalid(
            "Enable command networking in the selected conversation before delegating it",
        ));
    }
    let selections = desk::context(store, &source.session_id)?;
    let op = format!("mission-preset-{operation}");
    let preset_id = if let Some(id) = existing(store, &op, "preset")? {
        id
    } else {
        research::clone_preset(
            store,
            research::ClonePresetRequest {
                source_workspace_id: None,
                workspace_id: source.workspace_id.clone(),
                source_preset_id: h.preset.id.clone(),
                name: format!("Mission: {}", name.chars().take(100).collect::<String>()),
                operation_id: op.clone(),
            },
        )?
        .id
    };
    let update_op = format!("mission-preset-config-{operation}");
    if existing(store, &update_op, "preset")?.is_none() {
        let preset = research::harness_catalog(store, source.workspace_id.as_deref())?
            .presets
            .into_iter()
            .find(|p| p.id == preset_id)
            .ok_or_else(|| WorkbenchError::invalid("Mission preset is unavailable"))?;
        let mut modules: Vec<_> = preset
            .modules
            .into_iter()
            .filter(|m| m != "research_execution" && m != "task_tools")
            .collect();
        if !modules.iter().any(|m| m == "paper_tools") {
            modules.push("paper_tools".into());
        }
        research::update_preset(store,research::UpdatePresetRequest{base_prompt:None,preset_id:preset_id.clone(),expected_revision:preset.revision,
            name:preset.name,description:"Mission research tools; host executions are admitted by the coordinator.".into(),
            instructions:format!("{}\n\nThis conversation belongs to a bounded research mission. Report evidence and limitations. Research conclusions and reusable methods remain model assessments. Do not expand the mission remit, authorize host commands, or accept edits. Host checks are dispatched by the coordinator.",h.preset.instructions),
            modules,operation_id:update_op})?;
    }
    let mut scopes = Vec::new();
    let mut fingerprints = BTreeMap::new();
    for role in ["investigator", "planner", "challenger"] {
        let role_op = format!("{operation}-{role}");
        let b = tasks::occurrence_session(store, source, &role_op, &format!("{name} · {role}"))?;
        let configure_op = format!("mission-role-{role_op}");
        if existing(store, &configure_op, "session")?.is_none() {
            let s = store.session_snapshot(&b.session_id)?.session;
            let mut overrides = s.overrides;
            overrides["mode"] = json!(if role == "investigator" && policy.allow_edits {
                "edit"
            } else {
                "inspect"
            });
            overrides["commandNetwork"] = json!(policy.command_network);
            store.update_session(UpdateSessionRequest {
                session_id: s.id.clone(),
                expected_revision: s.revision,
                operation_id: configure_op,
                title: None,
                draft: None,
                overrides: Some(overrides),
                archived: None,
                preset_id: Some(preset_id.clone()),
                paper_id: s.paper_id,
                clear_paper: None,
            })?;
        }
        let current = desk::context(store, &b.session_id)?;
        if current.revision == 0 && !selections.items.is_empty() {
            desk::save_context(store, &b.session_id, 0, selections.items.clone())?;
        }
        let b = tasks::binding(store, &b.session_id)?;
        fingerprints.insert(b.session_id.clone(), authority(store, &b.session_id)?);
        scopes.push(Scope {
            session_id: Some(b.session_id),
            workspace_id: b.workspace_id,
            session_cursor: Some(b.cursor),
            harness_fingerprint: Some(b.harness_fingerprint),
            runtime_root: Some(b.runtime_root),
            root_identity: Some(b.root_identity),
            ..Scope::default()
        });
    }
    let context = format!("Source conversation context (source material):\n{}\n\nResearcher's last completed exchange:\n{}",h.context_preview, last_exchange(store,&source.session_id)?);
    Ok((
        scopes,
        fingerprints,
        super::search::prefix(&context, 48000).into(),
    ))
}

fn last_exchange(store: &Store, session: &str) -> WorkbenchResult<String> {
    let c = store.connection()?;
    let mut q=c.prepare("SELECT i.payload_json FROM transcript_items i JOIN session_bindings b ON b.id=i.binding_id WHERE b.session_id=?1 AND i.is_final=1 AND i.item_kind IN ('userMessage','agentMessage','user_message','agent_message') ORDER BY i.created_at DESC,i.id DESC LIMIT 4").map_err(error)?;
    let values = q
        .query_map([session], |r| r.get::<_, String>(0))
        .map_err(error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    Ok(values
        .into_iter()
        .rev()
        .map(|s| super::search::prefix(&s, 8000).to_string())
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn capabilities(store: &Store, ws: &str, policy: &Policy) -> WorkbenchResult<Vec<Capability>> {
    let mut result = Vec::new();
    for id in &policy.check_profile_ids {
        let profile = research::list_execution_profiles(store, ws)?
            .into_iter()
            .find(|p| &p.id == id)
            .ok_or_else(|| WorkbenchError::invalid("Check belongs to another project"))?;
        let preview = research::preview_host_execution(store, id)?;
        if !preview.authorized {
            return Err(WorkbenchError::invalid(format!(
                "Authorize check '{}' in Research tools first",
                profile.name
            )));
        }
        let execution = json!({"argv":profile.argv,"cwd":profile.cwd,"inputs":profile.inputs,"outputs":profile.outputs,"containment":"host_access","dependencyCoverage":"declared_only"});
        result.push(Capability {
            id: id.clone(),
            kind: "check".into(),
            name: profile.name,
            fingerprint: preview.fingerprint,
            parameters: json!({}),
            execution,
            timeout_seconds: profile.timeout_seconds,
        });
    }
    let mut ids = std::collections::BTreeSet::new();
    for id in &policy.experiment_ids {
        let record = desk::record(store, ws, id)?;
        if record.kind == "execution_plan" {
            ids.insert(id.clone());
        } else if record.kind == "experiment_plan" {
            let plan: super::programs::experiments::ExperimentPlan =
                serde_json::from_value(record.body).map_err(error)?;
            for spec in plan.specifications {
                if let Some(id) = spec.plan_id {
                    ids.insert(id);
                }
            }
        } else {
            return Err(WorkbenchError::invalid(
                "Choose a captured plan or prepared experiment grid",
            ));
        }
    }
    if ids.len() > 64 {
        return Err(WorkbenchError::invalid(
            "A mission can authorize at most 64 captured experiment variants",
        ));
    }
    for id in ids {
        let status = research::execution_plan::status(store, ws, &id)?;
        let p: research::execution_plan::ExecutionPlan =
            serde_json::from_value(status.record.body).map_err(error)?;
        let execution = json!({"argv":p.profile.argv,"inputs":p.captured_files.iter().map(|f|json!({"path":f.path,"hash":f.hash})).collect::<Vec<_>>(),"outputs":p.profile.outputs,"containment":p.containment,"dependencyCoverage":p.dependency_coverage,"toolchainVersion":p.toolchain_version,"cwd":"Fresh directory from these captured inputs"});
        result.push(Capability {
            id,
            kind: "experiment".into(),
            name: status.record.title,
            fingerprint: status.record.content_hash,
            parameters: p.parameters,
            execution,
            timeout_seconds: p.profile.timeout_seconds,
        });
    }
    for id in &policy.monitor_ids {
        let r = desk::record(store, ws, id)?;
        if r.kind != "monitor" {
            return Err(WorkbenchError::invalid("Choose a project monitor"));
        }
    }
    Ok(result)
}

pub fn authorize_experiments(
    store: &Store,
    ws: &str,
    capabilities: &[Capability],
) -> WorkbenchResult<()> {
    for c in capabilities.iter().filter(|c| c.kind == "experiment") {
        research::execution_plan::authorize(store, ws, &c.id, &c.fingerprint)?;
    }
    Ok(())
}

pub fn sources(
    store: &Store,
    ws: &str,
    refs: &[desk::ResearchObjectRef],
) -> WorkbenchResult<Vec<Value>> {
    refs.iter().map(|r| {
        let object=super::search::read_object(store,ws,r,8000)?;
        Ok(json!({"kind":"researchSource","reference":r,"object":object,"scope":"Exact retained source; support is assessed separately"}))
    }).collect()
}

pub fn changes(
    store: &Store,
    ws: &str,
    monitors: &[String],
    after: i64,
) -> WorkbenchResult<(i64, Vec<Value>)> {
    store.workspace(ws)?;
    let c = store.connection()?;
    let mut q=c.prepare("SELECT rowid,check_id,body_json FROM research_attention WHERE workspace_id=?1 AND rowid>?2 ORDER BY rowid LIMIT 64").map_err(error)?;
    let rows = q
        .query_map(params![ws, after], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    let mut cursor = after;
    let mut selected = Vec::new();
    for (sequence, id, body) in rows {
        cursor = sequence;
        if monitors.contains(&id) {
            selected.push(json!({"sequence":sequence,"monitorId":id,"change":serde_json::from_str::<Value>(&body).map_err(error)?}));
        }
    }
    Ok((cursor, selected))
}

/// Latest observed state supplements attention history, whose UI intentionally
/// deduplicates previously seen fingerprints. Reversions must still wake missions.
pub fn watch_snapshots(
    store: &Store,
    ws: &str,
    monitors: &[String],
) -> WorkbenchResult<BTreeMap<String, Value>> {
    let c = store.connection()?;
    let mut snapshots = BTreeMap::new();
    for id in monitors {
        let body:Option<String>=c.query_row("SELECT s.last_outcome_json FROM scheduled_checks s JOIN desk_records d ON d.id=s.id WHERE d.workspace_id=?1 AND s.id=?2",params![ws,id],|r|r.get(0)).optional().map_err(error)?.flatten();
        if let Some(body) = body {
            snapshots.insert(id.clone(), serde_json::from_str(&body).map_err(error)?);
        }
    }
    Ok(snapshots)
}
pub fn append_snapshot_changes(
    old: &BTreeMap<String, Value>,
    new: &BTreeMap<String, Value>,
    changes: &mut Vec<Value>,
) {
    for (id, value) in new {
        if old.get(id).is_some_and(|previous| previous != value)
            && !changes.iter().any(|c| c["monitorId"] == *id)
        {
            changes.push(json!({"monitorId":id,"change":value,"coverage":"Latest observed monitor state; intervening changes may coalesce."}));
        }
    }
}

pub fn choices(store: &Store, session: &str) -> WorkbenchResult<Value> {
    let snapshot = store.session_snapshot(session)?;
    let ws = snapshot
        .session
        .workspace_id
        .ok_or_else(|| WorkbenchError::invalid("Choose a project conversation"))?;
    let h = research::resolve_harness(store, session)?;
    let mut experiments = desk::records(store, &ws, "experiment_plan")?;
    experiments.extend(desk::records(store, &ws, "execution_plan")?);
    Ok(json!({"workspaceId":ws,"root":store.runtime_root(session)?,
        "canEdit":h.mode=="edit"&&snapshot.session.overrides.get("projectCheckpointId").is_some(),
        "commandNetwork":h.command_network,"canHostCompute":snapshot.session.overrides.get("projectCheckpointId").is_none(),"checks":research::list_execution_profiles(store,&ws)?,
        "experiments":experiments.into_iter().map(|r|json!({"id":r.id,"title":r.title,"kind":r.kind})).collect::<Vec<_>>(),
        "monitors":desk::records(store,&ws,"monitor")?.into_iter().map(|r|json!({"id":r.id,"title":r.title})).collect::<Vec<_>>()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Store, String, String) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir(&root).unwrap();
        let s = Store::open_at(&temp.path().join("store")).unwrap();
        let ws = s
            .create_workspace(CreateWorkspaceRequest {
                name: "Mission fixture".into(),
                root: Some(root.to_string_lossy().into()),
                operation_id: "workspace".into(),
            })
            .unwrap()
            .record
            .id;
        let session = s
            .create_session(CreateSessionRequest {
                workspace_id: Some(ws.clone()),
                title: "Research source".into(),
                operation_id: "session".into(),
            })
            .unwrap()
            .record
            .id;
        (temp, s, ws, session)
    }
    #[test]
    fn role_preparation_is_idempotent_scoped_and_preserves_the_source_draft() {
        let (_temp, s, _ws, session) = fixture();
        let current = s.session_snapshot(&session).unwrap().session;
        s.update_session(UpdateSessionRequest {
            session_id: session.clone(),
            expected_revision: current.revision,
            operation_id: "draft".into(),
            title: None,
            draft: Some("An unsent research idea".into()),
            overrides: None,
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
        let source = tasks::binding(&s, &session).unwrap();
        let (scopes, fingerprints, _) = roles(
            &s,
            &source,
            "fixture",
            "Research mission",
            &Policy::default(),
        )
        .unwrap();
        let (again, _, _) = roles(
            &s,
            &source,
            "fixture",
            "Research mission",
            &Policy::default(),
        )
        .unwrap();
        assert_eq!(
            scopes.iter().map(|s| &s.session_id).collect::<Vec<_>>(),
            again.iter().map(|s| &s.session_id).collect::<Vec<_>>()
        );
        for scope in &scopes {
            let id = scope.session_id.as_ref().unwrap();
            let h = research::resolve_harness(&s, id).unwrap();
            assert_eq!(h.mode, "inspect");
            assert!(h
                .dynamic_tools
                .iter()
                .any(|t| t["name"] == "workbench_research_object_v1"));
            assert!(!h
                .dynamic_tools
                .iter()
                .any(|t| t["name"] == "workbench_research_run"
                    || t["name"] == "workbench_task_propose"));
            assert!(refresh_scope(&s, scope, &fingerprints[id]).is_ok());
        }
        assert_eq!(
            s.session_snapshot(&session).unwrap().session.draft,
            "An unsent research idea"
        );
        assert_ne!(scopes[0].session_id, scopes[1].session_id);
        assert_ne!(scopes[1].session_id, scopes[2].session_id);
    }
    #[test]
    fn mission_cannot_upgrade_edit_or_network_permissions() {
        let (_temp, s, _ws, session) = fixture();
        let source = tasks::binding(&s, &session).unwrap();
        let policy = Policy {
            allow_edits: true,
            ..Policy::default()
        };
        assert!(roles(&s, &source, "edit", "Mission", &policy).is_err());
        let policy = Policy {
            command_network: true,
            ..Policy::default()
        };
        assert!(roles(&s, &source, "network", "Mission", &policy).is_err());
    }
    #[test]
    fn permission_changes_are_distinct_from_evolving_research_context() {
        let (_temp, s, _ws, session) = fixture();
        let source = tasks::binding(&s, &session).unwrap();
        let (scopes, fp, _) = roles(&s, &source, "roles", "Mission", &Policy::default()).unwrap();
        let scope = &scopes[0];
        let id = scope.session_id.as_ref().unwrap();
        tasks::deliver(
            &s,
            id,
            "retained-finding",
            json!({"finding":"An explicit new counterexample"}),
        )
        .unwrap();
        assert!(refresh_scope(&s, scope, &fp[id]).is_ok());
        let current = s.session_snapshot(id).unwrap().session;
        let mut overrides = current.overrides;
        overrides["commandNetwork"] = json!(true);
        s.update_session(UpdateSessionRequest {
            session_id: id.clone(),
            expected_revision: current.revision,
            operation_id: "changed-network".into(),
            title: None,
            draft: None,
            overrides: Some(overrides),
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
        assert!(refresh_scope(&s, scope, &fp[id]).is_err());
    }
    #[test]
    fn exact_sources_cannot_cross_projects_or_invent_revisions() {
        let (_temp, s, ws, _session) = fixture();
        let reference = desk::ResearchObjectRef {
            kind: "result".into(),
            id: "invented-result".into(),
            revision: "invented-revision".into(),
            start: None,
            end: None,
        };
        assert!(sources(&s, &ws, &[reference]).is_err());
    }
    #[test]
    fn change_signals_are_scoped_and_consumed_from_a_durable_cursor() {
        let (_temp, s, ws, _session) = fixture();
        let monitor = super::super::programs::monitors::save(
            &s,
            &ws,
            "Result changes",
            super::super::programs::monitors::Monitor {
                kind: "results".into(),
                target: String::new(),
                interval_seconds: 60,
                notify: false,
                network_consent: false,
            },
            "monitor",
        )
        .unwrap();
        for (at, value) in [(10, 1), (20, 2)] {
            super::super::programs::monitors::record_outcome(
                &s,
                &ws,
                &monitor.id,
                at / 10,
                Ok(json!({"value":value})),
                at,
            )
            .unwrap();
        }
        let (cursor, events) = changes(&s, &ws, std::slice::from_ref(&monitor.id), 0).unwrap();
        assert_eq!(events.len(), 1);
        assert!(changes(&s, &ws, std::slice::from_ref(&monitor.id), cursor)
            .unwrap()
            .1
            .is_empty());
        assert!(changes(&s, &ws, &[], 0).unwrap().1.is_empty());
        let ids = std::slice::from_ref(&monitor.id);
        let mut snapshots = watch_snapshots(&s, &ws, ids).unwrap();
        let mut cursor = cursor;
        for (at, value) in [(30, 1), (40, 2), (50, 1)] {
            super::super::programs::monitors::record_outcome(
                &s,
                &ws,
                &monitor.id,
                at / 10,
                Ok(json!({"value":value})),
                at,
            )
            .unwrap();
            let (next, mut events) = changes(&s, &ws, ids, cursor).unwrap();
            let observed = watch_snapshots(&s, &ws, ids).unwrap();
            append_snapshot_changes(&snapshots, &observed, &mut events);
            assert_eq!(
                events.len(),
                1,
                "A return to a previously seen state must wake a mission"
            );
            let mut duplicates = Vec::new();
            append_snapshot_changes(&observed, &observed, &mut duplicates);
            assert!(duplicates.is_empty());
            snapshots = observed;
            cursor = next;
        }
    }
    #[test]
    fn authorized_parameter_variants_run_from_the_capture_after_live_script_changes() {
        let (temp, s, ws, _session) = fixture();
        let root = temp.path().join("project");
        let script="import json, os\np=json.load(open(os.environ['PIPELINE_PARAMETERS_FILE']))\njson.dump({'value':p['x']**2},open('results.json','w'))\n";
        std::fs::write(root.join("analysis.py"), script).unwrap();
        let profile = research::save_execution_profile(
            &s,
            research::SaveExecutionProfileRequest {
                profile_id: None,
                workspace_id: ws.clone(),
                name: "Captured quadratic".into(),
                adapter: "command".into(),
                argv: vec!["/usr/bin/python3".into(), "analysis.py".into()],
                cwd: root.to_string_lossy().into(),
                environment: json!({}),
                inputs: vec!["analysis.py".into()],
                timeout_seconds: 30,
                outputs: vec!["results.json".into()],
                expected_revision: None,
                operation_id: "profile".into(),
            },
        )
        .unwrap();
        let base = research::execution_plan::capture(
            &s,
            research::execution_plan::CapturePlanRequest {
                research_inputs: Vec::new(),
                profile_id: profile.id.clone(),
                parameters: json!({"x":1}),
                random_seed: None,
                toolchain_version: "Fixture Python; executable identity captured".into(),
                operation_id: "capture".into(),
            },
        )
        .unwrap();
        let grid = super::super::programs::experiments::prepare(
            &s,
            super::super::programs::experiments::Prepare {
                workspace_id: ws.clone(),
                title: "Two variants".into(),
                question: "Compare two values".into(),
                base_plan_id: base.id,
                factors: vec![super::super::programs::experiments::Factor {
                    name: "x".into(),
                    values: vec![json!(2), json!(3)],
                }],
                exclusions: Vec::new(),
                interpretation: "Test exact parameter substitution".into(),
                max_runs: 2,
                max_seconds: 60,
                operation_id: "grid".into(),
            },
        )
        .unwrap();
        let policy = Policy {
            experiment_ids: vec![grid.id],
            ..Policy::default()
        };
        let caps = capabilities(&s, &ws, &policy).unwrap();
        assert_eq!(caps.len(), 2);
        authorize_experiments(&s, &ws, &caps).unwrap();
        std::fs::write(
            root.join("analysis.py"),
            "raise RuntimeError('This live script must not run')\n",
        )
        .unwrap();
        for cap in caps {
            let receipt = research::run_execution(
                &s,
                research::RunExecutionRequest {
                    profile_id: profile.id.clone(),
                    plan_id: Some(cap.id.clone()),
                    session_id: None,
                    test_only: true,
                    operation_id: format!("mission-fixture-{}", cap.id),
                },
            )
            .unwrap();
            assert_eq!(receipt.outcome, "completed");
            let output: Value = serde_json::from_slice(
                &std::fs::read(std::path::Path::new(&receipt.cwd).join("results.json")).unwrap(),
            )
            .unwrap();
            let x = cap.parameters["x"].as_i64().unwrap();
            assert_eq!(output["value"], x * x);
        }
    }
}

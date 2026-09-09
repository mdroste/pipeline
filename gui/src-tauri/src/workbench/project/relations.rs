//! Revision-specific decisions and explainable, bounded change propagation.
use crate::workbench::{
    desk::{self, err, DeskRecord, ResearchObjectRef},
    store::{Store, WorkbenchError, WorkbenchResult},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Relation {
    pub input: ResearchObjectRef,
    pub dependent: ResearchObjectRef,
    pub origin: String,
    pub accepted: bool,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decision {
    pub statement: String,
    pub rationale: String,
    pub alternatives: Vec<String>,
    pub assumptions: Vec<ResearchObjectRef>,
    pub state: String,
    pub note_id: Option<String>,
}
pub fn save_relation(
    store: &Store,
    ws: &str,
    mut r: Relation,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    for o in [&r.input, &r.dependent] {
        crate::workbench::search::read_object(store, ws, o, 1)?;
    }
    if r.input == r.dependent {
        return Err(WorkbenchError::invalid("An object cannot depend on itself"));
    }
    desk::check_text(&r.reason, 4000)?;
    r.origin = "user".into();
    r.accepted = true;
    desk::insert(
        store,
        ws,
        "relation",
        &r.reason,
        serde_json::to_value(&r).map_err(err)?,
        None,
        operation,
    )
}
pub fn save_decision(
    store: &Store,
    ws: &str,
    title: &str,
    mut d: Decision,
    supersedes: Option<&str>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    desk::check_text(&d.statement, 16000)?;
    desk::check_text(&d.rationale, 16000)?;
    if !["proposed", "accepted", "rejected"].contains(&d.state.as_str())
        || d.alternatives.len() > 32
        || d.assumptions.len() > 32
    {
        return Err(WorkbenchError::invalid(
            "Invalid decision state or too many alternatives/assumptions",
        ));
    }
    for o in &d.assumptions {
        crate::workbench::search::read_object(store, ws, o, 1)?;
    }
    let prior = supersedes
        .map(|id| desk::record(store, ws, id))
        .transpose()?;
    if prior.as_ref().is_some_and(|r| r.kind != "decision") {
        return Err(WorkbenchError::invalid("Supersede a decision record"));
    }
    desk::check_text(title, 500)?;
    let note_operation = format!("{operation}-note");
    let prior_note:Option<String>=store.connection()?.query_row("SELECT entity_id FROM change_log WHERE operation_id=?1 AND entity_type='research_note' AND workspace_id=?2",params![note_operation,ws],|r|r.get(0)).optional().map_err(err)?;
    let note_body = format!(
        "{}\n\nRationale: {}\nAlternatives: {}\nExact assumptions: {}\nSupersedes: {}",
        d.statement,
        d.rationale,
        d.alternatives.join("; "),
        serde_json::to_string(&d.assumptions).map_err(err)?,
        supersedes.unwrap_or("none")
    );
    let note = if let Some(id) = prior_note {
        let previous = super::research::get_note(store, &id)?;
        if previous.body != note_body {
            return Err(WorkbenchError::conflict(
                "Decision operation was reused with different content",
            ));
        }
        previous
    } else {
        super::research::create_note(
            store,
            super::research::CreateNoteRequest {
                workspace_id: ws.into(),
                paper_id: None,
                kind: "decision".into(),
                body: note_body.clone(),
                state: Some(d.state.clone()),
                origin: "user".into(),
                pinned: d.state == "accepted",
                operation_id: format!("{operation}-note"),
            },
            false,
        )?
    };
    d.note_id = Some(note.id.clone());
    let result = desk::insert(
        store,
        ws,
        "decision",
        title,
        serde_json::to_value(&d).map_err(err)?,
        supersedes,
        operation,
    )?;
    // Only an explicit accepted supersession retires the old accepted note.
    if d.state == "accepted" {
        if let Some(prior) = prior {
            if let Some(old) = prior.body["noteId"].as_str() {
                let c = store.connection()?;
                let revision:Option<i64>=c.query_row("SELECT revision FROM research_notes WHERE id=?1 AND workspace_id=?2 AND state='accepted'",params![old,ws],|r|r.get(0)).optional().map_err(err)?;
                if let Some(revision) = revision {
                    super::research::update_note(
                        store,
                        super::research::UpdateNoteRequest {
                            note_id: old.into(),
                            expected_revision: revision,
                            body: None,
                            state: Some("retired".into()),
                            pinned: Some(false),
                            operation_id: format!("{operation}-retire"),
                        },
                    )?;
                }
            }
        }
    }
    Ok(result)
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Impact {
    pub object: ResearchObjectRef,
    pub status: String,
    pub reason: String,
    pub path: Vec<ResearchObjectRef>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactReport {
    pub impacts: Vec<Impact>,
    pub relations: Vec<Relation>,
    pub complete: bool,
    pub limitations: Vec<String>,
}
fn key(o: &ResearchObjectRef) -> String {
    format!("{}:{}:{}", o.kind, o.id, o.revision)
}
fn exact(
    store: &Store,
    ws: &str,
    kind: &str,
    id: &str,
) -> WorkbenchResult<Option<ResearchObjectRef>> {
    store.connection()?.query_row("SELECT revision FROM research_search_sources WHERE workspace_id=?1 AND kind=?2 AND id=?3 ORDER BY length(revision) DESC,revision DESC LIMIT 1",params![ws,kind,id],|r|Ok(ResearchObjectRef{kind:kind.into(),id:id.into(),revision:r.get(0)?,start:None,end:None})).optional().map_err(err)
}
/// Project links explicitly represented by host-owned fields; free prose is not a dependency.
fn projected(store: &Store, ws: &str) -> WorkbenchResult<Vec<Relation>> {
    let mut out = Vec::new();
    let c = store.connection()?;
    let mut q=c.prepare("SELECT id,revision,kind,body_json FROM project_records WHERE workspace_id=?1 AND kind IN ('binding','response','theory','check','literature','specification','build','experiment') LIMIT 1000").map_err(err)?;
    let rows = q
        .query_map([ws], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    for (id, revision, kind, body) in rows {
        let mut body: Value = serde_json::from_str(&body).map_err(err)?;
        if kind == "response" {
            body = body["decision"].clone();
        }
        let accepted = kind != "binding" || body["confirmed"] == true;
        let dependent = ResearchObjectRef {
            kind: "record".into(),
            id,
            revision: revision.to_string(),
            start: None,
            end: None,
        };
        for (field, target) in [
            ("anchorId", "record"),
            ("sourceVersionId", "source"),
            ("executionId", "execution"),
            ("baselineExecutionId", "execution"),
            ("manuscriptRevisionId", "paper"),
            ("noteId", "record"),
            ("theoryId", "record"),
            ("taskId", "record"),
        ] {
            if let Some(id) = body[field].as_str() {
                if let Some(input) = exact(store, ws, target, id)? {
                    out.push(Relation {
                        input,
                        dependent: dependent.clone(),
                        origin: "host_reference".into(),
                        accepted,
                        reason: format!("{kind}.{field}"),
                    });
                }
            }
        }
        for field in [
            "assumptionIds",
            "relatedIds",
            "anchorIds",
            "evidenceAnchorIds",
            "executionIds",
        ] {
            for id in body[field]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if let Some(input) = exact(
                    store,
                    ws,
                    if field == "executionIds" {
                        "execution"
                    } else {
                        "record"
                    },
                    id,
                )? {
                    out.push(Relation {
                        input,
                        dependent: dependent.clone(),
                        origin: "host_reference".into(),
                        accepted,
                        reason: format!("{kind}.{field}"),
                    });
                }
            }
        }
        if let Some(id) = body["result"]["executionId"].as_str() {
            if let Some(input) = exact(store, ws, "execution", id)? {
                out.push(Relation {
                    input,
                    dependent: dependent.clone(),
                    origin: "host_reference".into(),
                    accepted,
                    reason: "Result binding execution".into(),
                });
            }
        }
    }
    for kind in [
        "sample",
        "decision",
        "publication_asset",
        "asset_inclusion",
        "deliverable",
        "symbol",
        "assumption_branch",
        "revision_campaign",
        "coauthor_review",
        "deliverable_role",
    ] {
        for record in desk::records(store, ws, kind)? {
            let field = if kind == "sample" {
                "datasets"
            } else if kind == "decision" {
                "assumptions"
            } else {
                "sources"
            };
            for value in record.body[field].as_array().into_iter().flatten() {
                if let Ok(input) = serde_json::from_value(value.clone()) {
                    out.push(Relation {
                        input,
                        dependent: record.reference(),
                        origin: "imported_declaration".into(),
                        accepted: kind != "decision" || record.body["state"] == "accepted",
                        reason: format!("Declared {field}"),
                    });
                }
            }
        }
    }
    let mut executions=c.prepare("SELECT id,created_at,input_manifest_json FROM research_executions WHERE workspace_id=?1 LIMIT 1000").map_err(err)?;
    let rows = executions
        .query_map([ws], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    for (id, revision, manifest) in rows {
        let manifest: Value = serde_json::from_str(&manifest).map_err(err)?;
        for r in manifest["researchInputs"].as_array().into_iter().flatten() {
            if let Ok(input) = serde_json::from_value(r.clone()) {
                out.push(Relation {
                    input,
                    dependent: ResearchObjectRef {
                        kind: "execution".into(),
                        id: id.clone(),
                        revision: revision.clone(),
                        start: None,
                        end: None,
                    },
                    origin: "host_reference".into(),
                    accepted: true,
                    reason: "Captured plan research input".into(),
                });
            }
        }
    }
    let mut evidence=c.prepare("SELECT claim_version_id,target_type,target_id,assessment,relation FROM evidence_links WHERE workspace_id=?1 LIMIT 1000").map_err(err)?;
    let links = evidence
        .query_map([ws], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    for (claim, kind, target, assessment, relation) in links {
        let kind = match kind.as_str() {
            "paper_revision" => "paper",
            "source_version" => "source",
            other => other,
        };
        if let (Some(input), Some(dependent)) = (
            exact(store, ws, kind, &target)?,
            exact(store, ws, "claim", &claim)?,
        ) {
            out.push(Relation {
                input,
                dependent,
                origin: format!("evidence / {assessment}"),
                accepted: matches!(assessment.as_str(), "human_confirmed" | "check_passed"),
                reason: format!("Claim evidence: {relation}"),
            });
        }
    }
    let mut results=c.prepare("SELECT sr.id,sr.created_at,e.id,e.created_at FROM structured_results sr JOIN research_executions e ON e.id=sr.execution_id WHERE e.workspace_id=?1 LIMIT 1000").map_err(err)?;
    for row in results
        .query_map([ws], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(err)?
    {
        let (id, revision, run, run_revision) = row.map_err(err)?;
        out.push(Relation {
            input: ResearchObjectRef {
                kind: "execution".into(),
                id: run,
                revision: run_revision,
                start: None,
                end: None,
            },
            dependent: ResearchObjectRef {
                kind: "result".into(),
                id,
                revision,
                start: None,
                end: None,
            },
            origin: "host_reference".into(),
            accepted: true,
            reason: "Result adopted from this execution".into(),
        });
    }
    Ok(out)
}
fn changed(
    store: &Store,
    ws: &str,
    o: &ResearchObjectRef,
    started: std::time::Instant,
    read: &mut usize,
) -> WorkbenchResult<Option<(String, String)>> {
    let c = store.connection()?;
    let status:Option<String>=match o.kind.as_str(){
        "paper"=>c.query_row("SELECT CASE WHEN p.current_revision_id<>pr.id THEN 'passage_moved' END FROM paper_revisions pr JOIN papers p ON p.id=pr.paper_id WHERE pr.id=?1 AND p.workspace_id=?2",params![o.id,ws],|r|r.get(0)).optional().map_err(err)?.flatten(),
        "source"=>c.query_row("SELECT CASE WHEN v.access_state='unavailable' THEN 'source_unavailable' WHEN EXISTS(SELECT 1 FROM source_versions n WHERE n.source_id=v.source_id AND n.created_at>v.created_at) THEN 'review_needed' END FROM source_versions v JOIN sources s ON s.id=v.source_id WHERE v.id=?1 AND s.workspace_id=?2",params![o.id,ws],|r|r.get(0)).optional().map_err(err)?.flatten(),
        "note"=>c.query_row("SELECT CASE WHEN CAST(revision AS TEXT)<>?3 THEN 'input_changed' END FROM research_notes WHERE id=?1 AND workspace_id=?2",params![o.id,ws,o.revision],|r|r.get(0)).optional().map_err(err)?.flatten(),
        "record"=>c.query_row("SELECT CASE WHEN CAST(revision AS TEXT)<>?3 THEN 'input_changed' WHEN kind='check' AND json_extract(body_json,'$.outcome')='failed' THEN 'check_failed' END FROM project_records WHERE id=?1 AND workspace_id=?2",params![o.id,ws,o.revision],|r|r.get(0)).optional().map_err(err)?.flatten(),
        "execution"=>c.query_row("SELECT CASE WHEN outcome='failed' THEN 'check_failed' WHEN outcome='outcome_unknown' THEN 'unknown' END FROM research_executions WHERE id=?1 AND workspace_id=?2",params![o.id,ws],|r|r.get(0)).optional().map_err(err)?.flatten(),
        _=>{let newer:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM desk_records WHERE supersedes=?1 AND workspace_id=?2)",params![o.id,ws],|r|r.get(0)).map_err(err)?;newer.then(||"input_changed".into())}
    };
    if status.is_none() && o.kind == "execution" {
        let e = crate::workbench::research::get_execution(store, &o.id)?;
        let original = e.input_manifest["planId"]
            .as_str()
            .and_then(|id| desk::record(store, ws, id).ok())
            .and_then(|p| p.body["profile"]["cwd"].as_str().map(str::to_string));
        let cwd = std::path::Path::new(original.as_deref().unwrap_or(&e.cwd));
        if let Ok(root) = super::files::SafeRoot::open(cwd) {
            for file in e.input_manifest["files"].as_array().into_iter().flatten() {
                let path = file["path"].as_str().unwrap_or("");
                if original.is_some() && path == "pipeline-parameters.json" {
                    continue;
                }
                if started.elapsed() > std::time::Duration::from_secs(2) || *read > 32 * 1024 * 1024
                {
                    return Ok(Some((
                        "unknown".into(),
                        "Input inspection reached its time or byte limit".into(),
                    )));
                }
                match root.read(path) {
                    Ok(bytes) => {
                        *read += bytes.len();
                        if Some(desk::hash(&bytes)).as_deref() != file["contentHash"].as_str() {
                            return Ok(Some((
                                "input_changed".into(),
                                format!("Declared input {path} changed since this execution"),
                            )));
                        }
                    }
                    Err(_) => {
                        return Ok(Some((
                            "unknown".into(),
                            format!("Declared input {path} is unavailable"),
                        )))
                    }
                }
            }
        } else {
            return Ok(Some((
                "unknown".into(),
                "Execution working directory is unavailable".into(),
            )));
        }
    }
    Ok(status.map(|s| {
        let reason = match s.as_str() {
            "passage_moved" => {
                "A later manuscript revision exists; remap and inspect this exact passage."
            }
            "input_changed" => "The referenced input has a newer version.",
            "check_failed" => "A linked check failed; inspect its receipt.",
            "source_unavailable" => "The source is unavailable.",
            "unknown" => "The linked run has an unknown outcome.",
            _ => "A source has changed; researcher review is required.",
        };
        (s, reason.into())
    }))
}
pub fn impact(store: &Store, ws: &str) -> WorkbenchResult<ImpactReport> {
    store.workspace(ws)?;
    let mut relations = desk::records(store, ws, "relation")?
        .into_iter()
        .map(|r| serde_json::from_value::<Relation>(r.body).map_err(err))
        .collect::<WorkbenchResult<Vec<_>>>()?;
    relations.extend(projected(store, ws)?);
    let started = std::time::Instant::now();
    let mut bytes_read = 0;
    let mut queue = VecDeque::new();
    let mut seen = HashSet::new();
    for r in &relations {
        for o in [&r.input, &r.dependent] {
            if seen.insert(key(o)) {
                if let Some((status, reason)) = changed(store, ws, o, started, &mut bytes_read)? {
                    queue.push_back(Impact {
                        object: o.clone(),
                        status,
                        reason,
                        path: vec![o.clone()],
                    });
                }
            }
        }
    }
    let mut visited = HashSet::new();
    let mut impacts = Vec::new();
    let mut complete = relations.len() < 2000;
    while let Some(item) = queue.pop_front() {
        if !visited.insert(key(&item.object)) {
            continue;
        }
        if impacts.len() >= 500 {
            complete = false;
            break;
        }
        for r in relations
            .iter()
            .filter(|r| r.accepted && key(&r.input) == key(&item.object))
        {
            if item.path.len() >= 16 {
                complete = false;
                continue;
            }
            let mut path = item.path.clone();
            path.push(r.dependent.clone());
            queue.push_back(Impact {
                object: r.dependent.clone(),
                status: "review_needed".into(),
                reason: format!("{} via {}", item.reason, r.reason),
                path,
            });
        }
        impacts.push(item);
    }
    Ok(ImpactReport{impacts,relations,complete,limitations:vec!["Only explicit and host-declared links are traversed. Unlinked work has unknown coverage.".into(),"A changed assumption requires rechecking; it does not establish a false proposition.".into()]})
}
pub fn handoff(store: &Store, session: &str, operation: &str) -> WorkbenchResult<DeskRecord> {
    let snapshot = store.session_snapshot(session)?;
    let ws = snapshot
        .session
        .workspace_id
        .ok_or_else(|| WorkbenchError::invalid("Select a project"))?;
    let home = super::home(store, &ws)?;
    let notes = super::research::list_notes(store, &ws, false)?;
    let proposals = notes
        .into_iter()
        .filter(|n| n.state == "proposed")
        .take(30)
        .collect::<Vec<_>>();
    desk::insert(
        store,
        &ws,
        "handoff",
        "Session handoff draft",
        json!({"sessionId":session,"state":"draft","decisions":proposals,"unresolved":home.tasks,"outputs":home.executions.into_iter().filter(|e|e.created_at>=snapshot.session.created_at).take(30).collect::<Vec<_>>(),"contextPreview":home.context_preview,"limitations":["Host-generated inventory, not an accepted summary. Accept individual proposed notes or create next actions explicitly."]}),
        None,
        operation,
    )
}

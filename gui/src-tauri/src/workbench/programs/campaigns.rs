use super::*;
use crate::workbench::{project, research, search};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Comment {
    pub response: ResearchObjectRef,
    pub required_outputs: Vec<ResearchObjectRef>,
    pub researcher_addressed: bool,
    pub judgment: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Campaign {
    pub round: u32,
    pub manuscript: ResearchObjectRef,
    pub comments: Vec<Comment>,
    pub change_summary: String,
    pub review_scope: String,
    pub broad_change: bool,
    pub previous_round: Option<String>,
}
pub fn save(
    store: &Store,
    ws: &str,
    title: &str,
    c: Campaign,
    supersedes: Option<&str>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    if c.round == 0
        || c.round > 100
        || c.comments.is_empty()
        || c.comments.len() > 100
        || c.manuscript.kind != "paper"
    {
        return Err(WorkbenchError::invalid(
            "Choose a manuscript revision and 1–100 comments for this round",
        ));
    }
    exact(store, ws, &c.manuscript)?;
    bounded(&c.change_summary, 16000)?;
    bounded(&c.review_scope, 16000)?;
    if let Some(previous) = &c.previous_round {
        let old = desk::record(store, ws, previous)?;
        if old.kind != "revision_campaign" {
            return Err(WorkbenchError::invalid("Select a previous campaign"));
        }
        let p = decode(&old)?;
        if p.round >= c.round {
            return Err(WorkbenchError::invalid(
                "A previous round must have a lower round number",
            ));
        }
    }
    for comment in &c.comments {
        let _ = response(store, ws, &comment.response)?;
        refs(store, ws, &comment.required_outputs, 20)?;
        if comment.researcher_addressed {
            bounded(&comment.judgment, 8000)?;
        }
    }
    let mut body = serde_json::to_value(&c).map_err(err)?;
    let mut sources = vec![c.manuscript];
    for comment in &c.comments {
        sources.push(comment.response.clone());
        sources.extend(comment.required_outputs.clone());
    }
    body["sources"] = json!(sources);
    // Campaign has an explicit sources projection in storage, stripped when decoding below.
    desk::insert(
        store,
        ws,
        "revision_campaign",
        title,
        body,
        supersedes,
        operation,
    )
}
fn decode(record: &DeskRecord) -> WorkbenchResult<Campaign> {
    let mut v = record.body.clone();
    v.as_object_mut().unwrap().remove("sources");
    serde_json::from_value(v).map_err(err)
}
fn response(
    store: &Store,
    ws: &str,
    r: &ResearchObjectRef,
) -> WorkbenchResult<project::ResponseRecord> {
    if r.kind != "record" || r.start.is_some() || r.end.is_some() {
        return Err(WorkbenchError::invalid(
            "Choose a complete response revision",
        ));
    }
    let v = search::read_object(store, ws, r, 64 * 1024)?;
    if v.truncated {
        return Err(WorkbenchError::invalid("Response exceeds 64 KiB"));
    }
    serde_json::from_str(&v.text).map_err(err)
}
pub fn completion(store: &Store, ws: &str, id: &str) -> WorkbenchResult<Value> {
    let record = desk::record(store, ws, id)?;
    if record.kind != "revision_campaign" {
        return Err(WorkbenchError::invalid("Select a revision campaign"));
    }
    let c = decode(&record)?;
    let impact = project::relations::impact(store, ws)?;
    let manuscript_state = freshness(store, ws, &c.manuscript);
    let mut rows = Vec::new();
    for comment in &c.comments {
        let r = response(store, ws, &comment.response)?;
        let d = &r.decision;
        let mut flags = project::response_flags(store, ws, d)?;
        if manuscript_state != "current" {
            flags.push(format!(
                "Campaign manuscript: {manuscript_state}; this round retains its selected version"
            ));
        }
        if let Some(execution) = &d.execution_id {
            for changed in impact
                .impacts
                .iter()
                .filter(|i| i.object.kind == "execution" && &i.object.id == execution)
            {
                flags.push(format!(
                    "Linked execution requires review: {}",
                    changed.reason
                ));
            }
        }

        let task = d
            .task_id
            .as_ref()
            .map(|id| project::record(store, ws, id, "task"))
            .transpose()?;
        let executed = d
            .execution_id
            .as_ref()
            .map(|id| research::get_execution(store, id))
            .transpose()?;
        if executed.as_ref().is_some_and(|e| e.workspace_id != ws) {
            return Err(WorkbenchError::invalid(
                "Execution belongs to another project",
            ));
        }
        let application = d
            .application_id
            .as_ref()
            .map(|id| project::record(store, ws, id, "application"))
            .transpose()?;
        let mut accepted = application
            .as_ref()
            .is_some_and(|a| a.body["state"] == "applied");
        if let Some(a) = &application {
            for file in a.body["files"].as_array().into_iter().flatten() {
                let path = file["path"].as_str().unwrap_or("");
                let now = project::program_file_hash(store, ws, path);
                if now.ok().flatten() != file["after"].as_str().map(str::to_owned) {
                    accepted = false;
                    flags.push(format!("Accepted file changed or is unavailable: {path}"));
                }
            }
        }
        let mut outputs = Vec::new();
        for source in &comment.required_outputs {
            let state = impact
                .impacts
                .iter()
                .find(|i| {
                    i.object.kind == source.kind
                        && i.object.id == source.id
                        && i.object.revision == source.revision
                })
                .map(|i| i.status.clone())
                .unwrap_or_else(|| freshness(store, ws, source));
            if state != "current" {
                flags.push(format!("Required {} {}: {state}", source.kind, source.id));
            }
            outputs.push(json!({"source":source,"state":state}));
        }
        let response_state = freshness(store, ws, &comment.response);
        if response_state != "current" {
            flags.push(
                "Response has a later revision; this round retains the selected version".into(),
            );
        }
        rows.push(json!({"response":comment.response,"number":d.number,"draft":d.draft,"disposition":d.disposition,"source":r.source,"taskDone":task.is_some_and(|t|t.body["status"]=="completed"),"fileAcceptedAndCurrent":accepted,"checkExecuted":executed.is_some_and(|e|e.outcome=="completed"),"requiredOutputs":outputs,"linkChecksPass":flags.is_empty(),"flags":flags,"researcherAddressed":comment.researcher_addressed,"judgment":comment.judgment}));
    }
    Ok(
        json!({"record":record,"manuscriptState":manuscript_state,"impactCoverageComplete":impact.complete,"comments":rows,"reviewRecommendation":if c.broad_change{"Prepare a full manuscript Review: this round declares broad changes"}else{"Preview a scoped Review using the exact manuscript and selected responses"},"notice":"Completion checks verify links and file state; the researcher judges substantive adequacy."}),
    )
}
pub fn freshness(store: &Store, ws: &str, r: &ResearchObjectRef) -> String {
    if exact(store, ws, r).is_err() {
        return "unavailable".into();
    }
    let current:WorkbenchResult<Option<String>>=match r.kind.as_str(){
        "paper"=>{let latest=store.connection().and_then(|c|c.query_row("SELECT p.current_revision_id FROM papers p JOIN paper_revisions r ON r.paper_id=p.id WHERE p.workspace_id=?1 AND r.id=?2",params![ws,r.id],|row|row.get::<_,Option<String>>(0)).map_err(err)).ok().flatten();return if latest.as_deref()==Some(r.id.as_str()){"current"}else{"superseded"}.into();},
        "record"=>store.connection().and_then(|c|c.query_row("SELECT CAST(revision AS TEXT) FROM project_records WHERE workspace_id=?1 AND id=?2",params![ws,r.id],|row|row.get(0)).optional().map_err(err)),
        "note"=>store.connection().and_then(|c|c.query_row("SELECT CAST(revision AS TEXT) FROM research_notes WHERE workspace_id=?1 AND id=?2",params![ws,r.id],|row|row.get(0)).optional().map_err(err)),
        _=>{let superseded=store.connection().and_then(|c|c.query_row("SELECT EXISTS(SELECT 1 FROM desk_records WHERE workspace_id=?1 AND supersedes=?2)",params![ws,r.id],|row|row.get::<_,bool>(0)).map_err(err)).unwrap_or(false);return if superseded{"superseded"}else{"current"}.into();}
    };
    if current.ok().flatten().as_deref() == Some(&r.revision) {
        "current"
    } else {
        "changed"
    }
    .into()
}
pub fn review(
    store: &Store,
    ws: &str,
    id: &str,
    anchors: Vec<String>,
    dependencies: Vec<String>,
) -> WorkbenchResult<Value> {
    let record = desk::record(store, ws, id)?;
    if record.kind != "revision_campaign" {
        return Err(WorkbenchError::invalid("Select a revision campaign"));
    }
    let c = decode(&record)?;
    for comment in &c.comments {
        if freshness(store, ws, &comment.response) != "current" {
            return Err(WorkbenchError::conflict("A selected response has changed. Save a new campaign version before preparing its re-review."));
        }
    }
    for id in &anchors {
        let a = project::record(store, ws, id, "anchor")?;
        if a.body["selection"]["revisionId"] != c.manuscript.id {
            return Err(WorkbenchError::invalid(
                "Select changed passages from this campaign's exact manuscript revision",
            ));
        }
    }
    Ok(json!(project::focus_review(
        store,
        project::FocusReviewRequest {
            workspace_id: ws.into(),
            anchor_ids: anchors,
            dependency_anchor_ids: dependencies,
            response_ids: c.comments.iter().map(|c| c.response.id.clone()).collect()
        }
    )?))
}

//! Explicit one-turn queue. No automatic transport on reconnect or process startup.
use super::*;
use crate::workbench::{commands as host, research, tasks};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub text: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub binding: tasks::TaskBinding,
    pub context: desk::ContextSelection,
    pub turn_budget: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextReview {
    pub id: String,
    pub revision: i64,
    pub request: Request,
    pub previous_context: desk::ContextSelection,
    pub previous_root: String,
    pub conversation_title: String,
    pub workspace_name: Option<String>,
    pub conversation_changed: bool,
    pub settings_changed: bool,
    pub settings: Value,
    pub fingerprint: String,
}

pub fn review_context(
    store: &Store,
    session: &str,
    id: &str,
    revision: i64,
) -> WorkbenchResult<ContextReview> {
    let c = store.connection()?;
    let body: String = c.query_row(
        "SELECT request_json FROM research_followups WHERE session_id=?1 AND id=?2 AND revision=?3 AND state='queued'",
        params![session, id, revision], |r| r.get(0),
    ).optional().map_err(err)?.ok_or_else(|| WorkbenchError::conflict("Only an unchanged, unsubmitted follow-up can have its context refreshed"))?;
    let mut request: Request = serde_json::from_str(&body).map_err(err)?;
    let previous_binding = request.binding;
    let previous_context = request.context;
    request.binding = tasks::binding(store, session)?;
    request.context = desk::context(store, session)?;
    tasks::validate_binding(store, &request.binding)?;
    let snapshot = store.session_snapshot(session)?;
    let effective = research::resolve_harness(store, session)?;
    if effective.fingerprint != request.binding.harness_fingerprint {
        return Err(WorkbenchError::conflict(
            "The research settings changed during review; review the context again",
        ));
    }
    let settings = json!({"preset":effective.preset.name,"permissions":effective.permission_profile,
        "commandNetwork":effective.command_network,"modules":effective.enabled_modules,"instructions":effective.preset.instructions});
    let fingerprint = desk::hash(
        serde_json::to_string(&(id, revision, &request, &settings))
            .map_err(err)?
            .as_bytes(),
    );
    Ok(ContextReview {
        id: id.into(),
        revision,
        conversation_changed: previous_binding.cursor != request.binding.cursor,
        settings_changed: previous_binding.harness_fingerprint
            != request.binding.harness_fingerprint,
        previous_root: previous_binding.runtime_root,
        previous_context,
        conversation_title: snapshot.session.title,
        workspace_name: snapshot.workspace.map(|w| w.name),
        request,
        settings,
        fingerprint,
    })
}

pub fn refresh_context(
    store: &Store,
    session: &str,
    id: &str,
    revision: i64,
    fingerprint: &str,
) -> WorkbenchResult<Value> {
    let review = review_context(store, session, id, revision)?;
    if review.fingerprint != fingerprint {
        return Err(WorkbenchError::conflict("The conversation or selected context changed after the preview. Review it again before refreshing."));
    }
    let body = serde_json::to_string(&review.request).map_err(err)?;
    let changed = store.connection()?.execute(
        "UPDATE research_followups SET request_json=?4,preceding_turn=?5,scope_hash=?6,revision=revision+1,updated_at=?7
         WHERE session_id=?1 AND id=?2 AND revision=?3 AND state='queued'",
        params![session, id, revision, body, review.request.binding.cursor, desk::hash(body.as_bytes()), desk::now()],
    ).map_err(err)?;
    if changed != 1 {
        return Err(WorkbenchError::conflict(
            "Follow-up changed; review it again",
        ));
    }
    list(store, session)
}
pub fn enqueue(
    store: &Store,
    session: &str,
    text: &str,
    model: Option<String>,
    effort: Option<String>,
    operation: &str,
) -> WorkbenchResult<Value> {
    bounded(text, 64000)?;
    bounded(operation, 120)?;
    let binding = tasks::binding(store, session)?;
    let context = desk::context(store, session)?;
    let request = Request {
        text: text.into(),
        model,
        effort,
        binding: binding.clone(),
        context,
        turn_budget: 1,
    };
    let body = serde_json::to_string(&request).map_err(err)?;
    let id = format!(
        "followup_{}",
        desk::hash(format!("{session}:{operation}").as_bytes())
    );
    let scope = desk::hash(body.as_bytes());
    let c = store.connection()?;
    if let Some(old) = c
        .query_row(
            "SELECT request_json FROM research_followups WHERE id=?1",
            [&id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(err)?
    {
        if old != body {
            return Err(WorkbenchError::conflict(
                "Queue operation was reused with changed content",
            ));
        }
        return list(store, session);
    }
    let count:i64=c.query_row("SELECT COUNT(*) FROM research_followups WHERE session_id=?1 AND state IN ('queued','dispatching','running','attention')",[session],|r|r.get(0)).map_err(err)?;
    if count >= 20 {
        return Err(WorkbenchError::invalid(
            "At most 20 pending follow-ups per conversation",
        ));
    }
    c.execute("INSERT INTO research_followups(id,session_id,position,request_json,state,preceding_turn,scope_hash,operation_id,created_at,updated_at) SELECT ?1,?2,COALESCE(MAX(position),0)+1,?3,'queued',?4,?5,?6,?7,?7 FROM research_followups WHERE session_id=?2",params![id,session,body,binding.cursor,scope,format!("followup-{}",desk::hash(id.as_bytes())),desk::now()]).map_err(err)?;
    list(store, session)
}
pub fn list(store: &Store, session: &str) -> WorkbenchResult<Value> {
    list_history(store, session, 0)
}
pub fn list_history(store: &Store, session: &str, history_offset: u32) -> WorkbenchResult<Value> {
    store.session_snapshot(session)?;
    let c = store.connection()?;
    let mut q=c.prepare("SELECT id,position,revision,request_json,state,scope_hash,result_json FROM research_followups
        WHERE session_id=?1 AND (state IN ('queued','dispatching','running','attention') OR id IN (
            SELECT id FROM research_followups WHERE session_id=?1 AND state IN ('completed','failed','cancelled')
            ORDER BY position DESC,id DESC LIMIT 100 OFFSET ?2))
        ORDER BY CASE WHEN state IN ('queued','dispatching','running','attention') THEN 0 ELSE 1 END,
            CASE WHEN state IN ('queued','dispatching','running','attention') THEN position END,
            position DESC,id DESC").map_err(err)?;
    let rows=q.query_map(params![session, history_offset],|r|Ok(json!({"id":r.get::<_,String>(0)?,"position":r.get::<_,i64>(1)?,"revision":r.get::<_,i64>(2)?,"request":serde_json::from_str::<Value>(&r.get::<_,String>(3)?).unwrap_or(Value::Null),"state":r.get::<_,String>(4)?,"fingerprint":r.get::<_,String>(5)?,"result":r.get::<_,Option<String>>(6)?.and_then(|s|serde_json::from_str::<Value>(&s).ok())}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    Ok(json!(rows))
}
pub fn control(
    store: &Store,
    session: &str,
    id: &str,
    revision: i64,
    action: &str,
) -> WorkbenchResult<Value> {
    store.session_snapshot(session)?;
    let mut c = store.connection()?;
    let tx = c.transaction().map_err(err)?;
    let (state,pos):(String,i64)=tx.query_row("SELECT state,position FROM research_followups WHERE session_id=?1 AND id=?2 AND revision=?3",params![session,id,revision],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
    if state != "queued" {
        return Err(WorkbenchError::conflict("Only unsubmitted follow-ups can be reordered or cancelled; use the owning turn's Stop control for running work"));
    }
    if action == "cancel" {
        tx.execute("UPDATE research_followups SET state='cancelled',revision=revision+1,updated_at=?2 WHERE id=?1",params![id,desk::now()]).map_err(err)?;
    } else if ["up", "down"].contains(&action) {
        let query = if action == "up" {
            "SELECT id,position FROM research_followups WHERE session_id=?1 AND state='queued' AND position<?2 ORDER BY position DESC LIMIT 1"
        } else {
            "SELECT id,position FROM research_followups WHERE session_id=?1 AND state='queued' AND position>?2 ORDER BY position LIMIT 1"
        };
        if let Some((other, p)) = tx
            .query_row(query, params![session, pos], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .optional()
            .map_err(err)?
        {
            tx.execute(
                "UPDATE research_followups SET position=?2,revision=revision+1 WHERE id=?1",
                params![id, p],
            )
            .map_err(err)?;
            tx.execute(
                "UPDATE research_followups SET position=?2,revision=revision+1 WHERE id=?1",
                params![other, pos],
            )
            .map_err(err)?;
        }
    } else {
        return Err(WorkbenchError::invalid("Unknown follow-up control"));
    }
    tx.commit().map_err(err)?;
    list(store, session)
}
pub fn prepare_dispatch(
    store: &Store,
    session: &str,
    id: &str,
    fingerprint: &str,
) -> WorkbenchResult<(host::SendTurnRequest, tasks::TaskBinding)> {
    let c = store.connection()?;
    let (body,scope,state,operation):(String,String,String,String)=c.query_row("SELECT request_json,scope_hash,state,operation_id FROM research_followups WHERE session_id=?1 AND id=?2",params![session,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(err)?;
    if state != "queued" || scope != fingerprint {
        return Err(WorkbenchError::conflict(
            "Follow-up is no longer queued or its reviewed context changed",
        ));
    }
    let r: Request = serde_json::from_str(&body).map_err(err)?;
    tasks::validate_binding(store, &r.binding)?;
    if !host::task_turn_available() {
        return Err(WorkbenchError::conflict(
            "Wait for the active Workspace turn",
        ));
    }
    c.execute("UPDATE research_followups SET state='dispatching',revision=revision+1,updated_at=?2 WHERE id=?1 AND state='queued'",params![id,desk::now()]).map_err(err)?;
    Ok((
        host::SendTurnRequest {
            session_id: session.into(),
            text: r.text,
            model: r.model,
            effort: r.effort,
            client_submission_id: operation,
        },
        r.binding,
    ))
}
pub fn reconcile(
    store: &Store,
    session: &str,
    id: &str,
    transport: Option<Value>,
) -> WorkbenchResult<Value> {
    let c = store.connection()?;
    let (operation, state): (String, String) = c
        .query_row(
            "SELECT operation_id,state FROM research_followups WHERE session_id=?1 AND id=?2",
            params![session, id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(err)?;
    if ["dispatching", "running", "attention"].contains(&state.as_str()) {
        let outcome = tasks::outcome(store, session, &operation)?;
        let next = match outcome.as_ref().and_then(|v| v["state"].as_str()) {
            Some("completed") => "completed",
            Some("failed" | "interrupted" | "cancelled") => "failed",
            Some("running") => "running",
            _ => "attention",
        };
        c.execute("UPDATE research_followups SET state=?2,result_json=?3,revision=revision+1,updated_at=?4 WHERE id=?1",params![id,next,json!({"receipt":outcome,"transport":transport,"notice":"An uncertain submission is never automatically retried. Inspect the owning conversation before creating any replacement request."}).to_string(),desk::now()]).map_err(err)?;
    }
    list(store, session)
}
pub fn branch(
    store: &Store,
    session: &str,
    title: &str,
    items: Vec<desk::ContextItem>,
    operation: &str,
) -> WorkbenchResult<Value> {
    let old = store.session_snapshot(session)?;
    let ws = old
        .session
        .workspace_id
        .ok_or_else(|| WorkbenchError::invalid("Choose a project conversation"))?;
    if items.len() > 12 || items.iter().filter(|i| i.role == "main").count() > 1 {
        return Err(WorkbenchError::invalid(
            "A context branch permits at most 12 sources and one main object",
        ));
    }
    let mut selected = std::collections::HashSet::new();
    for item in &items {
        if ![
            "main",
            "source",
            "data_dictionary",
            "prior_draft",
            "referee_report",
            "result",
            "supporting",
        ]
        .contains(&item.role.as_str())
            || !selected.insert(serde_json::to_string(&item.object).map_err(err)?)
        {
            return Err(WorkbenchError::invalid(
                "Context branch contains an invalid role or duplicate object",
            ));
        }
    }
    if items.is_empty() {
        return Err(WorkbenchError::invalid(
            "Select at least one retained context object",
        ));
    }
    for i in &items {
        if i.object.kind == "message" {
            return Err(WorkbenchError::invalid("Context branches use selected research objects; they do not copy conversation history"));
        }
        exact(store, &ws, &i.object)?;
    }
    let created = store.create_session(super::super::store::CreateSessionRequest {
        workspace_id: Some(ws),
        title: format!("Context branch: {title}"),
        operation_id: operation.into(),
    })?;
    desk::save_context(store, &created.record.id, 0, items)?;
    Ok(
        json!({"session":created.record,"sourceSessionId":session,"notice":"New conversation from selected retained context. History, source-root write grants, and task bindings were not copied."}),
    )
}

//! Workspace-owned task binding and turn inspection. Database calls use commands::run_store.
use super::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
pub(crate) mod activity;
pub(crate) mod delivery;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskBinding {
    pub session_id: String,
    pub workspace_id: Option<String>,
    pub cursor: String,
    pub harness_fingerprint: String,
    #[serde(default)]
    pub connection_id: Option<crate::connections::ConnectionId>,
    pub runtime_root: String,
    pub root_identity: String,
    /// What a schedule holds its later occurrences to. Never serialized:
    /// queued follow-ups embed this record and hash their saved form.
    #[serde(skip)]
    pub permission_profile: Option<String>,
    #[serde(skip)]
    pub preset_id: Option<String>,
    #[serde(skip)]
    pub access: Option<String>,
}
fn error(e: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::storage("Task conversation", e)
}
fn canonical_state(state: &str) -> &str {
    if state == "inProgress" {
        "running"
    } else {
        state
    }
}
pub(crate) fn compatible_cursor(expected: &str, actual: &str) -> bool {
    if expected == actual {
        return true;
    }
    match (expected.rsplit_once(':'), actual.rsplit_once(':')) {
        (Some((e, es)), Some((a, as_))) if e == a => {
            let es = canonical_state(es);
            let as_ = canonical_state(as_);
            es == as_ || (es == "running" && as_ == "completed")
        }
        _ => false,
    }
}
pub fn cursor(store: &Store, session: &str) -> WorkbenchResult<String> {
    let value:Option<(String,String)>=store.connection()?.query_row("SELECT t.id,t.state FROM turns t JOIN session_bindings b ON b.id=t.binding_id WHERE b.session_id=?1 ORDER BY t.created_at DESC,t.id DESC LIMIT 1",[session],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(error)?;
    Ok(value
        .map(|(id, state)| format!("{id}:{}", canonical_state(&state)))
        .unwrap_or_default())
}
pub fn is_task_turn(store: &Store, thread: &str, turn: &str) -> WorkbenchResult<bool> {
    store.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM turns t JOIN session_bindings b ON b.id=t.binding_id WHERE b.provider_thread_id=?1 AND t.provider_turn_id=?2 AND t.client_submission_id LIKE 'task-%')",params![thread,turn],|r|r.get(0)).map_err(error)
}
/// Everything in the harness that decides what a turn may do. The permission
/// profile alone is not enough: file tools follow the mode, and web search,
/// the browser and command sites sit outside it. Research context, prompts and
/// tool descriptions are left out because they move with ordinary project
/// activity and app updates.
pub(super) fn access(effective: &super::research::EffectiveHarness) -> String {
    let value = json!({"permission":effective.permission_profile,"mode":effective.mode,
        "fileMode":effective.file_mode,"webSearch":effective.web_search,
        "network":effective.command_network,"capabilities":effective.capabilities,
        "modules":effective.enabled_modules});
    super::desk::hash(value.to_string().as_bytes())
}
pub fn binding(store: &Store, session: &str) -> WorkbenchResult<TaskBinding> {
    let snapshot = store.session_snapshot(session)?;
    if snapshot.session.archived_at.is_some()
        || snapshot
            .workspace
            .as_ref()
            .is_some_and(|w| w.archived_at.is_some())
    {
        return Err(WorkbenchError::invalid("Task conversation is archived"));
    }
    let effective = super::research::resolve_harness(store, session)?;
    Ok(TaskBinding {
        session_id: session.into(),
        workspace_id: snapshot.session.workspace_id,
        cursor: cursor(store, session)?,
        access: Some(access(&effective)),
        harness_fingerprint: effective.fingerprint,
        permission_profile: Some(effective.permission_profile),
        preset_id: Some(effective.preset.id),
        connection_id: Some(
            crate::connections::ConnectionId::from_runtime(
                snapshot
                    .session
                    .overrides
                    .get("runtime")
                    .and_then(Value::as_str)
                    .unwrap_or("codex"),
            )
            .map_err(WorkbenchError::invalid)?,
        ),
        root_identity: super::store::root_identity(&store.runtime_root(session)?)?,
        runtime_root: store.runtime_root(session)?.to_string_lossy().into_owned(),
    })
}
/// The connection, project, folder, cursor and harness a step was prepared for.
/// Delivery stops here: it never touches the composer, so a draft can't affect it.
pub fn validate_context(store: &Store, expected: &TaskBinding) -> WorkbenchResult<()> {
    let actual = binding(store, &expected.session_id)?;
    if expected.connection_id.is_none() || actual.connection_id != expected.connection_id {
        return Err(WorkbenchError::conflict(
            "This automation's connection is missing or changed. Choose Use current conversation context before continuing.",
        ));
    }
    if actual.root_identity != expected.root_identity
        || !compatible_cursor(&expected.cursor, &actual.cursor)
        || actual.workspace_id != expected.workspace_id
        || actual.runtime_root != expected.runtime_root
        || actual.harness_fingerprint != expected.harness_fingerprint
    {
        return Err(WorkbenchError::conflict("The conversation, project, or research settings changed. Review the new context before continuing the automation."));
    }
    Ok(())
}
pub fn validate_binding(store: &Store, expected: &TaskBinding) -> WorkbenchResult<()> {
    validate_context(store, expected)?;
    if !store
        .session_snapshot(&expected.session_id)?
        .session
        .draft
        .is_empty()
    {
        return Err(WorkbenchError::conflict(
            "This conversation has an unsent draft. Send or clear it before continuing the automation.",
        ));
    }
    Ok(())
}
type SubmittedTurn = (String, String, String, Option<String>, Option<String>);
/// The most of one reply an automation step carries. The longest research
/// manuscript fits, with its fields, reviewer responses and JSON escaping.
const REPLY_LIMIT: usize = 512 * 1024;
/// Progress messages kept beside a separate final answer.
const PROGRESS_LIMIT: usize = 64 * 1024;
const REPLY_MESSAGES: usize = 100;
#[derive(Debug)]
struct TurnReply {
    text: String,
    final_text: Option<String>,
    /// Older messages were left out of `text`.
    truncated: bool,
    /// The answer itself is over the limit, so no step may use it as complete.
    too_long: bool,
}
fn submitted_turn(
    store: &Store,
    session: &str,
    operation: &str,
) -> WorkbenchResult<Option<SubmittedTurn>> {
    store.connection()?.query_row("SELECT t.id,t.state,b.provider_thread_id,t.provider_turn_id,t.error_json FROM turns t JOIN session_bindings b ON b.id=t.binding_id WHERE b.session_id=?1 AND t.client_submission_id=?2 ORDER BY b.incarnation DESC LIMIT 1",params![session,operation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(error)
}
fn ended(state: &str) -> bool {
    matches!(
        canonical_state(state),
        "completed" | "failed" | "interrupted" | "cancelled"
    )
}
/// True only while the submitted turn is open and its runtime still owns it.
/// A store row left open by a crash is not a running turn.
pub fn turn_running(store: &Store, session: &str, operation: &str) -> WorkbenchResult<bool> {
    Ok(
        submitted_turn(store, session, operation)?.is_some_and(|(_, state, thread, turn, _)| {
            !ended(&state)
                && turn.is_some_and(|turn| super::commands::turn_has_live_owner(&thread, &turn))
        }),
    )
}
/// The longest reason a step carries for a turn that did not finish.
const TURN_ERROR_LIMIT: usize = 1000;
/// Why a turn did not finish, as the researcher should read it. Codex records an
/// object that may name the reason; API and Claude turns record plain text.
fn turn_error(stored: &str) -> Option<String> {
    let value: Value = serde_json::from_str(stored).ok()?;
    let plain = match value["codexErrorInfo"].as_str() {
        Some("usageLimitExceeded") => {
            Some("The ChatGPT usage limit was reached. Retry after it resets.")
        }
        Some("unauthorized") => {
            Some("ChatGPT sign-in needs attention. Reconnect in Settings, then retry.")
        }
        _ => None,
    };
    let message = plain
        .or_else(|| value.as_str())
        .or_else(|| value["message"].as_str())?
        .trim();
    (!message.is_empty()).then(|| super::search::prefix(message, TURN_ERROR_LIMIT).to_string())
}
/// The newest turn in the conversation when it belongs to one of these task
/// operations: its cursor, and whether it has ended. A step checks the turn
/// before it, so nothing else has moved the conversation in between.
pub fn own_newest_turn(
    store: &Store,
    session: &str,
    operations: &[String],
) -> WorkbenchResult<Option<(String, bool)>> {
    let newest: Option<(String, String, Option<String>)> = store.connection()?.query_row("SELECT t.id,t.state,t.client_submission_id FROM turns t JOIN session_bindings b ON b.id=t.binding_id WHERE b.session_id=?1 ORDER BY t.created_at DESC,t.id DESC LIMIT 1",[session],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(error)?;
    Ok(newest.and_then(|(id, state, submission)| {
        let submission = submission?;
        let operation = submission.strip_prefix("task-")?;
        operations
            .iter()
            .any(|known| known == operation)
            .then(|| (format!("{id}:{}", canonical_state(&state)), ended(&state)))
    }))
}
pub fn outcome(store: &Store, session: &str, operation: &str) -> WorkbenchResult<Option<Value>> {
    let c = store.connection()?;
    let Some((id, state, thread, turn, failure)) = submitted_turn(store, session, operation)?
    else {
        return Ok(None);
    };
    let (items, older) = reply_items(&c, &id)?;
    let reply = turn_text(items)?;
    let answer = reply.final_text.as_deref().unwrap_or(&reply.text);
    let state = canonical_state(&state);
    let mut value = json!({"state":state,"threadId":thread,"turnId":turn,"text":reply.text,"finalText":reply.final_text,"answer":answer,"cursor":format!("{id}:{}", state),"truncated":older || reply.truncated});
    if reply.too_long {
        value["replyTooLong"] = json!(true);
    }
    if let Some(failure) = failure.as_deref().and_then(turn_error) {
        value["error"] = json!(failure);
    }
    Ok(Some(value))
}
/// A turn's newest messages, oldest first, and whether older ones were left out.
/// Items of one turn can share a timestamp, so the final answer is read on its
/// own rather than trusted to be among the newest.
fn reply_items(c: &rusqlite::Connection, turn: &str) -> WorkbenchResult<(Vec<String>, bool)> {
    const MESSAGES: &str = "SELECT id,payload_json FROM transcript_items WHERE turn_id=?1 AND is_final=1 AND lower(item_kind) LIKE '%agentmessage%'";
    let read = |sql: String, limit: usize| -> WorkbenchResult<Vec<(String, String)>> {
        let mut stmt = c.prepare(&sql).map_err(error)?;
        let rows = stmt
            .query_map(params![turn, limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error);
        rows
    };
    let mut items = read(
        format!("{MESSAGES} ORDER BY created_at DESC,id DESC LIMIT ?2"),
        REPLY_MESSAGES + 1,
    )?;
    let older = items.len() > REPLY_MESSAGES;
    items.truncate(REPLY_MESSAGES);
    items.reverse();
    let finals = read(
        format!("{MESSAGES} AND CASE WHEN json_valid(payload_json) THEN json_extract(payload_json,'$.phase') END='final_answer' ORDER BY created_at DESC,id DESC LIMIT ?2"),
        2,
    )?;
    for item in finals {
        if !items.iter().any(|(id, _)| *id == item.0) {
            items.push(item);
        }
    }
    Ok((items.into_iter().map(|(_, body)| body).collect(), older))
}
/// Keeps the newest messages. A long final answer travels on its own, so it is
/// not repeated in `text`, and an answer over the limit is reported, never cut.
fn turn_text(items: Vec<String>) -> WorkbenchResult<TurnReply> {
    let mut messages = Vec::new();
    for item in items {
        let v: Value = serde_json::from_str(&item).map_err(error)?;
        if let Some(s) = v.get("text").and_then(Value::as_str) {
            let last = v.get("phase").and_then(Value::as_str) == Some("final_answer");
            messages.push((s.to_string(), last));
        }
    }
    let mut finals = messages.iter().filter(|m| m.1);
    let final_text = match (finals.next(), finals.next()) {
        (Some((only, _)), None) => Some(only.clone()),
        _ => None,
    };
    let separate = final_text.is_some();
    let limit = if separate {
        PROGRESS_LIMIT
    } else {
        REPLY_LIMIT
    };
    let mut kept = Vec::new();
    let mut bytes = 0;
    let mut truncated = false;
    for (text, last) in messages.iter().rev() {
        if bytes + text.len() + 1 > limit {
            if separate && *last {
                continue;
            }
            truncated = true;
            break;
        }
        bytes += text.len() + 1;
        kept.push(text.as_str());
    }
    kept.reverse();
    let mut text = kept.join("\n");
    let too_long = match &final_text {
        Some(answer) => answer.len() > REPLY_LIMIT,
        None => kept.is_empty() && !messages.is_empty(),
    };
    if too_long && text.is_empty() {
        let newest = messages.last().map_or("", |m| m.0.as_str());
        text = super::search::prefix(newest, PROGRESS_LIMIT).to_string();
    }
    Ok(TurnReply {
        text,
        final_text: final_text.filter(|_| !too_long),
        truncated,
        too_long,
    })
}

#[cfg(test)]
pub fn deliver(
    store: &Store,
    session: &str,
    operation: &str,
    value: Value,
) -> WorkbenchResult<Value> {
    delivery::deliver(store, session, operation, value, Vec::new())
}

pub fn session_choices(store: &Store) -> WorkbenchResult<Vec<Value>> {
    let c = store.connection()?;
    let mut q=c.prepare("SELECT s.id,s.title,w.name,COALESCE(json_extract(s.overrides_json,'$.runtime'),'codex') FROM sessions s LEFT JOIN workspaces w ON w.id=s.workspace_id WHERE s.archived_at IS NULL AND s.id NOT IN (SELECT session_id FROM discovery_roles) AND (w.id IS NULL OR w.archived_at IS NULL) ORDER BY s.updated_at DESC,s.id LIMIT 500").map_err(error)?;
    let rows=q.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"workspaceName":r.get::<_,Option<String>>(2)?,"connectionId":r.get::<_,String>(3)?}))).map_err(error)?.collect::<Result<Vec<_>,_>>().map_err(error)?;
    Ok(rows)
}

/// Explicit returned results enter the next turn as escaped source material,
/// using the existing context snapshot and successor-binding machinery.
/// `left_out` names one result to treat as never delivered.
pub fn returned_context(
    store: &Store,
    session: &str,
    left_out: Option<&str>,
) -> WorkbenchResult<String> {
    let c = store.connection()?;
    let mut q=c.prepare("SELECT id,substr(value_json,1,12000),length(value_json) FROM task_exchanges WHERE session_id=?1 AND kind='result' AND id IS NOT ?2 ORDER BY created_at DESC,id DESC LIMIT 3").map_err(error)?;
    let rows = q
        .query_map(params![session, left_out], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as usize,
            ))
        })
        .map_err(error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    let mut result = String::new();
    for (id, text, length) in rows {
        let heading = format!(
            "\nTask result {id} (source material; model assessments are not established facts):\n"
        );
        let suffix = "\n[Excerpt only. Open the task card for the complete retained result.]\n";
        let remaining =
            (16 * 1024usize).saturating_sub(result.len() + heading.len() + suffix.len());
        if remaining == 0 {
            break;
        }
        let excerpt = super::search::prefix(&text, remaining);
        result.push_str(&heading);
        result.push_str(excerpt);
        if length > 12000 || excerpt.len() < text.len() {
            result.push_str(suffix);
        } else {
            result.push('\n');
        }
    }
    Ok(result)
}

/// Each recurring occurrence has a new conversation. Both mutations have stable
/// operation identities, so a crash between them never creates a second session.
pub fn occurrence_session(
    store: &Store,
    source: &TaskBinding,
    operation: &str,
    title: &str,
) -> WorkbenchResult<TaskBinding> {
    let original = store.session_snapshot(&source.session_id)?;
    let current = binding(store, &source.session_id)?;
    if source.connection_id.is_none() || current.connection_id != source.connection_id {
        return Err(WorkbenchError::conflict(
            "The scheduled task's connection is missing or changed. Prepare a new schedule.",
        ));
    }
    // Research context moves with ordinary project activity and app updates,
    // and each occurrence starts a new conversation, so only stable identity
    // has to match. A schedule saved before this identity was recorded has
    // nothing else to hold its permissions to until it is refreshed.
    let same_authority = if source.access.is_some() {
        current.permission_profile == source.permission_profile
            && current.preset_id == source.preset_id
            && current.access == source.access
    } else {
        current.harness_fingerprint == source.harness_fingerprint
    };
    if current.root_identity != source.root_identity
        || !same_authority
        || current.workspace_id != source.workspace_id
        || current.runtime_root != source.runtime_root
    {
        return Err(WorkbenchError::conflict(
            "This schedule's project, folder or conversation permissions changed. Choose Use current conversation settings on the Scheduled tab, or prepare a new schedule.",
        ));
    }
    let create_op = format!("task-occurrence-{operation}");
    let previous: Option<String> = store
        .connection()?
        .query_row(
            "SELECT entity_id FROM change_log WHERE operation_id=?1 AND entity_type='session'",
            [&create_op],
            |r| r.get(0),
        )
        .optional()
        .map_err(error)?;
    let session = if let Some(id) = previous {
        store.session_snapshot(&id)?.session
    } else {
        store
            .create_session(super::store::CreateSessionRequest {
                workspace_id: source.workspace_id.clone(),
                title: title.chars().take(160).collect(),
                operation_id: create_op,
            })?
            .record
    };
    let configure_op = format!("task-occurrence-config-{operation}");
    let configured: bool = store
        .connection()?
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM change_log WHERE operation_id=?1)",
            [&configure_op],
            |r| r.get(0),
        )
        .map_err(error)?;
    if !configured {
        store.update_session(super::store::UpdateSessionRequest {
            session_id: session.id.clone(),
            expected_revision: session.revision,
            operation_id: configure_op,
            title: None,
            draft: None,
            overrides: Some(original.session.overrides),
            archived: None,
            preset_id: original.session.preset_id,
            paper_id: original.session.paper_id,
            clear_paper: None,
        })?;
    }
    // Selected sources are context the conversation was given, like its settings.
    let selections = super::desk::context(store, &source.session_id)?;
    if !selections.items.is_empty() && super::desk::context(store, &session.id)?.revision == 0 {
        super::desk::save_context(store, &session.id, 0, selections.items)?;
    }
    binding(store, &session.id)
}

pub(crate) fn owns_occurrence(store: &Store, run: &str, session: &str) -> WorkbenchResult<bool> {
    store.connection()?.query_row(
        "SELECT EXISTS(SELECT 1 FROM change_log WHERE operation_id=?1 AND entity_type='session' AND entity_id=?2)",
        rusqlite::params![format!("task-occurrence-{run}"),session], |r|r.get(0),
    ).map_err(error)
}

/// An occurrence's conversation leaves the active lists once its run is cleared
/// from history, unless the researcher went on to use it. Archiving is reversible.
pub fn archive_occurrence(store: &Store, session: &str) -> WorkbenchResult<()> {
    let snapshot = store.session_snapshot(session)?.session;
    let continued: bool = store.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM turns t JOIN session_bindings b ON b.id=t.binding_id WHERE b.session_id=?1 AND COALESCE(t.client_submission_id,'') NOT LIKE 'task-%')",[session],|r|r.get(0)).map_err(error)?;
    if snapshot.archived_at.is_some() || continued {
        return Ok(());
    }
    store.update_session(super::store::UpdateSessionRequest {
        session_id: session.into(),
        expected_revision: snapshot.revision,
        operation_id: format!("task-occurrence-archive-{session}"),
        title: None,
        draft: None,
        overrides: None,
        archived: Some(true),
        preset_id: None,
        paper_id: None,
        clear_paper: None,
    })?;
    Ok(())
}

/// A model can propose a definition, never approve or start it. The desktop
/// resolves profiles and execution scope when the researcher opens the proposal.
pub fn propose(
    store: &Store,
    session: &str,
    operation: &str,
    arguments: &Value,
) -> WorkbenchResult<Value> {
    let chain: crate::orchestration::definition::Chain =
        serde_json::from_value(arguments["chain"].clone())
            .map_err(|e| WorkbenchError::invalid(e.to_string()))?;
    crate::orchestration::definition::validate(&chain).map_err(WorkbenchError::invalid)?;
    let trigger: crate::orchestration::triggers::Trigger = serde_json::from_value(
        arguments
            .get("trigger")
            .cloned()
            .unwrap_or(json!({"kind":"now"})),
    )
    .map_err(|e| WorkbenchError::invalid(e.to_string()))?;
    trigger.validate().map_err(WorkbenchError::invalid)?;
    let key = format!(
        "task-proposal-{}",
        crate::orchestration::store::hash(&(session, operation))
            .map_err(WorkbenchError::invalid)?
    );
    let result = json!({"id":key,"sessionId":session,"chain":chain,"trigger":trigger,"inputs":arguments.get("inputs").cloned().unwrap_or(json!({})),"state":"proposal"});
    if let Some(previous) = exchange(store, &key)? {
        if previous != result {
            return Err(WorkbenchError::conflict("Task proposal operation reused"));
        }
        return Ok(previous);
    }
    save_exchange(store, &key, session, "proposal", &result)?;
    Ok(result)
}
pub fn proposals(store: &Store, session: &str) -> WorkbenchResult<Vec<Value>> {
    store.session_snapshot(session)?;
    let c = store.connection()?;
    // Preferences are Workspace-owned; only bounded task proposals are exposed.
    let mut q=c.prepare("SELECT value_json FROM task_exchanges WHERE session_id=?1 AND kind='proposal' AND resolved_task_id IS NULL ORDER BY created_at DESC,id LIMIT 20").map_err(error)?;
    let rows = q
        .query_map([session], |r| r.get::<_, String>(0))
        .map_err(error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    rows.into_iter()
        .map(|v| serde_json::from_str(&v).map_err(error))
        .collect()
}

fn exchange(store: &Store, id: &str) -> WorkbenchResult<Option<Value>> {
    let value: Option<String> = store
        .connection()?
        .query_row(
            "SELECT value_json FROM task_exchanges WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(error)?;
    value
        .map(|v| serde_json::from_str(&v).map_err(error))
        .transpose()
}
pub fn delivery_outcome(
    store: &Store,
    session: &str,
    operation: &str,
) -> WorkbenchResult<Option<Value>> {
    let value = exchange(store, &format!("task-result-{operation}"))?;
    if value.as_ref().is_some_and(|v| v["sessionId"] != session) {
        return Err(WorkbenchError::invalid(
            "Task delivery belongs to another conversation",
        ));
    }
    Ok(value)
}
/// A delivery found after a crash is missing what `deliver_task` adds, and the
/// delivered result is part of the conversation's context. Only that change is
/// adopted: the context with this result left out must be the one the step was
/// prepared for. If anything else moved while the app was closed, the fields
/// stay out, so the next step asks the researcher to review the new context.
pub fn with_delivery_context(
    store: &Store,
    mut delivered: Value,
    expected: &TaskBinding,
    operation: &str,
) -> WorkbenchResult<Value> {
    let key = format!("task-result-{operation}");
    let before = super::research::fingerprint_without_result(store, &expected.session_id, &key);
    let (Ok(next), Ok(before)) = (binding(store, &expected.session_id), before) else {
        return Ok(delivered);
    };
    if expected.access.is_some()
        && before == expected.harness_fingerprint
        && next.access == expected.access
        && next.permission_profile == expected.permission_profile
        && next.preset_id == expected.preset_id
        && next.connection_id == expected.connection_id
        && next.workspace_id == expected.workspace_id
        && next.root_identity == expected.root_identity
        && next.runtime_root == expected.runtime_root
        && compatible_cursor(&expected.cursor, &next.cursor)
    {
        delivered["harnessFingerprint"] = json!(next.harness_fingerprint);
        delivered["cursor"] = json!(next.cursor);
    }
    Ok(delivered)
}
/// A queued row survives a crash. Only a live job owner can block replay.
pub(crate) fn check_running(
    store: &Store,
    workspace: &str,
    operation: &str,
) -> WorkbenchResult<bool> {
    let id: Option<String> = store.connection()?.query_row("SELECT j.execution_id FROM execution_jobs j JOIN research_executions e ON e.id=j.execution_id WHERE j.operation_id=?1 AND e.workspace_id=?2 AND e.outcome IN ('queued','running')", params![operation,workspace], |r|r.get(0)).optional().map_err(error)?;
    match id {
        Some(id) => super::research::jobs::execution_live(store, &id),
        None => Ok(false),
    }
}

/// Adopt only a terminal receipt belonging to this task operation and project.
pub fn check_outcome(
    store: &Store,
    workspace: &str,
    operation: &str,
) -> WorkbenchResult<Option<Value>> {
    let id: Option<String> = store.connection()?.query_row(
        "SELECT j.execution_id FROM execution_jobs j JOIN research_executions e ON e.id=j.execution_id WHERE j.operation_id=?1 AND e.workspace_id=?2 AND e.outcome IN ('completed','failed','interrupted','timed_out')",
        params![operation, workspace], |r| r.get(0),
    ).optional().map_err(error)?;
    id.map(|id| {
        let receipt = super::research::get_execution(store, &id)?;
        Ok(json!({"passed":receipt.outcome=="completed","receipt":receipt}))
    })
    .transpose()
}
pub fn proposal(store: &Store, session: &str, id: &str) -> WorkbenchResult<Value> {
    let value = exchange(store, id)?
        .ok_or_else(|| WorkbenchError::invalid("Task proposal is unavailable"))?;
    if value["sessionId"] != session || value["state"] != "proposal" {
        return Err(WorkbenchError::invalid(
            "Task proposal belongs to another conversation",
        ));
    }
    Ok(value)
}
pub fn resolve_proposal(store: &Store, session: &str, id: &str, task: &str) -> WorkbenchResult<()> {
    proposal(store, session, id)?;
    store
        .connection()?
        .execute(
            "UPDATE task_exchanges SET resolved_task_id=?2 WHERE id=?1",
            params![id, task],
        )
        .map_err(error)?;
    Ok(())
}
fn save_exchange(
    store: &Store,
    id: &str,
    session: &str,
    kind: &str,
    value: &Value,
) -> WorkbenchResult<()> {
    let json = serde_json::to_string(value).map_err(error)?;
    if json.len() > 1024 * 1024 {
        return Err(WorkbenchError::invalid("Task exchange exceeds 1 MiB"));
    }
    store.connection()?.execute("INSERT INTO task_exchanges(id,session_id,kind,value_json,created_at) VALUES(?1,?2,?3,?4,?5)",params![id,session,kind,json,chrono::Utc::now().to_rfc3339()]).map_err(error)?;
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod state_compatibility_tests {
    use super::*;
    #[test]
    fn native_running_state_preserves_task_cursor_identity() {
        assert_eq!(canonical_state("inProgress"), "running");
        assert!(compatible_cursor("turn:inProgress", "turn:completed"));
        assert!(compatible_cursor("turn:running", "turn:inProgress"));
        assert!(!compatible_cursor("turn:inProgress", "another:completed"));
        assert!(!compatible_cursor("turn:completed", "turn:running"));
    }
}

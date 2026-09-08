use super::*;
use crate::workbench::{acquisition, project, research};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Monitor {
    pub kind: String,
    pub target: String,
    pub interval_seconds: u64,
    pub notify: bool,
    pub network_consent: bool,
}
pub fn save(
    store: &Store,
    ws: &str,
    title: &str,
    m: Monitor,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    if ![
        "accepted_file",
        "dataset",
        "results",
        "execution",
        "metadata_query",
    ]
    .contains(&m.kind.as_str())
        || m.interval_seconds < 60
        || m.interval_seconds > 30 * 86400
    {
        return Err(WorkbenchError::invalid(
            "Choose a supported check and an interval from one minute to 30 days",
        ));
    }
    if m.kind != "results" {
        bounded(&m.target, 1000)?;
    }
    if m.kind == "metadata_query"
        && (!m.network_consent || !acquisition::network_enabled(store, ws)?)
    {
        return Err(WorkbenchError::invalid(
            "Enable project acquisition and explicitly allow this query to be sent to Crossref",
        ));
    }
    if m.kind != "metadata_query" {
        local(store, ws, &m)?;
    }
    let r = desk::insert(
        store,
        ws,
        "monitor",
        title,
        serde_json::to_value(m).map_err(err)?,
        None,
        operation,
    )?;
    store
        .connection()?
        .execute(
            "INSERT OR IGNORE INTO scheduled_checks(id,next_due_at) VALUES(?1,?2)",
            params![r.id, chrono::Utc::now().timestamp()],
        )
        .map_err(err)?;
    Ok(r)
}
pub fn local(store: &Store, ws: &str, m: &Monitor) -> WorkbenchResult<Value> {
    match m.kind.as_str() {
        "accepted_file" => {
            Ok(json!({"path":m.target,"hash":project::program_file_hash(store,ws,&m.target)?}))
        }
        "dataset" => {
            let selected = desk::record(store, ws, &m.target)?;
            if selected.kind != "dataset" {
                return Err(WorkbenchError::invalid("Choose a tracked dataset version"));
            }
            let mut latest = selected;
            for _ in 0..100 {
                let next:Option<String>=store.connection()?.query_row("SELECT id FROM desk_records WHERE workspace_id=?1 AND kind='dataset' AND supersedes=?2 ORDER BY created_at DESC,id DESC LIMIT 1",params![ws,latest.id],|r|r.get(0)).optional().map_err(err)?;
                if let Some(n) = next {
                    latest = desk::record(store, ws, &n)?;
                } else {
                    break;
                }
            }
            Ok(json!({"versionId":latest.id,"hash":latest.content_hash,"title":latest.title}))
        }
        "results" => {
            let c = store.connection()?;
            let mut q=c.prepare("SELECT sr.id,sr.created_at FROM structured_results sr JOIN research_executions e ON e.id=sr.execution_id WHERE e.workspace_id=?1 ORDER BY sr.created_at DESC,sr.id LIMIT 500").map_err(err)?;
            let results = q
                .query_map([ws], |r| {
                    Ok(json!({"id":r.get::<_,String>(0)?,"createdAt":r.get::<_,String>(1)?}))
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            Ok(json!({"results":results,"window":500}))
        }
        "execution" => {
            let e = research::get_execution(store, &m.target)?;
            if e.workspace_id != ws {
                return Err(WorkbenchError::invalid(
                    "Execution belongs to another project",
                ));
            }
            Ok(json!({"executionId":e.id,"outcome":e.outcome,"exitStatus":e.exit_status}))
        }
        _ => Err(WorkbenchError::invalid(
            "Remote checks run outside the database worker",
        )),
    }
}
pub fn due(store: &Store, at: i64) -> WorkbenchResult<Vec<(DeskRecord, i64)>> {
    let c = store.connection()?;
    let mut q=c.prepare("SELECT d.workspace_id,d.id,s.revision FROM scheduled_checks s JOIN desk_records d ON d.id=s.id JOIN workspaces w ON w.id=d.workspace_id WHERE s.enabled=1 AND s.next_due_at<=?1 AND w.archived_at IS NULL ORDER BY s.next_due_at,s.id LIMIT 8").map_err(err)?;
    let ids = q
        .query_map([at], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    ids.into_iter()
        .map(|(ws, id, revision)| Ok((desk::record(store, &ws, &id)?, revision)))
        .collect()
}
/// First success establishes a quiet baseline. Missed intervals coalesce into one check.
pub fn record_outcome(
    store: &Store,
    ws: &str,
    id: &str,
    expected_revision: i64,
    result: WorkbenchResult<Value>,
    at: i64,
) -> WorkbenchResult<bool> {
    let (record, m): (_, Monitor) = load(store, ws, id, "monitor")?;
    let mut c = store.connection()?;
    let tx = c.transaction().map_err(err)?;
    let (enabled,old,failures,revision):(bool,Option<String>,u32,i64)=tx.query_row("SELECT enabled,last_fingerprint,consecutive_failures,revision FROM scheduled_checks WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(err)?;
    // The revision identifies the captured occurrence and also retires results
    // from checks that were paused/resumed while their work was in flight.
    if !enabled || revision != expected_revision {
        return Ok(false);
    }
    let (body, fingerprint, next_failures) = match result {
        Ok(value) => {
            let hash = desk::hash(value.to_string().as_bytes());
            (json!({"status":"checked","value":value}), Some(hash), 0)
        }
        Err(error) => (
            json!({"status":"failed","reason":error.message}),
            None,
            failures.saturating_add(1),
        ),
    };
    let changed = fingerprint
        .as_ref()
        .is_some_and(|f| old.as_ref().is_some_and(|o| o != f));
    let failure_notice = next_failures == 3;
    let mut notify = false;
    if changed || failure_notice {
        let dedup = desk::hash(format!("{id}:{revision}").as_bytes());
        let aid = format!(
            "attention_{}",
            desk::hash(format!("{id}:{dedup}").as_bytes())
        );
        let n=tx.execute("INSERT OR IGNORE INTO research_attention(id,workspace_id,check_id,fingerprint,body_json,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![aid,ws,id,dedup,json!({"title":record.title,"checkKind":m.kind,"outcome":body,"interpretation":"A tracked state changed. Inspect the evidence before drawing a research conclusion."}).to_string(),desk::now()]).map_err(err)?;
        notify = n == 1 && m.notify;
    }
    let wait = m
        .interval_seconds
        .saturating_mul(2u64.pow(next_failures.min(6)))
        .min(30 * 86400);
    tx.execute("UPDATE scheduled_checks SET last_fingerprint=COALESCE(?2,last_fingerprint),last_outcome_json=?3,last_checked_at=?4,next_due_at=?5,consecutive_failures=?6,revision=revision+1 WHERE id=?1",params![id,fingerprint,body.to_string(),at,at+wait as i64,next_failures]).map_err(err)?;
    tx.commit().map_err(err)?;
    Ok(notify)
}
pub fn list(store: &Store, ws: &str) -> WorkbenchResult<Value> {
    store.workspace(ws)?;
    let c = store.connection()?;
    let mut q=c.prepare("SELECT d.id,d.title,d.body_json,s.enabled,s.revision,s.next_due_at,s.last_checked_at,s.last_outcome_json FROM scheduled_checks s JOIN desk_records d ON d.id=s.id WHERE d.workspace_id=?1 ORDER BY d.created_at DESC LIMIT 100").map_err(err)?;
    let checks=q.query_map([ws],|r|Ok(json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"spec":serde_json::from_str::<Value>(&r.get::<_,String>(2)?).unwrap_or(Value::Null),"enabled":r.get::<_,bool>(3)?,"revision":r.get::<_,i64>(4)?,"nextDueAt":r.get::<_,i64>(5)?,"lastCheckedAt":r.get::<_,Option<i64>>(6)?,"lastOutcome":r.get::<_,Option<String>>(7)?.and_then(|s|serde_json::from_str::<Value>(&s).ok())}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    let mut q=c.prepare("SELECT id,body_json,created_at,acknowledged_at FROM research_attention WHERE workspace_id=?1 ORDER BY created_at DESC LIMIT 200").map_err(err)?;
    let attention=q.query_map([ws],|r|Ok(json!({"id":r.get::<_,String>(0)?,"body":serde_json::from_str::<Value>(&r.get::<_,String>(1)?).unwrap_or(Value::Null),"createdAt":r.get::<_,String>(2)?,"acknowledgedAt":r.get::<_,Option<String>>(3)?}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    Ok(
        json!({"checks":checks,"attention":attention,"lifecycle":"Checks run while the desktop process is alive. Close-to-tray is optional in Tasks. Quit stops checks; missed checks coalesce on next launch."}),
    )
}
pub fn control(
    store: &Store,
    ws: &str,
    id: &str,
    revision: i64,
    action: &str,
) -> WorkbenchResult<Value> {
    if action == "acknowledge" {
        let n = store
            .connection()?
            .execute(
                "UPDATE research_attention SET acknowledged_at=?3 WHERE workspace_id=?1 AND id=?2",
                params![ws, id, desk::now()],
            )
            .map_err(err)?;
        if n == 0 {
            return Err(WorkbenchError::invalid(
                "Attention item belongs to another project",
            ));
        }
    } else {
        let _ = load::<Monitor>(store, ws, id, "monitor")?;
        let enabled = match action {
            "resume" => true,
            "pause" => false,
            _ => return Err(WorkbenchError::invalid("Unknown check action")),
        };
        let n=store.connection()?.execute("UPDATE scheduled_checks SET enabled=?3,revision=revision+1,next_due_at=?4 WHERE id=?1 AND revision=?2",params![id,revision,enabled,chrono::Utc::now().timestamp()]).map_err(err)?;
        if n != 1 {
            return Err(WorkbenchError::conflict(
                "Check changed; refresh before editing",
            ));
        }
    }
    list(store, ws)
}

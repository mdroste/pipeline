use super::super::{definition::Chain, state::Progress, store::*};
use super::model::Summary;
use super::{brief, model::*};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::Value;

pub fn get(s: &Store, id: &str) -> Result<Mission> {
    let body: String = s
        .connection()?
        .query_row("SELECT body FROM missions WHERE id=?1", [id], |r| r.get(0))
        .map_err(err)?;
    serde_json::from_str(&body).map_err(err)
}
pub fn previous(s: &Store, operation: &str, fingerprint: &str) -> Result<Option<Mission>> {
    let row: Option<(String, String)> = s
        .connection()?
        .query_row(
            "SELECT fingerprint,body FROM missions WHERE operation=?1",
            [operation],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(err)?;
    row.map(|(old, body)| {
        if old != fingerprint {
            return Err("Mission operation was reused with a different request".into());
        }
        serde_json::from_str(&body).map_err(err)
    })
    .transpose()
}
pub fn create(s: &Store, m: Mission, operation: &str, fingerprint: &str) -> Result<Mission> {
    if let Some(m) = previous(s, operation, fingerprint)? {
        return Ok(m);
    }
    let mut c = s.connection()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    tx.execute("INSERT INTO missions(id,operation,fingerprint,revision,state,workspace_id,due_at,updated_at,body) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![m.id,operation,fingerprint,m.revision,m.state,m.workspace_id,m.due_at,m.updated_at,serde_json::to_string(&m).map_err(err)?]).map_err(err)?;
    event(&tx, &m.id, "prepared", "Mission scope and limits prepared")?;
    tx.commit().map_err(err)?;
    Ok(m)
}
fn event(tx: &Transaction<'_>, id: &str, kind: &str, detail: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO mission_events(mission_id,at,kind,detail) VALUES(?1,?2,?3,?4)",
        params![id, now(), kind, detail],
    )
    .map_err(err)?;
    Ok(())
}
fn write(tx: &Transaction<'_>, m: &mut Mission, kind: &str, detail: &str) -> Result<()> {
    let expected = m.revision;
    m.revision += 1;
    m.updated_at = now();
    m.brief = brief::render(m);
    let body = serde_json::to_string(&m).map_err(err)?;
    if body.len() > 4 * 1024 * 1024 {
        return Err(
            "Mission retained state exceeds 4 MiB; export the work before starting another mission"
                .into(),
        );
    }
    if tx.execute("UPDATE missions SET revision=?2,state=?3,due_at=?4,updated_at=?5,body=?6 WHERE id=?1 AND revision=?7",params![m.id,m.revision,m.state,m.due_at,m.updated_at,body,expected]).map_err(err)?!=1 {
        m.revision=expected;return Err("Mission changed; reload before continuing".into());
    }
    event(tx, &m.id, kind, detail)
}
pub fn save(s: &Store, m: &mut Mission, kind: &str, detail: &str) -> Result<()> {
    let mut c = s.connection()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    write(&tx, m, kind, detail)?;
    tx.commit().map_err(err)
}
pub fn list(s: &Store, workspace: Option<&str>, offset: u32) -> Result<Vec<Summary>> {
    let c = s.connection()?;
    let mut q=c.prepare("SELECT id,json_extract(body,'$.definition.name'),state,json_extract(body,'$.reason'),workspace_id,updated_at,json_array_length(body,'$.rounds'),(SELECT COUNT(*) FROM json_each(body,'$.questions') WHERE json_extract(value,'$.answer') IS NULL) FROM missions WHERE (?1 IS NULL OR workspace_id=?1) ORDER BY updated_at DESC,id LIMIT 50 OFFSET ?2").map_err(err)?;
    let rows = q
        .query_map(params![workspace, offset], |r| {
            Ok(Summary {
                id: r.get(0)?,
                name: r.get(1)?,
                state: r.get(2)?,
                reason: r.get(3)?,
                workspace_id: r.get(4)?,
                updated_at: r.get(5)?,
                rounds: r.get::<_, u32>(6)? as usize,
                open_questions: r.get::<_, u32>(7)? as usize,
            })
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(rows)
}
pub fn due(s: &Store, at: i64) -> Result<Vec<String>> {
    let c = s.connection()?;
    let mut q=c.prepare("SELECT id FROM missions WHERE state IN ('queued','running','waiting','paused','stopping') AND (due_at<=?1 OR (state IN ('queued','running','waiting') AND json_extract(body,'$.deadlineAt')<=?1)) ORDER BY due_at,id LIMIT 16").map_err(err)?;
    let rows = q
        .query_map([at], |r| r.get(0))
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(rows)
}
pub fn owner(s: &Store, task: &str) -> Result<Option<String>> {
    s.connection()?
        .query_row(
            "SELECT mission_id FROM mission_children WHERE task_id=?1",
            [task],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)
}
pub fn wake_owner(s: &Store, task: &str) -> Result<bool> {
    let Some(id) = owner(s, task)? else {
        return Ok(false);
    };
    let mut m = get(s, &id)?;
    if m.active_child.as_deref() == Some(task) && !m.terminal() {
        m.due_at = Some(now());
        save(
            s,
            &mut m,
            "childStateChanged",
            "A managed child action changed state",
        )?;
    }
    Ok(true)
}

/// A child and its budget reservation enter the same transaction. No orphan
/// approved task can be launched between preparation and mission association.
pub fn admit(s: &Store, m: &mut Mission, chain: Chain, scope: Scope) -> Result<TaskRun> {
    if m.state != "queued" || m.active_child.is_some() {
        return Err("Mission is not ready to dispatch".into());
    }
    let remaining = m.definition.budget.active_seconds as u64
        - m.active_seconds
            .min(m.definition.budget.active_seconds as u64);
    if m.actions_reserved >= m.definition.budget.max_actions
        || remaining < 30
        || m.deadline_at.is_none_or(|d| d <= now())
    {
        return Err("Mission action, active-time or elapsed budget reached".into());
    }
    if chain.steps.len() != 1
        || chain.limits.max_actions != 1
        || chain.limits.action_timeout_secs as u64 > remaining
    {
        return Err("Mission child must be one bounded action within its remaining time".into());
    }
    super::super::definition::validate(&chain)?;
    let time = now();
    let run = TaskRun {
        id: id(),
        mission_id: Some(m.id.clone()),
        revision: 0,
        name: chain.name.clone(),
        state: "queued".into(),
        reason: None,
        chain,
        scope,
        inputs: serde_json::json!({}),
        progress: Progress::default(),
        created_at: time,
        updated_at: time,
        due_at: Some(time),
        deadline_at: m.deadline_at.unwrap(),
        schedule_id: None,
        occurrence: None,
    };
    let mut c = s.connection()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    let op = format!("mission-{}-{}", m.id, m.actions_reserved);
    super::super::store::insert_run(&tx, &run, &op, &hash(&(&run.chain, &run.scope))?)?;
    tx.execute(
        "INSERT INTO mission_children(task_id,mission_id,phase,round) VALUES(?1,?2,?3,?4)",
        params![
            run.id,
            m.id,
            serde_json::to_string(&m.phase).map_err(err)?,
            m.rounds.len() as i64
        ],
    )
    .map_err(err)?;
    m.actions_reserved += 1;
    m.active_child = Some(run.id.clone());
    m.child_ids.push(run.id.clone());
    m.state = "running".into();
    m.due_at = Some(time + 60);
    m.reason = run.name.clone();
    write(&tx, m, "dispatched", &run.name)?;
    tx.commit().map_err(err)?;
    Ok(run)
}
pub fn adopted(s: &Store, m: &mut Mission, task: &str) -> Result<()> {
    let mut c = s.connection()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    if tx.execute("UPDATE mission_children SET adopted=1 WHERE task_id=?1 AND mission_id=?2 AND adopted=0",params![task,m.id]).map_err(err)?!=1 {
        return Err("This child result has already been adopted".into());
    }
    write(&tx, m, "adopted", "Recorded the completed child result")?;
    tx.commit().map_err(err)
}
pub fn answer(
    s: &Store,
    m: &mut Mission,
    question: &str,
    operation: &str,
    answer: &str,
) -> Result<()> {
    super::validate::text(answer, 8000)?;
    super::validate::text(operation, 128)?;
    let mut c = s.connection()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    let old: Option<(String, String, String)> = tx
        .query_row(
            "SELECT mission_id,question_id,answer FROM mission_answers WHERE operation=?1",
            [operation],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(err)?;
    if let Some(old) = old {
        return if old == (m.id.clone(), question.into(), answer.into()) {
            Ok(())
        } else {
            Err("Answer operation was reused".into())
        };
    }
    if m.terminal() || m.state == "stopping" {
        return Err("This mission no longer accepts research input".into());
    }
    let q = m
        .questions
        .iter_mut()
        .find(|q| q.spec.id == question)
        .ok_or("Question is unavailable")?;
    if q.answer.is_some() {
        return Err("This question already has a recorded answer".into());
    }
    q.answer = Some(answer.into());
    tx.execute(
        "INSERT INTO mission_answers(operation,mission_id,question_id,answer) VALUES(?1,?2,?3,?4)",
        params![operation, m.id, question, answer],
    )
    .map_err(err)?;
    if m.state == "waiting" {
        m.state = "queued".into();
        m.phase = Phase::Plan;
        m.reason = "Research input received".into();
        m.due_at = Some(now());
    }
    write(&tx, m, "answer", "Researcher supplied an answer")?;
    tx.commit().map_err(err)
}
pub fn events(s: &Store, id: &str, after: i64) -> Result<Vec<Event>> {
    let c = s.connection()?;
    let mut q=c.prepare("SELECT sequence,at,kind,detail FROM mission_events WHERE mission_id=?1 AND sequence>?2 ORDER BY sequence LIMIT 100").map_err(err)?;
    let rows = q
        .query_map(params![id, after], |r| {
            Ok(Event {
                sequence: r.get(0)?,
                at: r.get(1)?,
                kind: r.get(2)?,
                detail: r.get(3)?,
                attempts: Vec::new(),
            })
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(rows)
}
pub fn methods(s: &Store, ws: &str) -> Result<Vec<Method>> {
    let c = s.connection()?;
    let mut q=c.prepare("SELECT value FROM missions,json_each(missions.body,'$.methods') WHERE workspace_id=?1 AND json_extract(value,'$.retained')=1 ORDER BY updated_at DESC LIMIT 100").map_err(err)?;
    let rows = q
        .query_map([ws], |r| r.get::<_, String>(0))
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    rows.iter()
        .map(|s| serde_json::from_str(s).map_err(err))
        .collect()
}
pub fn child_output(run: &TaskRun) -> Result<Value> {
    run.progress
        .outputs
        .get("action")
        .cloned()
        .ok_or_else(|| "Completed child omitted its result".into())
}

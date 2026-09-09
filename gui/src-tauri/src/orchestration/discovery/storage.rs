use super::model::Summary;
use super::{
    super::{definition::Chain, state::Progress, store::*},
    model::*,
};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::de::DeserializeOwned;
use serde_json::json;

pub fn get(s: &Store, id: &str) -> Result<Run> {
    let v: String = s
        .connection()?
        .query_row("SELECT body FROM discovery_runs WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .map_err(err)?;
    serde_json::from_str(&v).map_err(err)
}
fn records<T: DeserializeOwned>(s: &Store, id: &str, kind: &str) -> Result<Vec<T>> {
    let c = s.connection()?;
    let mut q = c.prepare("SELECT body FROM discovery_records WHERE run_id=?1 AND kind=?2 ORDER BY ordinal LIMIT 101").map_err(err)?;
    let rows = q
        .query_map(params![id, kind], |r| r.get::<_, String>(0))
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    rows.iter()
        .map(|r| serde_json::from_str(r).map_err(err))
        .collect()
}
pub fn load(s: &Store, id: &str) -> Result<Portfolio> {
    Ok(Portfolio {
        run: get(s, id)?,
        candidates: records(s, id, "candidate")?,
        papers: records(s, id, "paper")?,
    })
}
pub fn previous(s: &Store, op: &str, fp: &str) -> Result<Option<Run>> {
    let row: Option<(String, String)> = s
        .connection()?
        .query_row(
            "SELECT fingerprint,body FROM discovery_runs WHERE operation=?1",
            [op],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(err)?;
    row.map(|(old, body)| {
        if old != fp {
            return Err("Start operation was reused with different settings".into());
        }
        serde_json::from_str(&body).map_err(err)
    })
    .transpose()
}
pub fn create(s: &Store, r: &Run, op: &str, fp: &str) -> Result<()> {
    let mut c = s.connection()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    tx.execute(
        "INSERT INTO discovery_runs VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            r.id,
            op,
            fp,
            r.revision,
            r.state,
            r.workspace_id,
            r.due_at,
            r.deadline_at,
            r.updated_at,
            serde_json::to_string(r).map_err(err)?
        ],
    )
    .map_err(err)?;
    event(
        &tx,
        &r.id,
        "started",
        "Research scope and configuration saved",
    )?;
    tx.commit().map_err(err)
}
fn event(tx: &Transaction<'_>, id: &str, kind: &str, detail: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO discovery_events(run_id,at,kind,detail) VALUES(?1,?2,?3,?4)",
        params![id, now(), kind, detail],
    )
    .map_err(err)?;
    Ok(())
}
fn write(tx: &Transaction<'_>, r: &mut Run, kind: &str) -> Result<()> {
    let old = r.revision;
    r.revision += 1;
    r.updated_at = now();
    let body = serde_json::to_string(r).map_err(err)?;
    if body.len() > 2 * 1024 * 1024 {
        return Err("Portfolio summary exceeds 2 MiB".into());
    }
    if tx.execute("UPDATE discovery_runs SET revision=?2,state=?3,due_at=?4,deadline_at=?5,updated_at=?6,body=?7 WHERE id=?1 AND revision=?8",params![r.id,r.revision,r.state,r.due_at,r.deadline_at,r.updated_at,body,old]).map_err(err)?!=1 { return Err("Self-discovery changed; reload before continuing".into()); }
    event(tx, &r.id, kind, &r.reason)
}
fn write_records(tx: &Transaction<'_>, p: &Portfolio) -> Result<()> {
    for (kind, entries) in [
        (
            "candidate",
            p.candidates
                .iter()
                .map(serde_json::to_value)
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(err)?,
        ),
        (
            "paper",
            p.papers
                .iter()
                .map(serde_json::to_value)
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(err)?,
        ),
    ] {
        for (index, value) in entries.iter().enumerate() {
            let body = value.to_string();
            if body.len() > 4 * 1024 * 1024 {
                return Err("A retained research record exceeds 4 MiB".into());
            }
            tx.execute("INSERT INTO discovery_records VALUES(?1,?2,?3,?4) ON CONFLICT(run_id,kind,ordinal) DO UPDATE SET body=excluded.body",params![p.run.id,kind,index as i64,body]).map_err(err)?;
        }
    }
    Ok(())
}
pub fn save(s: &Store, p: &mut Portfolio, kind: &str, adopted: Option<&str>) -> Result<()> {
    let mut c = s.connection()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    if let Some(child) = adopted {
        if tx.execute("UPDATE discovery_children SET adopted=1 WHERE task_id=?1 AND run_id=?2 AND adopted=0",params![child,p.run.id]).map_err(err)?!=1 { return Err("Child result already adopted".into()); }
    }
    write_records(&tx, p)?;
    write(&tx, &mut p.run, kind)?;
    tx.commit().map_err(err)
}
pub fn admit(s: &Store, p: &mut Portfolio, mut chain: Chain, scope: Scope) -> Result<()> {
    let r = &mut p.run;
    let remaining = r.definition.active_seconds.saturating_sub(r.active_seconds);
    if r.state != "running"
        || r.active_child.is_some()
        || r.deadline_at <= now()
        || r.actions_reserved >= r.definition.max_actions
        || remaining < 30
        || chain.steps.len() != 1
        || chain.limits.max_actions != 1
        || u64::from(chain.limits.action_timeout_secs) > remaining
    {
        return Err("Portfolio is not ready or its budget is exhausted".into());
    }
    let mut inputs = json!({});
    if let super::super::definition::Action::Workspace { prompt, .. } = &mut chain.steps[0].action {
        inputs["discoveryPrompt"] = json!(std::mem::replace(
            prompt,
            "{{input:discoveryPrompt}}".into()
        ));
    }
    super::super::definition::validate(&chain)?;
    let time = now();
    let child = TaskRun {
        id: id(),
        mission_id: None,
        revision: 0,
        name: chain.name.clone(),
        state: "queued".into(),
        reason: None,
        chain,
        scope: scope.clone(),
        inputs,
        progress: Progress::default(),
        created_at: time,
        updated_at: time,
        due_at: Some(time),
        deadline_at: r.deadline_at,
        schedule_id: None,
        occurrence: None,
    };
    let mut c = s.connection()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    insert_run(
        &tx,
        &child,
        &format!("discovery-{}-{}", r.id, r.actions_reserved),
        &hash(&(&child.chain, &child.scope, &child.inputs))?,
    )?;
    tx.execute(
        "INSERT INTO discovery_children(task_id,run_id,phase) VALUES(?1,?2,?3)",
        params![
            child.id,
            r.id,
            serde_json::to_string(&r.phase).map_err(err)?
        ],
    )
    .map_err(err)?;
    r.actions_reserved += 1;
    r.active_child = Some(child.id);
    r.active_scope = Some(scope);
    r.due_at = Some(time + 60);
    r.reason = child.name;
    write(&tx, r, "dispatched")?;
    write_records(&tx, p)?;
    tx.commit().map_err(err)
}
pub fn owner(s: &Store, child: &str) -> Result<Option<String>> {
    s.connection()?
        .query_row(
            "SELECT run_id FROM discovery_children WHERE task_id=?1",
            [child],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)
}
pub fn wake_owner(s: &Store, child: &str) -> Result<bool> {
    let Some(id) = owner(s, child)? else {
        return Ok(false);
    };
    let mut r = get(s, &id)?;
    if r.active_child.as_deref() == Some(child) && !r.terminal() {
        let mut c = s.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        r.due_at = Some(now());
        write(&tx, &mut r, "childChanged")?;
        tx.commit().map_err(err)?;
    }
    Ok(true)
}
pub fn due(s: &Store) -> Result<Vec<String>> {
    let c = s.connection()?;
    let mut q=c.prepare("SELECT id FROM discovery_runs WHERE state IN ('running','awaitingSelection','paused','stopping') AND (due_at<=?1 OR deadline_at<=?1) ORDER BY COALESCE(due_at,deadline_at),id LIMIT 8").map_err(err)?;
    let rows = q
        .query_map([now()], |r| r.get(0))
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(rows)
}
pub fn list(s: &Store, ws: Option<&str>, offset: u32) -> Result<Vec<Summary>> {
    let c = s.connection()?;
    let mut q=c.prepare("SELECT body FROM discovery_runs WHERE (?1 IS NULL OR workspace_id=?1) ORDER BY updated_at DESC,id LIMIT 25 OFFSET ?2").map_err(err)?;
    let rows = q
        .query_map(params![ws, offset], |r| r.get::<_, String>(0))
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    rows.iter()
        .map(|v| {
            serde_json::from_str::<Run>(v)
                .map(|r| Summary::from(&r))
                .map_err(err)
        })
        .collect()
}
pub fn select(s: &Store, p: &mut Portfolio, expected: &str, ids: Vec<u32>, op: &str) -> Result<()> {
    super::validate::text(op, 128)?;
    let fp = hash(&(&p.run.id, expected, &ids))?;
    let mut c = s.connection()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    let old: Option<String> = tx
        .query_row(
            "SELECT fingerprint FROM discovery_selections WHERE operation=?1",
            [op],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    if let Some(old) = old {
        return if old == fp {
            Ok(())
        } else {
            Err("Selection operation was reused".into())
        };
    }
    let r = &p.run;
    if r.definition.mode != Mode::Supervised
        || r.state != "awaitingSelection"
        || r.deadline_at <= now()
        || r.selection_hash.as_deref() != Some(expected)
        || ids.is_empty()
        || ids.len() > r.definition.paper_count
        || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
        || ids.iter().any(|id| {
            !r.selection
                .as_ref()
                .is_some_and(|s| s.shortlist.contains(id))
        })
    {
        return Err(
            "Choose distinct projects from the current shortlist within the paper limit".into(),
        );
    }
    p.run.selected = ids.clone();
    p.papers = ids.into_iter().map(Paper::new).collect();
    p.run.phase = Phase::Research { paper: 0 };
    p.run.state = "running".into();
    p.run.due_at = Some(now());
    p.run.reason = "Your project selection is saved; research is starting".into();
    tx.execute(
        "INSERT INTO discovery_selections VALUES(?1,?2,?3)",
        params![op, p.run.id, fp],
    )
    .map_err(err)?;
    write_records(&tx, p)?;
    write(&tx, &mut p.run, "selected")?;
    tx.commit().map_err(err)
}
pub fn events(s: &Store, id: &str, after: i64) -> Result<Vec<Event>> {
    let c = s.connection()?;
    let mut q=c.prepare("SELECT sequence,at,kind,detail FROM discovery_events WHERE run_id=?1 AND sequence>?2 ORDER BY sequence LIMIT 100").map_err(err)?;
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

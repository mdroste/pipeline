//! Read-only progress assembled from coordinator milestones and owning-runtime
//! observations. Polling this view never advances a run or changes its revision.
use super::{storage, store, Coordinator};
use crate::workbench::store::WorkbenchResult;
use crate::workbench::{commands::run_store, tasks};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use tasks::activity::{Agent, Entry, Snapshot};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Step {
    pub id: String,
    pub label: String,
    pub state: String,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Activity {
    pub revision: i64,
    pub observed_at: i64,
    pub last_activity_at: Option<i64>,
    pub step: Option<Step>,
    pub entries: Vec<Entry>,
    pub agents: Vec<Agent>,
}

struct Child {
    id: String,
    label: String,
    state: String,
    session: Option<String>,
    operation: Option<String>,
    started: Option<i64>,
    finished: Option<i64>,
    updated: i64,
    team: Option<Snapshot>,
}

fn context(s: &store::Store, id: &str) -> store::Result<(Activity, Vec<Child>)> {
    let run = storage::get(s, id)?;
    let c = s.connection()?;
    let mut q = c.prepare("SELECT sequence,at,substr(detail,1,1000) FROM discovery_events WHERE run_id=?1 ORDER BY sequence DESC LIMIT 100").map_err(store::err)?;
    let entries = q
        .query_map([id], |r| {
            Ok(Entry {
                id: format!("milestone-{}", r.get::<_, i64>(0)?),
                at: r.get(1)?,
                label: r.get(2)?,
                state: "recorded".into(),
            })
        })
        .map_err(store::err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(store::err)?;
    // Read display metadata only. Step outputs can include large manuscripts.
    let mut q = c
        .prepare(
            "SELECT r.id,substr(json_extract(r.body,'$.name'),1,500),r.state,r.session_id,r.updated_at,
        json_extract(r.body,'$.progress.receipts.action.operation'),
        json_extract(r.body,'$.progress.receipts.action.state'),
        json_extract(r.body,'$.progress.receipts.action.startedAt'),
        json_extract(r.body,'$.progress.receipts.action.finishedAt')
        FROM discovery_children d JOIN runs r ON r.id=d.task_id
        WHERE d.run_id=?1 ORDER BY d.rowid DESC LIMIT 8",
        )
        .map_err(store::err)?;
    let rows = q
        .query_map([id], |r| {
            Ok(Child {
                id: r.get(0)?,
                label: r.get(1)?,
                state: r.get::<_, Option<String>>(6)?.unwrap_or(r.get(2)?),
                session: r.get(3)?,
                updated: r.get(4)?,
                operation: r.get(5)?,
                started: r.get(7)?,
                finished: r.get(8)?,
                team: None,
            })
        })
        .map_err(store::err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(store::err)?;
    let mut children = rows;
    for child in &mut children {
        if let Some(operation) = &child.operation {
            child.team = super::adapters::idea_team::read(s, operation)?;
        }
    }
    let step = children
        .iter()
        .find(|child| Some(&child.id) == run.active_child.as_ref())
        .map(|child| Step {
            id: child.id.clone(),
            label: child.label.clone(),
            state: child.state.clone(),
            started_at: child.started,
            finished_at: child.finished,
        });
    Ok((
        Activity {
            revision: run.revision,
            observed_at: store::now(),
            last_activity_at: entries.iter().map(|e| e.at).max(),
            step,
            entries,
            agents: vec![],
        },
        children,
    ))
}

fn merge(activity: &mut Activity, child: &Child, mut snapshot: Snapshot) {
    activity.last_activity_at = activity
        .last_activity_at
        .max(snapshot.last_activity_at)
        .max(Some(child.updated));
    if let Some(at) = child.started {
        activity.entries.push(Entry {
            id: format!("{}-started", child.id),
            at,
            label: child.label.clone(),
            state: "running".into(),
        });
    }
    if let Some(at) = child.finished {
        activity.entries.push(Entry {
            id: format!("{}-finished", child.id),
            at,
            label: child.label.clone(),
            state: child.state.clone(),
        });
    }
    for entry in &mut snapshot.entries {
        entry.id = format!("{}-{}", child.id, entry.id);
        entry.label = format!("{} · {}", child.label, entry.label);
    }
    activity.entries.extend(snapshot.entries);
    if activity
        .step
        .as_ref()
        .is_some_and(|step| step.id == child.id)
    {
        activity.agents.push(Agent {
            id: child.id.clone(),
            label: child.label.clone(),
            state: child.state.clone(),
            updated_at: snapshot.last_activity_at.unwrap_or(child.updated),
        });
        for agent in &mut snapshot.agents {
            if !matches!(child.state.as_str(), "running" | "queued")
                && matches!(agent.state.as_str(), "running" | "queued" | "starting")
            {
                agent.state = if child.finished.is_some() {
                    "interrupted"
                } else {
                    "unknown"
                }
                .into();
            }
        }
        activity.agents.extend(snapshot.agents);
    }
}

pub(crate) async fn read(c: &Coordinator, id: String) -> store::Result<Activity> {
    let (mut activity, mut children) = c.db(move |s| context(&s, &id)).await?;
    let requests = children
        .iter()
        .filter(|child| child.team.is_none())
        .filter_map(|child| {
            Some((
                child.id.clone(),
                child.session.clone()?,
                format!("task-{}", child.operation.as_ref()?),
            ))
        })
        .collect::<Vec<_>>();
    let observations = run_store(move |s| {
        requests
            .into_iter()
            .map(|(id, session, submission)| {
                tasks::activity::read(&s, &session, &submission).map(|snapshot| (id, snapshot))
            })
            .collect::<WorkbenchResult<BTreeMap<_, _>>>()
    })
    .await
    .map_err(|e| e.message)?;
    for child in &mut children {
        let snapshot = child
            .team
            .take()
            .or_else(|| observations.get(&child.id).cloned())
            .unwrap_or_default();
        merge(&mut activity, child, snapshot);
    }
    activity
        .entries
        .sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.id.cmp(&b.id)));
    activity.entries.truncate(100);
    activity.observed_at = store::now();
    Ok(activity)
}

#[tauri::command]
pub async fn discovery_activity(app: tauri::AppHandle, id: String) -> store::Result<Value> {
    let c = super::super::manager(app).await?;
    serde_json::to_value(read(&c, id).await?).map_err(store::err)
}

#[cfg(test)]
mod tests;

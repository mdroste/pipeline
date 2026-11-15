//! An attempt has one owner. A dispatch marker precedes any side effect; its
//! absence proves non-submission only for new receipts. Missing owner evidence
//! after that marker is never permission to replay. Old receipts infer the
//! owner from the immutable step and make no assumptions about submission.
use super::{atomic_json, task_binding, terminal_failure};
use crate::orchestration::{
    definition::Action,
    state::Receipt,
    store::{err, Result, Scope, Store},
};
use crate::workbench::{commands::run_store, tasks};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{io::Read, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Workspace,
    Review,
    Snapshot,
    Check,
    CapturedCheck,
    LiteratureLookup,
    Deliver,
    Control,
}

impl Kind {
    pub fn of(action: &Action) -> Self {
        match action {
            Action::Workspace { .. } => Self::Workspace,
            Action::Review { .. } => Self::Review,
            Action::Snapshot { .. } => Self::Snapshot,
            Action::Check { .. } => Self::Check,
            Action::CapturedCheck { .. } => Self::CapturedCheck,
            Action::LiteratureLookup { .. } => Self::LiteratureLookup,
            Action::Deliver { .. } => Self::Deliver,
            Action::Delay { .. }
            | Action::Input { .. }
            | Action::Until { .. }
            | Action::If { .. }
            | Action::Repeat { .. }
            | Action::While { .. }
            | Action::ForEach { .. }
            | Action::Parallel { .. }
            | Action::Chain { .. } => Self::Control,
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    NotSubmitted,
    Running,
    Unknown,
    Completed(Value),
    Failed(Value),
}
impl Outcome {
    fn terminal(value: Value) -> Self {
        if terminal_failure(&value) {
            Self::Failed(value)
        } else {
            Self::Completed(value)
        }
    }
}
fn read(path: &Path, limit: usize) -> Result<Option<Value>> {
    if !path.try_exists().map_err(err)? {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    crate::safety::open_regular_file(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > limit {
        return Err("Action journal exceeds its size limit".into());
    }
    serde_json::from_slice(&bytes).map(Some).map_err(err)
}
pub fn mark_team(store: &Store, operation: &str) -> Result<()> {
    atomic_json(
        &store
            .root
            .join("actions")
            .join(operation)
            .join("dispatch.json"),
        &json!({"operation":operation,"kind":Kind::Workspace,"owner":"ideaTeam"}),
    )
}
pub fn mark(store: &Store, operation: &str, kind: Kind) -> Result<()> {
    atomic_json(
        &store
            .root
            .join("actions")
            .join(operation)
            .join("dispatch.json"),
        &json!({"operation":operation,"kind":kind}),
    )
}

/// This never submits an action. Owner probes may reconstruct a local journal
/// from an immutable result, but cannot create or restart a child.
pub async fn lookup(
    store: &Store,
    scope: &Scope,
    action: &Action,
    receipt: &Receipt,
) -> Result<Outcome> {
    let kind = Kind::of(action);
    if receipt.action_kind.is_some_and(|k| k != kind)
        || !crate::orchestration::store::is_operation(&receipt.operation)
    {
        return Err("Action receipt does not match its saved step or attempt".into());
    }
    let op = receipt.operation.clone();
    let dir = store.root.join("actions").join(&op);
    if let Some(value) = read(&dir.join("result.json"), 3 * 1024 * 1024)? {
        return Ok(Outcome::terminal(value));
    }
    if receipt.action_kind.is_some() {
        match read(&dir.join("dispatch.json"), 4096)? {
            None => return Ok(Outcome::NotSubmitted),
            Some(marker)
                if kind == Kind::Workspace
                    && marker == json!({"operation":op,"kind":kind,"owner":"ideaTeam"}) =>
            {
                return Ok(read(&dir.join("team-result.json"), 3 * 1024 * 1024)?
                    .map_or(Outcome::Unknown, Outcome::terminal))
            }
            Some(marker) if marker != json!({"operation":op,"kind":kind}) => {
                return Err("Dispatch marker does not match this attempt".into())
            }
            _ => {}
        }
    }
    let scope = scope.clone();
    let value = match action {
        Action::Snapshot { .. } => read(&dir.join("snapshot.json"), 3 * 1024 * 1024)?,
        Action::Workspace { .. } => {
            let Some(session) = scope.session_id else {
                return Ok(Outcome::Unknown);
            };
            let (v, live) = run_store(move |s| {
                let submission = format!("task-{op}");
                Ok((
                    tasks::outcome(&s, &session, &submission)?,
                    tasks::turn_running(&s, &session, &submission)?,
                ))
            })
            .await
            .map_err(|e| e.message)?;
            if v.as_ref()
                .is_some_and(|v| v["state"] != "completed" && !terminal_failure(v))
            {
                return Ok(if live {
                    Outcome::Running
                } else {
                    Outcome::Unknown
                });
            }
            v
        }
        Action::Review { .. } => {
            let local = store.clone();
            return tokio::task::spawn_blocking(move || {
                if let Some(value) = super::recover_review(&local, &op)? {
                    return Ok(Outcome::terminal(value));
                }
                let Some(id) = super::journaled_review(&local, &op)? else {
                    return Ok(Outcome::Unknown);
                };
                use crate::commands::orchestration::{AttemptStatus,attempt_status};
                Ok(match attempt_status(&id,&op)? {
                    AttemptStatus::Live=>Outcome::Running,
                    AttemptStatus::Ended(status)=>Outcome::Failed(json!({"state":"failed","reviewRunId":id,"error":format!("The Review ended with status {status}")})),
                    AttemptStatus::Unknown=>Outcome::Unknown,
                })
            })
            .await
            .map_err(err)?;
        }
        Action::LiteratureLookup { query } => {
            let Some(project) = scope.workspace_id else {
                return Ok(Outcome::Unknown);
            };
            let query = query.clone();
            run_store(move |s| {
                crate::workbench::discovery::lookup_outcome(&s, &project, &op, &query)
            })
            .await
            .map_err(|e| e.message)?
        }
        Action::Deliver { .. } => {
            let Some(session) = scope.session_id.clone() else {
                return Ok(Outcome::Unknown);
            };
            let binding = task_binding(&scope).ok();
            run_store(
                move |s| match (tasks::delivery_outcome(&s, &session, &op)?, binding) {
                    (Some(value), Some(binding)) => {
                        tasks::with_delivery_context(&s, value, &binding, &op).map(Some)
                    }
                    (v, _) => Ok(v),
                },
            )
            .await
            .map_err(|e| e.message)?
        }
        Action::Check { .. } | Action::CapturedCheck { .. } => {
            let Some(project) = scope.workspace_id else {
                return Ok(Outcome::Unknown);
            };
            let (value, running) = run_store(move |s| {
                Ok((
                    tasks::check_outcome(&s, &project, &op)?,
                    tasks::check_running(&s, &project, &op)?,
                ))
            })
            .await
            .map_err(|e| e.message)?;
            if running {
                return Ok(Outcome::Running);
            }
            value
        }
        _ => None,
    };
    Ok(value.map_or(Outcome::Unknown, Outcome::terminal))
}

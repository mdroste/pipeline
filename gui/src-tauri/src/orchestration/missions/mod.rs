//! Adaptive research planning above the deterministic task coordinator.
//!
//! A mission adds goals, planning and challenge rounds; it never adds a second
//! executor. Completion is an assessment against the researcher's criteria, not
//! a verified result. Missions never accept manuscript edits, confirm evidence,
//! or widen execution authority. Each child action is tied to its mission and
//! reserves budget before dispatch, and all descendants share that budget.
//! Captured experiments choose only among prepared variants; changed inputs or
//! grants need attention. Prompt text never authorizes a host command.
mod accounting;
mod assessment;
mod brief;
mod chains;
mod planning;
pub(crate) use accounting::{account, exhausted};
pub(crate) use assessment::apply_challenge;
use assessment::host_finding;
pub(crate) use chains::child_chain;
pub(crate) use planning::apply_plan;
pub mod commands;
pub mod model;
mod prompts;
#[cfg(all(feature = "e2e", debug_assertions))]
pub(crate) mod qualification;
pub(crate) mod storage;
#[cfg(test)]
mod tests;
mod validate;

use super::{
    adapters,
    store::{self, now, Result, TaskRun},
    Coordinator,
};
#[cfg(test)]
use crate::orchestration::definition::{Action, Chain, Limits, Step};
use crate::workbench::{commands::run_store, missions as workspace};
use model::*;
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};
use tauri::Emitter;

pub(crate) fn notify(m: &Coordinator, mission: &Mission, previous: &str) {
    m.notify();
    if let Some(app) = &m.app {
        let _ = app.emit(
            "missions:changed",
            json!({"id":mission.id,"state":mission.state}),
        );
        if mission.state != previous
            && (mission.terminal() || matches!(mission.state.as_str(), "attention" | "waiting"))
        {
            let _=app.emit("missions:notice",json!({"id":mission.id,"name":mission.definition.name,"state":mission.state,"reason":mission.reason}));
            // Notification delivery belongs to the shell's device preferences.
        }
    }
}
async fn save(m: &Coordinator, mission: &mut Mission, kind: &str) -> Result<()> {
    let mut copy = mission.clone();
    let event = kind.to_string();
    *mission = m
        .db(move |s| {
            let detail = copy.reason.clone();
            storage::save(&s, &mut copy, &event, &detail)?;
            Ok(copy)
        })
        .await?;
    Ok(())
}
/// Only the next poll time changed: no revision, event or rewritten brief.
async fn touch(m: &Coordinator, mission: &Mission) -> Result<()> {
    let id = mission.id.clone();
    let due = mission.due_at;
    m.db(move |s| storage::touch_due(&s, &id, due)).await
}
pub(crate) async fn tick(coordinator: &Arc<Coordinator>) -> Result<()> {
    let ids = coordinator.db(|s| storage::due(&s, now())).await?;
    let mut outcome = Ok(());
    for id in ids {
        if let Err(error) = tick_mission(coordinator, &id).await {
            // A mission that can't record even its failure waits on its own.
            let retry = Some(now() + store::RECORD_RETRY_SECS);
            let _ = coordinator
                .db(move |s| storage::touch_due(&s, &id, retry))
                .await;
            outcome = outcome.and(Err(error));
        }
    }
    outcome
}
async fn tick_mission(coordinator: &Arc<Coordinator>, id: &str) -> Result<()> {
    let id = id.to_string();
    let key = id.clone();
    let mut mission = coordinator.db(move |s| storage::get(&s, &key)).await?;
    let previous = mission.state.clone();
    let revision = mission.revision;
    if let Err(reason) = advance(coordinator, &mut mission).await {
        // A rejected save must not become the basis for recovery: state that
        // could not be committed could not be committed here either.
        mission = coordinator.db(move |s| storage::get(&s, &id)).await?;
        mission.state = "attention".into();
        mission.reason = reason;
        mission.due_at = None;
        mission.attention_count += 1;
        save(coordinator, &mut mission, "attention").await?;
    }
    if revision != mission.revision {
        notify(coordinator, &mission, &previous);
    }
    Ok(())
}

async fn advance(coordinator: &Arc<Coordinator>, m: &mut Mission) -> Result<()> {
    if m.terminal() {
        return Ok(());
    }
    if m.state != "paused" && m.state != "stopping" && m.deadline_at.is_some_and(|d| d <= now()) {
        m.reason = "Research automation deadline reached".into();
        if let Some(id) = m.active_child.clone() {
            stop_child(coordinator, &id).await?;
            m.state = "stopping".into();
            m.stop_outcome = Some("exhausted".into());
            m.due_at = Some(now() + 2);
        } else {
            m.state = "exhausted".into();
            m.due_at = None;
        }
        return save(coordinator, m, "deadline").await;
    }
    if let Some(id) = m.active_child.clone() {
        let child = coordinator.db(move |s| s.get(&id)).await?;
        if child.state == "finished" {
            let paused = m.state == "paused";
            let stopping = m.state == "stopping";
            if stopping {
                account(m, &child);
                m.active_child = None;
                m.state = m.stop_outcome.clone().unwrap_or("stopped".into());
                m.due_at = None;
            } else {
                // Work on a copy: a malformed response cannot partially update the agenda.
                let mut next = m.clone();
                consume(coordinator, &mut next, &child).await?;
                account(&mut next, &child);
                next.active_child = None;
                if paused && !next.terminal() {
                    next.state = "paused".into();
                    next.reason = "Paused; the completed work is retained".into();
                    next.due_at = None;
                }
                *m = next;
            }
            let mut copy = m.clone();
            let child_id = child.id.clone();
            *m = coordinator
                .db(move |s| {
                    storage::adopted(&s, &mut copy, &child_id)?;
                    Ok(copy)
                })
                .await?;
        } else if m.state == "stopping"
            && ["cancelled", "attention", "failed"].contains(&child.state.as_str())
        {
            account(m, &child);
            m.active_child = None;
            m.state = m.stop_outcome.clone().unwrap_or("stopped".into());
            m.due_at = None;
            let mut copy = m.clone();
            let child_id = child.id.clone();
            *m = coordinator
                .db(move |s| {
                    storage::adopted(&s, &mut copy, &child_id)?;
                    Ok(copy)
                })
                .await?;
        } else if ["attention", "failed", "cancelled"].contains(&child.state.as_str()) {
            return Err(format!(
                "{}: {}",
                child.name,
                child
                    .reason
                    .as_deref()
                    .unwrap_or("Inspect the child receipt before retrying")
            ));
        } else {
            m.due_at = if m.state == "paused" && child.state == "paused" {
                None
            } else {
                Some(now() + 60)
            };
            return touch(coordinator, m).await;
        }
    }
    if m.terminal() || m.state == "paused" {
        return Ok(());
    }
    if !m.definition.policy.monitor_ids.is_empty() {
        let ws = m.workspace_id.clone();
        let monitors = m.definition.policy.monitor_ids.clone();
        let cursor = m.watch_cursor;
        let (cursor, mut changes, snapshots) = run_store(move |s| {
            let (cursor, changes) = workspace::changes(&s, &ws, &monitors, cursor)?;
            Ok((
                cursor,
                changes,
                workspace::watch_snapshots(&s, &ws, &monitors)?,
            ))
        })
        .await
        .map_err(|e| e.message)?;
        workspace::append_snapshot_changes(&m.watch_snapshots, &snapshots, &mut changes);
        if cursor != m.watch_cursor || snapshots != m.watch_snapshots {
            m.watch_cursor = cursor;
            m.watch_snapshots = snapshots;
            if !changes.is_empty() {
                m.watch_epoch = m.watch_epoch.saturating_add(1);
            }
            if !changes.is_empty() {
                m.changes.extend(changes);
                if m.changes.len() > 32 {
                    m.changes.drain(..m.changes.len() - 32);
                }
                if m.state == "waiting"
                    && m.rounds
                        .last()
                        .is_some_and(|r| r.plan.disposition == Disposition::WaitForChange)
                {
                    m.state = "queued".into();
                    m.phase = Phase::Plan;
                    m.reason = "A tracked project state changed; reassessing the agenda".into();
                }
            }
            save(coordinator, m, "projectChange").await?;
        }
    }
    if m.state == "waiting" {
        m.due_at = if m.definition.policy.monitor_ids.is_empty() {
            None
        } else {
            Some(now() + 60)
        };
        return touch(coordinator, m).await;
    }
    if m.state != "queued" {
        return Ok(());
    }
    if exhausted(m) {
        m.state = "exhausted".into();
        m.reason="Research automation budget reached; retained results and unresolved questions are available in the brief".into();
        m.due_at = None;
        return save(coordinator, m, "budget").await;
    }
    let source = match m.phase {
        Phase::Plan => &m.planner_scope,
        Phase::Challenge => &m.challenger_scope,
        _ => &m.scope,
    };
    let fingerprint = m
        .authority_fingerprints
        .get(source.session_id.as_deref().unwrap_or(""))
        .cloned()
        .ok_or(
            "This automation's permission record is unavailable. Start a new research automation.",
        )?;
    let scope = source.clone();
    let scope = run_store(move |s| workspace::refresh_scope(&s, &scope, &fingerprint))
        .await
        .map_err(|e| e.message)?;
    let chain = child_chain(m)?;
    let mut copy = m.clone();
    *m = coordinator
        .db(move |s| {
            storage::admit(&s, &mut copy, chain, scope)?;
            Ok(copy)
        })
        .await?;
    Ok(())
}

async fn consume(coordinator: &Coordinator, m: &mut Mission, child: &TaskRun) -> Result<()> {
    let output = storage::child_output(child)?;
    match m.phase {
        Phase::Plan => apply_plan(m, validate::response(&output)?, &child.id)?,
        Phase::Investigate => {
            let round = m.rounds.last().ok_or("Investigation round is missing")?;
            let c = round
                .selected
                .as_ref()
                .ok_or("Selected candidate is missing")?;
            let number = round.number;
            let finding = if c.kind == InvestigationKind::Workspace {
                validate::response::<Finding>(&output)?
            } else {
                host_finding(
                    c,
                    output["receipt"]["outcome"].as_str().unwrap_or("unknown"),
                )
            };
            validate::finding(&finding)?;
            let ws = m.workspace_id.clone();
            let refs = finding.sources.clone();
            let retained = if refs.is_empty() {
                Vec::new()
            } else {
                run_store(move |s| workspace::sources(&s, &ws, &refs))
                    .await
                    .map_err(|e| e.message)?
            };
            let mut evidence = BTreeMap::new();
            for (i, source) in retained.into_iter().enumerate() {
                evidence.insert(format!("r{number}-source{}", i + 1), source);
            }
            if c.kind != InvestigationKind::Workspace {
                evidence.insert(
                    format!("r{number}-execution"),
                    json!({"kind":"executionReceipt","result":output}),
                );
            }
            let memo=format!("# Investigation {}\n\n{}\n\nOutcome: {}\n\nMethod: {}\n\nTested domain: {}\n\nLimitations:\n{}",number,finding.summary,finding.outcome,finding.method,finding.tested_domain,finding.limitations.join("\n"));
            let scope = m.scope.clone();
            let paths = finding.files.clone();
            let snapshots = coordinator
                .db(move |s| {
                    let mut snapshots = vec![adapters::snapshot(
                        &s,
                        &scope,
                        json!(memo),
                        "investigation.md",
                    )?];
                    let mut bytes = 0u64;
                    for path in paths {
                        let filename = std::path::Path::new(&path)
                            .file_name()
                            .and_then(|p| p.to_str())
                            .ok_or("Evidence file name is invalid")?;
                        let snapshot =
                            adapters::snapshot(&s, &scope, json!({"path":path}), filename)?;
                        bytes =
                            bytes.saturating_add(snapshot["bytes"].as_u64().unwrap_or(u64::MAX));
                        if bytes > 64 * 1024 * 1024 {
                            return Err("Investigation file evidence exceeds 64 MiB".into());
                        }
                        snapshots.push(snapshot);
                    }
                    Ok(snapshots)
                })
                .await?;
            for (i, snapshot) in snapshots.into_iter().enumerate() {
                evidence.insert(
                    if i == 0 {
                        format!("r{number}-memo")
                    } else {
                        format!("r{number}-file{i}")
                    },
                    snapshot,
                );
            }
            let signature = store::hash(&(
                &finding,
                evidence
                    .values()
                    .filter_map(|v| v.get("hash"))
                    .collect::<Vec<_>>(),
            ))?;
            let round = m.rounds.last_mut().unwrap();
            round.finding = Some(finding);
            round.evidence = evidence;
            round.result_fingerprint = Some(signature);
            round.child_ids.push(child.id.clone());
            m.phase = if m.definition.policy.review_profile_id.is_some() {
                Phase::Review
            } else {
                Phase::Challenge
            };
            m.state = "queued".into();
            m.due_at = Some(now());
            m.reason = "Investigation retained; independent assessment is next".into();
        }
        Phase::Review => {
            let r = m.rounds.last_mut().ok_or("Review round is missing")?;
            r.review = Some(prompts::retained_review(&output, child));
            r.child_ids.push(child.id.clone());
            m.phase = Phase::Challenge;
            m.state = "queued".into();
            m.due_at = Some(now());
            m.reason = "Review retained; checking the evidence and completion criteria".into();
        }
        Phase::Challenge => apply_challenge(m, validate::response(&output)?, &child.id)?,
    }
    Ok(())
}

pub(crate) async fn stop_child(coordinator: &Coordinator, id: &str) -> Result<()> {
    coordinator.stop_children(id);
    let id = id.to_string();
    coordinator
        .db(move |s| {
            let mut run = s.get(&id)?;
            if !["finished", "cancelled", "failed"].contains(&run.state.as_str()) {
                run.state = if run.progress.receipts.values().any(|r| r.state == "running") {
                    "cancelling"
                } else {
                    "cancelled"
                }
                .into();
                run.due_at = None;
                run.reason = Some("Stopped by the owning automation".into());
                s.write(
                    &mut run,
                    "missionStop",
                    "Owning automation stopped this action",
                )?;
            }
            Ok(())
        })
        .await
}

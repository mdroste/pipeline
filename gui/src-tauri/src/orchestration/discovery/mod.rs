//! Durable portfolio research, dispatched exclusively through the existing task adapters.
mod artifacts;
use artifacts::{check_artifacts, consume, materialize};
pub mod commands;
mod engine;
mod export;
pub mod model;
mod prompts;
#[cfg(any(test, all(feature = "e2e", debug_assertions)))]
pub(crate) mod qualification;
pub(crate) mod storage;
#[cfg(test)]
mod tests;
mod validate;

use super::{
    adapters,
    definition::{Action, Binding, Chain, Limits, Step},
    store::{self, now, Result, TaskRun},
    Coordinator,
};
use crate::workbench::{commands::run_store, discovery as workspace, missions as research};
use model::*;
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};
use tauri::Emitter;

fn notice_transition(previous: &str, state: &str) -> bool {
    previous != state
        && matches!(
            state,
            "awaitingSelection"
                | "waiting"
                | "blocked"
                | "exhausted"
                | "failed"
                | "completed"
                | "partial"
        )
}
fn notify(c: &Coordinator, r: &Run, previous: &str) {
    c.notify();
    if let Some(app) = &c.app {
        let _ = app.emit("discovery:changed", json!({"id":r.id,"state":r.state}));
        if notice_transition(previous, &r.state) {
            let _ = app.emit(
                "discovery:notice",
                json!({"id":r.id,"state":r.state,"revision":r.revision}),
            );
        }
    }
}
async fn save(
    c: &Coordinator,
    p: &mut Portfolio,
    kind: &str,
    adopted: Option<String>,
) -> Result<()> {
    let mut copy = p.clone();
    let kind = kind.to_string();
    let (stored, previous) = c
        .db(move |s| {
            let previous = storage::get(&s, &copy.run.id)?.state;
            storage::save(&s, &mut copy, &kind, adopted.as_deref())?;
            Ok((copy, previous))
        })
        .await?;
    *p = stored;
    notify(c, &p.run, &previous);
    Ok(())
}
pub(crate) async fn tick(c: &Arc<Coordinator>) -> Result<()> {
    let ids = c.db(|s| storage::due(&s)).await?;
    for id in ids {
        let mut p = c.db(move |s| storage::load(&s, &id)).await?;
        if let Err(e) = advance(c, &mut p).await {
            // A rejected capture or database transaction must not become the
            // basis for recovery. Retain the last committed portfolio.
            let id = p.run.id.clone();
            p = c.db(move |s| storage::load(&s, &id)).await?;
            p.run.reason = e;
            if let Some(id) = p.run.active_child.clone() {
                super::missions::stop_child(c, &id).await?;
                p.run.state = "stopping".into();
                p.run.stop_outcome = Some("blocked".into());
                p.run.due_at = Some(now() + 2);
            } else {
                p.run.state = "blocked".into();
                p.run.due_at = None;
            }
            save(c, &mut p, "blocked", None).await?;
        }
        if p.run.terminal() {
            let id = p.run.id.clone();
            run_store(move |s| workspace::revoke(&s, &id))
                .await
                .map_err(|e| e.message)?;
        }
    }
    Ok(())
}
fn account(p: &mut Portfolio, child: &TaskRun) {
    p.run.active_seconds = p.run.active_seconds.saturating_add(
        child
            .progress
            .receipts
            .values()
            .map(|r| {
                r.finished_at
                    .unwrap_or(now())
                    .saturating_sub(r.started_at)
                    .max(0) as u64
            })
            .sum::<u64>(),
    );
    if let Some(index) = p.run.phase.paper() {
        if let Some(paper) = p.papers.get_mut(index) {
            if paper.scope.as_ref().and_then(|s| s.session_id.as_ref())
                == child.scope.session_id.as_ref()
            {
                paper.scope = Some(child.scope.clone());
            }
        }
    }
    p.run.active_child = None;
    p.run.active_scope = None;
}
async fn advance(c: &Arc<Coordinator>, p: &mut Portfolio) -> Result<()> {
    if p.run.terminal() {
        return Ok(());
    }
    if p.run.deadline_at <= now() && p.run.state != "stopping" {
        p.run.reason = "Elapsed research deadline reached; all recorded work is retained".into();
        if let Some(id) = p.run.active_child.clone() {
            super::missions::stop_child(c, &id).await?;
            p.run.state = "stopping".into();
            p.run.stop_outcome = Some("exhausted".into());
            p.run.due_at = Some(now() + 2);
        } else {
            p.run.state = "exhausted".into();
            p.run.due_at = None;
        }
        return save(c, p, "deadline", None).await;
    }
    if let Some(id) = p.run.active_child.clone() {
        let child = c.db(move |s| s.get(&id)).await?;
        if ["finished", "attention", "failed", "cancelled"].contains(&child.state.as_str()) {
            if p.run.state == "stopping" {
                account(p, &child);
                p.run.state = p.run.stop_outcome.clone().unwrap_or("cancelled".into());
                p.run.due_at = None;
                return save(c, p, "stopped", Some(child.id)).await;
            }
            if child.state != "finished" {
                // A lost acknowledgement can contain completed work. Reconcile, never resubmit.
                let mut recovered = child.clone();
                c.recover_results(&mut recovered).await?;
                if recovered
                    .progress
                    .receipts
                    .values()
                    .all(|r| r.state == "completed")
                    && !recovered.progress.receipts.is_empty()
                {
                    recovered.state = "queued".into();
                    recovered.due_at = Some(now());
                    c.save(
                        &mut recovered,
                        "reconciled",
                        "Adopted the recorded result without replaying it",
                    )
                    .await?;
                    return Ok(());
                }
                return Err(format!(
                    "{}; the recorded child was not replayed",
                    child
                        .reason
                        .as_deref()
                        .unwrap_or("A research action did not finish")
                ));
            }
            let paused = p.run.state == "paused";
            let output = child
                .progress
                .outputs
                .get("action")
                .ok_or("Completed research action omitted its output")?
                .clone();
            let mut next = p.clone();
            let result = consume(c, &mut next, &child, &output).await;
            match result {
                Ok(()) => {
                    account(&mut next, &child);
                    *p = next;
                }
                Err(e) => {
                    // Only a completed, journaled model response may receive a bounded repair turn.
                    account(p, &child);
                    if p.run.repairs < 2
                        && !matches!(
                            p.run.phase,
                            Phase::Acquire { .. } | Phase::ExternalReview { .. } | Phase::Deliver
                        )
                    {
                        p.run.repairs += 1;
                        p.run.reason = format!("Repair the response contract: {e}");
                        p.run.due_at = Some(now());
                    } else {
                        p.run.state = "blocked".into();
                        p.run.reason = format!("Research response could not be adopted: {e}");
                        p.run.due_at = None;
                    }
                }
            }
            if paused && !p.run.terminal() {
                p.run.state = "paused".into();
                p.run.due_at = None;
            }
            save(c, p, "adopted", Some(child.id)).await?;
        } else {
            p.run.due_at = Some(now() + 60);
            return save(c, p, "waiting", None).await;
        }
    }
    if p.run.state != "running" {
        return Ok(());
    }
    if p.run.actions_reserved >= p.run.definition.max_actions
        || p.run.active_seconds + 30 > p.run.definition.active_seconds
    {
        p.run.state = "exhausted".into();
        p.run.reason =
            "Research budget reached; recorded papers, reviews and other work remain available"
                .into();
        p.run.due_at = None;
        return save(c, p, "budget", None).await;
    }
    if p.run.catalog_revision != crate::auto_review::catalog_revision() {
        return Err("The research specialist catalog changed; start a new automation using the retained work".into());
    }
    let phase = p.run.phase.clone();
    let writer = matches!(phase, Phase::Research { .. } | Phase::Draft { .. })
        && p.run.definition.allow_computation;
    let persistent = matches!(phase, Phase::Research { .. } | Phase::Draft { .. });
    let source = p.run.source_scope.clone();
    let expected = p.run.source_authority.clone();
    run_store(move |s| research::refresh_scope(&s, &source, &expected))
        .await
        .map_err(|e| e.message)?;
    let scope = if let Some(index) = phase.paper().filter(|_| persistent) {
        if let (Some(scope), Some(fp)) = (&p.papers[index].scope, &p.papers[index].authority) {
            let scope = scope.clone();
            let fp = fp.clone();
            run_store(move |s| research::refresh_scope(&s, &scope, &fp))
                .await
                .map_err(|e| e.message)?
        } else {
            let source = p.run.source_scope.clone();
            let id = p.run.id.clone();
            let deadline = p.run.deadline_at;
            let role = format!("paper-{}", p.papers[index].candidate_id);
            let (scope, fp) =
                run_store(move |s| workspace::role(&s, &source, &id, &role, writer, deadline))
                    .await
                    .map_err(|e| e.message)?;
            materialize(c, &scope, &p.run.input_artifacts).await?;
            p.papers[index]
                .evidence
                .extend(p.run.input_artifacts.clone());
            p.papers[index].scope = Some(scope.clone());
            p.papers[index].authority = Some(fp);
            scope
        }
    } else if matches!(
        phase,
        Phase::Acquire { .. } | Phase::ExternalReview { .. } | Phase::Deliver
    ) {
        p.run.source_scope.clone()
    } else {
        let source = p.run.source_scope.clone();
        let id = p.run.id.clone();
        let deadline = p.run.deadline_at;
        let role = format!("action-{}", p.run.actions_reserved);
        run_store(move |s| workspace::role(&s, &source, &id, &role, false, deadline))
            .await
            .map_err(|e| e.message)?
            .0
    };
    if phase.paper().is_none()
        && !matches!(
            phase,
            Phase::Acquire { .. } | Phase::ExternalReview { .. } | Phase::Deliver
        )
    {
        materialize(c, &scope, &p.run.input_artifacts).await?;
    }
    if phase.reviewer() {
        if let Some(index) = phase.paper() {
            let mut files = p.run.input_artifacts.clone();
            for (id, a) in &p.papers[index].evidence {
                if a["kind"] == "artifact" {
                    files.insert(
                        format!("{id}-{}", a["filename"].as_str().unwrap_or("evidence")),
                        a.clone(),
                    );
                }
            }
            if let Some(v) = p.papers[index].versions.last() {
                files.extend(v.artifacts.clone());
            }
            materialize(c, &scope, &files).await?;
        }
    }
    let action = match phase {
        Phase::Acquire { index } => Action::LiteratureLookup {
            query: p
                .run
                .orientation
                .as_ref()
                .ok_or("Orientation missing")?
                .literature_queries[index]
                .clone(),
        },
        Phase::ExternalReview { paper } => Action::Review {
            profile_id: p
                .run
                .definition
                .review_profile_id
                .clone()
                .ok_or("Review profile missing")?,
            input: Binding::Literal {
                value: p.papers[paper]
                    .versions
                    .last()
                    .and_then(|v| v.artifacts.get("paper.md"))
                    .ok_or("Paper snapshot missing")?
                    .clone(),
            },
            interpretation: "document".into(),
            variables: BTreeMap::new(),
        },
        Phase::Deliver => Action::Deliver {
            input: Binding::Literal {
                value: export::delivery(p),
            },
        },
        _ => Action::Workspace {
            prompt: prompts::prompt(p)?,
            model: if phase.reviewer() {
                p.run.definition.reviewer_model.clone()
            } else {
                p.run.definition.author_model.clone()
            },
            effort: None,
        },
    };
    let timeout = u64::from(p.run.definition.action_timeout_seconds).min(
        p.run
            .definition
            .active_seconds
            .saturating_sub(p.run.active_seconds),
    ) as u32;
    let name = format!("Self-discovery · {}", export::phase_name(&p.run.phase));
    let chain = Chain {
        schema_version: 1,
        name: name.clone(),
        description: CONTRACT.into(),
        steps: vec![Step {
            id: "action".into(),
            label: name,
            action,
        }],
        limits: Limits {
            max_actions: 1,
            deadline_hours: p.run.definition.deadline_hours,
            action_timeout_secs: timeout,
        },
    };
    let mut copy = p.clone();
    *p = c
        .db(move |s| {
            storage::admit(&s, &mut copy, chain, scope)?;
            Ok(copy)
        })
        .await?;
    notify(c, &p.run, &p.run.state);
    Ok(())
}

#[cfg(test)]
#[test]
fn discovery_notices_only_cover_actionable_transitions() {
    for state in [
        "awaitingSelection",
        "waiting",
        "blocked",
        "exhausted",
        "failed",
        "completed",
        "partial",
    ] {
        assert!(notice_transition("running", state));
        assert!(!notice_transition(state, state));
    }
    for state in ["running", "paused", "cancelled", "stopping"] {
        assert!(!notice_transition("running", state));
    }
}

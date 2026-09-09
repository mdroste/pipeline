//! Adaptive research planning above the deterministic task coordinator.
mod brief;
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
    definition::{Action, Binding, Chain, Limits, Step},
    store::{self, now, Result, TaskRun},
    Coordinator,
};
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
pub(crate) async fn tick(coordinator: &Arc<Coordinator>) -> Result<()> {
    let ids = coordinator.db(|s| storage::due(&s, now())).await?;
    for id in ids {
        let mut mission = coordinator.db(move |s| storage::get(&s, &id)).await?;
        let previous = mission.state.clone();
        let revision = mission.revision;
        if let Err(reason) = advance(coordinator, &mut mission).await {
            mission.state = "attention".into();
            mission.reason = reason;
            mission.due_at = None;
            mission.attention_count += 1;
            save(coordinator, &mut mission, "attention").await?;
        }
        if revision != mission.revision {
            notify(coordinator, &mission, &previous);
        }
    }
    Ok(())
}

async fn advance(coordinator: &Arc<Coordinator>, m: &mut Mission) -> Result<()> {
    if m.terminal() {
        return Ok(());
    }
    if m.state != "paused" && m.state != "stopping" && m.deadline_at.is_some_and(|d| d <= now()) {
        m.reason = "Mission elapsed deadline reached".into();
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
            return save(coordinator, m, "waitingForChild").await;
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
        return save(coordinator, m, "waiting").await;
    }
    if m.state != "queued" {
        return Ok(());
    }
    if exhausted(m) {
        m.state = "exhausted".into();
        m.reason="Mission budget reached; retained results and unresolved questions are available in the brief".into();
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
        .ok_or("Mission authority record is unavailable")?;
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
pub(crate) fn exhausted(m: &Mission) -> bool {
    m.actions_reserved >= m.definition.budget.max_actions
        || m.active_seconds + 30 > m.definition.budget.active_seconds as u64
        || (m.phase == Phase::Plan && m.rounds.len() >= m.definition.budget.max_rounds as usize)
}
pub(crate) fn account(m: &mut Mission, child: &TaskRun) {
    m.active_seconds = m.active_seconds.saturating_add(
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
    let target = if child.scope.session_id == m.planner_scope.session_id {
        &mut m.planner_scope
    } else if child.scope.session_id == m.challenger_scope.session_id {
        &mut m.challenger_scope
    } else {
        &mut m.scope
    };
    *target = child.scope.clone();
}
pub(crate) fn child_chain(m: &Mission) -> Result<Chain> {
    let workspace = |prompt| Action::Workspace {
        prompt,
        model: None,
        effort: None,
    };
    let (label, action) = match m.phase {
        Phase::Plan => (
            "Choose the next investigation".to_string(),
            workspace(prompts::plan(m)?),
        ),
        Phase::Challenge => (
            "Challenge the findings".to_string(),
            workspace(prompts::challenge(m)?),
        ),
        Phase::Review => {
            let r = m.rounds.last().ok_or("Investigation is missing")?;
            let artifact = r
                .evidence
                .get(&format!("r{}-memo", r.number))
                .ok_or("Investigation memo was not captured")?;
            (
                "Review the investigation memo".into(),
                Action::Review {
                    profile_id: m
                        .definition
                        .policy
                        .review_profile_id
                        .clone()
                        .ok_or("Review was not authorized")?,
                    input: Binding::Literal {
                        value: artifact.clone(),
                    },
                    interpretation: "document".into(),
                    variables: BTreeMap::new(),
                },
            )
        }
        Phase::Investigate => {
            let c = m
                .rounds
                .last()
                .and_then(|r| r.selected.as_ref())
                .ok_or("Selected investigation is missing")?;
            let action = match c.kind {
                InvestigationKind::Workspace => workspace(prompts::investigate(m)?),
                InvestigationKind::Check => Action::Check {
                    profile_id: c.capability_id.clone().ok_or("Check identity is missing")?,
                },
                InvestigationKind::Experiment => Action::CapturedCheck {
                    plan_id: c
                        .capability_id
                        .clone()
                        .ok_or("Experiment identity is missing")?,
                },
            };
            (c.question.chars().take(160).collect(), action)
        }
    };
    let remaining = (m.definition.budget.active_seconds as u64)
        .saturating_sub(m.active_seconds)
        .min(m.definition.budget.action_timeout_seconds as u64) as u32;
    Ok(Chain {
        schema_version: 1,
        name: format!(
            "{} · {}",
            m.definition.name.chars().take(60).collect::<String>(),
            label.chars().take(90).collect::<String>()
        ),
        description: "Managed research mission action".into(),
        steps: vec![Step {
            id: "action".into(),
            label,
            action,
        }],
        limits: Limits {
            max_actions: 1,
            deadline_hours: m.definition.budget.deadline_hours,
            action_timeout_secs: remaining,
        },
    })
}
fn merge_questions(m: &mut Mission, questions: Vec<QuestionSpec>) -> Result<()> {
    for spec in questions {
        if let Some(existing) = m.questions.iter().find(|q| q.spec.id == spec.id) {
            if store::hash(&existing.spec)? != store::hash(&spec)? {
                return Err("An existing research question cannot be silently redefined".into());
            }
        } else {
            m.questions.push(Question { spec, answer: None });
        }
    }
    if m.questions.len() > 64 {
        return Err("Mission question limit reached".into());
    }
    Ok(())
}
pub(crate) fn apply_plan(m: &mut Mission, p: Plan, child: &str) -> Result<()> {
    validate::plan(m, &p)?;
    for spec in &p.new_goals {
        m.goals.push(Goal {
            spec: spec.clone(),
            state: "open".into(),
            assessment: String::new(),
            resolved_round: None,
        });
    }
    merge_questions(m, p.questions.clone())?;
    let selected = p
        .candidates
        .iter()
        .find(|c| Some(&c.id) == p.selected_id.as_ref())
        .cloned();
    let disposition = p.disposition;
    m.rounds.push(Round {
        number: m.rounds.len() as u32 + 1,
        change_cursor: m.watch_epoch,
        plan: p,
        selected,
        finding: None,
        evidence: BTreeMap::new(),
        review: None,
        challenge: None,
        child_ids: vec![child.into()],
        result_fingerprint: None,
    });
    m.state = "queued".into();
    m.due_at = Some(now());
    match disposition {
        Disposition::Investigate => {
            m.phase = Phase::Investigate;
            m.reason = "Investigation selected within the mission's scope".into();
        }
        Disposition::AssessCompletion => {
            m.phase = Phase::Challenge;
            m.reason = "Checking the original completion criteria".into();
        }
        Disposition::WaitForInput | Disposition::WaitForChange => {
            m.state = "waiting".into();
            m.phase = Phase::Plan;
            m.reason = if disposition == Disposition::WaitForInput {
                "Waiting for your research input"
            } else {
                "Waiting for a tracked project change"
            }
            .into();
            if disposition == Disposition::WaitForInput
                && !m.questions.iter().any(|q| q.answer.is_none())
            {
                return Err("The proposed input wait has no unanswered question".into());
            }
            m.due_at = if m.definition.policy.monitor_ids.is_empty() {
                None
            } else {
                Some(now() + 60)
            };
        }
    }
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
                Finding{summary:format!("Executed the selected check: {}",c.question),outcome:format!("Execution outcome: {}",output["receipt"]["outcome"].as_str().unwrap_or("unknown")),method:format!("Recorded {} execution",if c.kind==InvestigationKind::Experiment{"captured-plan"}else{"configured host"}),tested_domain:c.instruction.clone(),limitations:vec!["Process completion and scientific support are separate assessments. Dependency coverage is declared-only; host execution is not a conversation sandbox.".into()],sources:Vec::new(),files:Vec::new()}
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
            r.review = Some(output);
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

pub(crate) fn apply_challenge(m: &mut Mission, c: Challenge, child: &str) -> Result<()> {
    validate::challenge(m, &c)?;
    let r = m.rounds.last().ok_or("Challenge round is missing")?;
    if r.challenge.is_some() {
        return Err("This investigation already has a retained challenge".into());
    }
    if c.goal_resolved
        && c.outcome != Outcome::Infeasible
        && r.selected
            .as_ref()
            .is_some_and(|s| s.kind != InvestigationKind::Workspace)
        && r.evidence
            .get(&format!("r{}-execution", r.number))
            .is_some_and(|e| e["result"]["passed"] != true)
    {
        return Err(
            "A failed or unknown computation cannot establish support or a scientific refutation"
                .into(),
        );
    }
    let repeated = r.result_fingerprint.as_ref().is_some_and(|f| {
        m.rounds
            .iter()
            .take(m.rounds.len() - 1)
            .any(|old| old.result_fingerprint.as_ref() == Some(f))
    });
    if c.progress_made && !repeated {
        m.stagnant_rounds = 0;
    } else {
        m.stagnant_rounds += 1;
    }
    if let Some(candidate) = &r.selected {
        if let Some(goal) = m.goals.iter_mut().find(|g| g.spec.id == candidate.goal_id) {
            goal.assessment = c.summary.clone();
            if c.goal_resolved {
                goal.state = match c.outcome {
                    Outcome::Supported => "supported",
                    Outcome::Refuted => "refuted",
                    Outcome::Infeasible => "infeasible",
                    Outcome::Inconclusive => "open",
                }
                .into();
                goal.resolved_round = Some(r.number);
            }
        }
    }
    let number = r.number;
    for (i, spec) in c.methods.iter().enumerate() {
        m.methods.push(Method {
            id: format!("{}-r{number}-m{i}", m.id),
            mission_id: m.id.clone(),
            round: number,
            workspace_id: m.workspace_id.clone(),
            spec: spec.clone(),
            retained: false,
        });
    }
    merge_questions(m, c.questions.clone())?;
    let complete = c.mission_complete;
    let r = m.rounds.last_mut().unwrap();
    r.challenge = Some(c);
    r.child_ids.push(child.into());
    m.phase = Phase::Plan;
    m.state = "queued".into();
    m.due_at = Some(now());
    m.reason = "Assessment retained; selecting the next useful investigation".into();
    if complete {
        m.state = "completed".into();
        m.reason="The challenger assessed every original criterion as met; review the evidence and its limitations".into();
        m.due_at = None;
    } else if m.stagnant_rounds >= m.definition.budget.max_stagnant_rounds {
        m.state = "attention".into();
        m.reason="Repeated investigations are no longer adding evidence. Reconsider the agenda before spending more resources.".into();
        m.due_at = None;
        m.attention_count += 1;
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
                run.reason = Some("Stopped by the owning mission".into());
                s.save(
                    &mut run,
                    "missionStop",
                    "Owning mission stopped this action",
                )?;
            }
            Ok(())
        })
        .await
}

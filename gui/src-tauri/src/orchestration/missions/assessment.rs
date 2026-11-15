//! Host findings and challenge assessment, separate from admission transactions.
use super::planning::merge_questions;
use super::{
    model::{Candidate, Challenge, Finding, InvestigationKind, Method, Mission, Outcome, Phase},
    prompts, validate,
};
use crate::orchestration::store::{now, Result};

/// The finding synthesized for a Check/Experiment execution. Plan candidates
/// allow 12 000-byte instructions while findings cap tested_domain at 4 000,
/// so the instruction is excerpted; the full text stays in the round.
pub(super) fn host_finding(c: &Candidate, outcome: &str) -> Finding {
    Finding {
        execution_ids:vec![],
        summary: format!(
            "Executed the selected check: {}",
            prompts::short(&c.question, 3800)
        ),
        outcome: format!("Execution outcome: {outcome}"),
        method: format!(
            "Recorded {} execution",
            if c.kind == InvestigationKind::Experiment {
                "captured-plan"
            } else {
                "configured host"
            }
        ),
        tested_domain: prompts::short(&c.instruction, 3800),
        limitations: vec!["Process completion and scientific support are separate assessments. Dependency coverage is declared-only; host execution is not a conversation sandbox.".into()],
        sources: Vec::new(),
        files: Vec::new(),
    }
}

pub(crate) fn apply_challenge(m: &mut Mission, c: Challenge, child: &str) -> Result<()> {
    validate::challenge(m, &c)?;
    super::upgrade_round_fingerprints(&mut m.rounds)?;
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
    let evidence = m
        .rounds
        .iter()
        .flat_map(|round| round.evidence.clone())
        .collect();
    let mut objections = c.unresolved.clone();
    if r.review.as_ref().is_some_and(|v| {
        v["complete"] != true
            || v["highPriorityCount"].as_u64().unwrap_or(0) > 0
            || v["unknownPriorityCount"].as_u64().unwrap_or(0) > 0
    }) {
        objections.push("The structured Review is incomplete or retains material findings.".into());
    }
    let no_round_objection = objections.is_empty();
    for a in &c.criteria {
        if a.status != super::model::CriterionStatus::Met {
            objections.push(format!(
                "{}: {}",
                m.definition.criteria[a.criterion], a.explanation
            ));
        }
    }
    let record = crate::orchestration::claims::assess(
        format!("round-{}", r.number),
        ("Independent challenger", child),
        &m.definition.criteria,
        &c.claim_assessments,
        &evidence,
        objections,
        matches!(c.outcome, Outcome::Supported | Outcome::Refuted)
            && c.criteria
                .iter()
                .all(|a| a.status == super::model::CriterionStatus::Met),
    )?;
    if record
        .claims
        .iter()
        .filter_map(|a| a.goal_id.as_ref())
        .any(|id| !m.goals.iter().any(|g| g.spec.id == *id))
    {
        return Err("Claim names an unknown agenda goal".into());
    }
    let selected_supported = r.selected.as_ref().is_some_and(|candidate| {
        let mut matching = record
            .claims
            .iter()
            .filter(|a| a.goal_id.as_ref() == Some(&candidate.goal_id))
            .peekable();
        record.supported
            || (no_round_objection
                && matching.peek().is_some()
                && matching.all(|a| a.meets_requirements))
    });
    let overall_infeasible = r.selected.is_none() && c.outcome == Outcome::Infeasible;
    let infeasible = c.goal_resolved && c.outcome == Outcome::Infeasible;
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
            if c.goal_resolved && (selected_supported || infeasible) {
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
    let requested_complete = c.mission_complete;
    let complete = requested_complete && record.supported;
    let r = m.rounds.last_mut().unwrap();
    r.claim_record = Some(record);
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
    } else if overall_infeasible || (infeasible && m.goals.iter().all(|g| g.state != "open")) {
        m.state = "stopped".into();
        m.reason =
            "Investigation stopped as infeasible; no supported completion is claimed.".into();
        m.due_at = None;
    } else if requested_complete {
        m.state = "attention".into();
        m.reason="The assessment has unresolved evidence requirements. Inspect the claims before continuing.".into();
        m.attention_count += 1;
        m.due_at = None;
    } else if m.stagnant_rounds >= m.definition.budget.max_stagnant_rounds {
        m.state = "attention".into();
        m.reason="Repeated investigations are no longer adding evidence. Reconsider the agenda before spending more resources.".into();
        m.due_at = None;
        m.attention_count += 1;
    }
    Ok(())
}

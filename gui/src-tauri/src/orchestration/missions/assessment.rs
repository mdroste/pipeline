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

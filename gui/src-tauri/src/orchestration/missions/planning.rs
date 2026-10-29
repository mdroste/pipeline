//! Validated plan application and question lineage.
use super::{
    model::{Disposition, Goal, Mission, Phase, Plan, Question, QuestionSpec, Round},
    validate,
};
use crate::orchestration::store::{self, now, Result};
use std::collections::BTreeMap;

pub(super) fn merge_questions(m: &mut Mission, questions: Vec<QuestionSpec>) -> Result<()> {
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

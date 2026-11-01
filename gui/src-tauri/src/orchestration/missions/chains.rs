//! Bounded deterministic child-chain construction.
use super::{
    model::{InvestigationKind, Mission, Phase},
    prompts,
};
use crate::orchestration::{
    definition::{Action, Binding, Chain, Limits, Step},
    store::Result,
};
use crate::workbench::search::prefix;
use std::collections::BTreeMap;

/// A chain's name and step label are each limited to 200 bytes, not characters.
const LABEL_BYTES: usize = 200;
const NAME_PART_BYTES: usize = 98;

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
            (prefix(&c.question, LABEL_BYTES).to_string(), action)
        }
    };
    let remaining = (m.definition.budget.active_seconds as u64)
        .saturating_sub(m.active_seconds)
        .min(m.definition.budget.action_timeout_seconds as u64) as u32;
    Ok(Chain {
        schema_version: 1,
        name: format!(
            "{} · {}",
            prefix(&m.definition.name, NAME_PART_BYTES),
            prefix(&label, NAME_PART_BYTES)
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

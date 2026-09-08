use super::super::store::{hash, Result};
use super::model::*;
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, BTreeSet};

pub fn text(value: &str, limit: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > limit || value.contains('\0') {
        return Err(format!("Required text is empty or exceeds {limit} bytes"));
    }
    Ok(())
}
fn id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(
            "Use a short stable identifier containing letters, numbers, underscores or dashes"
                .into(),
        );
    }
    Ok(())
}
fn strings(values: &[String], max: usize, bytes: usize) -> Result<()> {
    if values.len() > max {
        return Err("Too many entries".into());
    }
    for v in values {
        text(v, bytes)?;
    }
    Ok(())
}
pub fn definition(d: &Definition) -> Result<()> {
    if d.schema_version != 1 {
        return Err("Unsupported mission version".into());
    }
    text(&d.name, 160)?;
    text(&d.objective, 8000)?;
    if d.background.len() > 16000 {
        return Err("Mission background exceeds 16 KiB".into());
    }
    strings(&d.criteria, 12, 2000)?;
    if d.criteria.is_empty() {
        return Err("Specify at least one completion criterion".into());
    }
    let b = &d.budget;
    if !(1..=32).contains(&b.max_rounds)
        || !(3..=256).contains(&b.max_actions)
        || !(60..=604800).contains(&b.active_seconds)
        || !(30..=7200).contains(&b.action_timeout_seconds)
        || b.active_seconds < b.action_timeout_seconds
        || !(1..=720).contains(&b.deadline_hours)
        || !(1..=8).contains(&b.max_stagnant_rounds)
    {
        return Err("Mission limits are outside the supported range".into());
    }
    for list in [
        &d.policy.check_profile_ids,
        &d.policy.experiment_ids,
        &d.policy.monitor_ids,
        &d.method_ids,
    ] {
        strings(list, 32, 200)?;
        if list.iter().collect::<BTreeSet<_>>().len() != list.len() {
            return Err("Duplicate mission selections".into());
        }
    }
    Ok(())
}

/// Only an entire JSON response (optionally one JSON fence) is accepted.
pub fn response<T: DeserializeOwned>(value: &serde_json::Value) -> Result<T> {
    let source = value["finalText"]
        .as_str()
        .or_else(|| value["text"].as_str())
        .ok_or("The research turn has no text response")?
        .trim();
    if source.len() > 128 * 1024 {
        return Err("Research response exceeds 128 KiB".into());
    }
    let body = if let Some(body) = source
        .strip_prefix("```json\n")
        .or_else(|| source.strip_prefix("```\n"))
    {
        body.strip_suffix("```")
            .ok_or("Incomplete research JSON fence")?
            .trim()
    } else {
        source
    };
    serde_json::from_str(body)
        .map_err(|e| format!("Research response did not satisfy its structured contract: {e}"))
}

pub fn plan(m: &Mission, p: &Plan) -> Result<()> {
    text(&p.summary, 8000)?;
    text(&p.selection_reason, 4000)?;
    if p.new_goals.len() > 16 || m.goals.len() + p.new_goals.len() > 64 || p.candidates.len() > 8 {
        return Err("Mission goal or candidate limit exceeded".into());
    }
    let mut goals: BTreeMap<_, _> = m
        .goals
        .iter()
        .map(|g| (g.spec.id.as_str(), &g.spec))
        .collect();
    for g in &p.new_goals {
        id(&g.id)?;
        text(&g.question, 4000)?;
        text(&g.rationale, 4000)?;
        text(&g.resolving_evidence, 4000)?;
        strings(&g.depends_on, 16, 80)?;
        if goals.insert(&g.id, g).is_some() {
            return Err("Existing goals cannot be replaced or silently redefined".into());
        }
    }
    for g in goals.values() {
        for dependency in g.depends_on.iter().chain(g.parent_id.iter()) {
            if !goals.contains_key(dependency.as_str()) {
                return Err("Goal refers to an unknown dependency or parent".into());
            }
        }
    }
    fn visit<'a>(
        key: &'a str,
        goals: &BTreeMap<&'a str, &'a GoalSpec>,
        path: &mut BTreeSet<&'a str>,
        done: &mut BTreeSet<&'a str>,
    ) -> Result<()> {
        if done.contains(key) {
            return Ok(());
        }
        if !path.insert(key) {
            return Err("Goal dependencies contain a cycle".into());
        }
        let g = goals[key];
        for next in g.depends_on.iter().chain(g.parent_id.iter()) {
            visit(next, goals, path, done)?;
        }
        path.remove(key);
        done.insert(key);
        Ok(())
    }
    let mut done = BTreeSet::new();
    for key in goals.keys() {
        visit(key, &goals, &mut BTreeSet::new(), &mut done)?;
    }
    let mut candidates = BTreeSet::new();
    for c in &p.candidates {
        id(&c.id)?;
        if !candidates.insert(&c.id) || !goals.contains_key(c.goal_id.as_str()) {
            return Err("Candidate identities or goals are invalid".into());
        }
        for t in [&c.question, &c.uncertainty, &c.rationale, &c.expected_cost] {
            text(t, 4000)?;
        }
        text(&c.instruction, 12000)?;
        strings(&c.possible_outcomes, 6, 2000)?;
        if c.possible_outcomes.len() < 2 {
            return Err("Describe at least two possible investigation outcomes".into());
        }
        let kind = match c.kind {
            InvestigationKind::Workspace => "workspace",
            InvestigationKind::Check => "check",
            InvestigationKind::Experiment => "experiment",
        };
        if kind == "workspace" {
            if c.capability_id.is_some() {
                return Err("A Workspace investigation cannot select a host capability".into());
            }
        } else if !m
            .capabilities
            .iter()
            .any(|a| Some(&a.id) == c.capability_id.as_ref() && a.kind == kind)
        {
            return Err(
                "Investigation requests a capability outside this mission's authorization".into(),
            );
        }
    }
    match p.disposition {
        Disposition::Investigate => {
            let chosen = p
                .candidates
                .iter()
                .find(|c| Some(&c.id) == p.selected_id.as_ref())
                .ok_or("Select an available investigation")?;
            if m.goals.iter().any(|g| {
                g.spec.id == chosen.goal_id
                    && matches!(g.state.as_str(), "supported" | "refuted" | "infeasible")
            }) {
                return Err("A resolved goal needs an explicit new follow-up goal".into());
            }
            let g = goals[chosen.goal_id.as_str()];
            if g.depends_on.iter().any(|dep| {
                !m.goals.iter().any(|g| {
                    g.spec.id == *dep
                        && matches!(g.state.as_str(), "supported" | "refuted" | "infeasible")
                })
            }) {
                return Err("The selected investigation has unresolved prerequisites".into());
            }
            let signature = hash(&(
                chosen.goal_id.clone(),
                chosen.instruction.trim(),
                chosen.kind,
                &chosen.capability_id,
                m.watch_epoch,
            ))?;
            let repeats = m
                .rounds
                .iter()
                .filter(|r| {
                    r.selected.as_ref().is_some_and(|c| {
                        hash(&(
                            c.goal_id.clone(),
                            c.instruction.trim(),
                            c.kind,
                            &c.capability_id,
                            r.change_cursor,
                        ))
                        .ok()
                        .as_ref()
                            == Some(&signature)
                    })
                })
                .count();
            if repeats >= 2 {
                return Err(
                    "The same investigation has already been attempted twice; revise the agenda"
                        .into(),
                );
            }
        }
        Disposition::WaitForInput
            if p.questions.is_empty() && !m.questions.iter().any(|q| q.answer.is_none()) =>
        {
            return Err("Waiting for input requires a concrete unanswered question".into())
        }
        Disposition::WaitForChange if m.definition.policy.monitor_ids.is_empty() => {
            return Err("Waiting for change requires a selected project monitor".into())
        }
        _ => {
            if p.selected_id.is_some() {
                return Err("Only an investigation may select a candidate".into());
            }
        }
    }
    questions(&p.questions, &goals.keys().map(|s| s.to_string()).collect())
}

pub fn questions(qs: &[QuestionSpec], goals: &BTreeSet<String>) -> Result<()> {
    if qs.len() > 8 {
        return Err("Bundle at most eight research questions".into());
    }
    let mut ids = BTreeSet::new();
    for q in qs {
        id(&q.id)?;
        text(&q.question, 3000)?;
        text(&q.why_needed, 3000)?;
        if !ids.insert(&q.id)
            || q.goal_ids.len() > 16
            || q.goal_ids.iter().any(|id| !goals.contains(id))
        {
            return Err("Question identities or goal references are invalid".into());
        }
    }
    Ok(())
}
pub fn finding(f: &Finding) -> Result<()> {
    text(&f.summary, 32000)?;
    text(&f.outcome, 8000)?;
    text(&f.method, 4000)?;
    text(&f.tested_domain, 4000)?;
    strings(&f.limitations, 16, 3000)?;
    strings(&f.files, 8, 1000)?;
    if f.sources.len() > 24 {
        return Err("Use at most 24 exact evidence references".into());
    }
    for p in &f.files {
        let path = std::path::Path::new(p);
        if path.is_absolute()
            || path
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("Evidence files must be confined relative paths".into());
        }
    }
    Ok(())
}
pub fn challenge(m: &Mission, c: &Challenge) -> Result<()> {
    text(&c.summary, 16000)?;
    text(&c.tested_domain, 4000)?;
    strings(&c.limitations, 16, 3000)?;
    strings(&c.unresolved, 16, 3000)?;
    let evidence: BTreeSet<_> = m.rounds.iter().flat_map(|r| r.evidence.keys()).collect();
    let mut criteria = BTreeSet::new();
    for a in &c.criteria {
        text(&a.explanation, 4000)?;
        if a.criterion >= m.definition.criteria.len() || !criteria.insert(a.criterion) {
            return Err("Challenge criterion is missing, duplicated or outside the mission".into());
        }
        if a.evidence_ids.len() > 32 || a.evidence_ids.iter().any(|id| !evidence.contains(id)) {
            return Err("Challenge cites evidence that the host did not retain".into());
        }
        if a.status == CriterionStatus::Met && a.evidence_ids.is_empty() {
            return Err("A met criterion requires retained evidence".into());
        }
    }
    if criteria.len() != m.definition.criteria.len() {
        return Err("Challenge must assess every original completion criterion".into());
    }
    if c.goal_resolved && c.outcome == Outcome::Inconclusive {
        return Err("An inconclusive investigation cannot resolve its goal".into());
    }
    if c.mission_complete
        && (c.criteria.iter().any(|a| a.status != CriterionStatus::Met) || !c.unresolved.is_empty())
    {
        return Err("Mission completion requires all criteria met and no unresolved checks".into());
    }
    questions(
        &c.questions,
        &m.goals.iter().map(|g| g.spec.id.clone()).collect(),
    )?;
    if c.methods.len() > 3 {
        return Err("Propose at most three reusable methods per investigation".into());
    }
    for method in &c.methods {
        text(&method.name, 160)?;
        text(&method.when_to_use, 4000)?;
        text(&method.procedure, 8000)?;
        text(&method.limitations, 4000)?;
        if method.evidence_ids.is_empty()
            || method.evidence_ids.len() > 32
            || method.evidence_ids.iter().any(|id| !evidence.contains(id))
        {
            return Err("A reusable method needs retained supporting evidence".into());
        }
    }
    Ok(())
}

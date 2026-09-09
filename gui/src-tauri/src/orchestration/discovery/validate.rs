use super::{
    super::store::{hash, Result},
    model::*,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeSet;

pub fn text(v: &str, max: usize) -> Result<()> {
    if v.trim().is_empty() || v.len() > max || v.contains('\0') {
        return Err(format!("Required text is empty or exceeds {max} bytes"));
    }
    Ok(())
}
fn strings(v: &[String], min: usize, max: usize, bytes: usize) -> Result<()> {
    if !(min..=max).contains(&v.len()) {
        return Err("Invalid number of entries".into());
    }
    for s in v {
        text(s, bytes)?;
    }
    Ok(())
}
pub fn definition(d: &Definition) -> Result<()> {
    if d.schema_version != 1 {
        return Err("Unsupported self-discovery definition".into());
    }
    text(&d.prompt, 16000)?;
    text(&d.ranking_priorities, 4000)?;
    paths(&d.input_paths)?;
    if !(50..=100).contains(&d.candidate_count)
        || !(1..=25).contains(&d.shortlist_count)
        || !(1..=9).contains(&d.paper_count)
        || d.paper_count > d.shortlist_count
    {
        return Err(
            "Explore 50–100 candidates, shortlist 1–25, and select 1–9 papers with K <= N".into(),
        );
    }
    if d.proposal_reviewers > 4
        || d.proposal_revision_passes > 2
        || d.paper_reviewers > 4
        || d.revision_passes > 5
        || !(1..=16).contains(&d.research_rounds)
        || d.max_replacements > 5
    {
        return Err("Research or review limits are outside the supported range".into());
    }
    if !(20..=2000).contains(&d.max_actions)
        || !(60..=604800).contains(&d.active_seconds)
        || !(30..=7200).contains(&d.action_timeout_seconds)
        || u64::from(d.action_timeout_seconds) > d.active_seconds
        || !(1..=720).contains(&d.deadline_hours)
    {
        return Err("Automation resource limits are outside the supported range".into());
    }
    for v in [&d.author_model, &d.reviewer_model, &d.review_profile_id]
        .into_iter()
        .flatten()
    {
        text(v, 160)?;
    }
    Ok(())
}
pub fn response<T: DeserializeOwned>(v: &Value) -> Result<T> {
    let raw = v["finalText"]
        .as_str()
        .or_else(|| v["text"].as_str())
        .ok_or("The model omitted its final response")?
        .trim();
    if raw.len() > 256 * 1024 {
        return Err("Research response exceeds 256 KiB".into());
    }
    let raw = raw
        .strip_prefix("```json\n")
        .or_else(|| raw.strip_prefix("```\n"))
        .map(|s| s.strip_suffix("```").ok_or("Incomplete JSON response"))
        .transpose()?
        .unwrap_or(raw);
    serde_json::from_str(raw).map_err(|e| format!("Invalid research response: {e}"))
}
pub fn orientation(o: &Orientation) -> Result<()> {
    strings(&o.fields, 1, 6, 200)?;
    strings(&o.research_standards, 1, 16, 1500)?;
    strings(&o.contribution_forms, 1, 8, 1000)?;
    strings(&o.proposal_lenses, 3, 10, 1000)?;
    strings(&o.assumptions, 0, 12, 1500)?;
    strings(&o.constraints, 0, 12, 1500)?;
    strings(&o.literature_queries, 0, 8, 500)?;
    let c = crate::auto_review::catalog();
    for (ids, allowed) in [
        (
            &o.subject_ids,
            c.disciplines
                .iter()
                .flat_map(|g| &g.roles)
                .map(|r| r.id)
                .collect::<BTreeSet<_>>(),
        ),
        (
            &o.method_ids,
            c.method_families
                .iter()
                .flat_map(|g| &g.roles)
                .map(|r| r.id)
                .collect(),
        ),
    ] {
        if ids.len() > 6
            || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
            || ids.iter().any(|s| !allowed.contains(s.as_str()))
        {
            return Err("Orientation selected an unknown or duplicate review specialist".into());
        }
    }
    Ok(())
}
pub fn proposals(v: &[Proposal], expected: usize) -> Result<()> {
    if v.len() != expected {
        return Err(format!(
            "Expected exactly {expected} proposals in this batch"
        ));
    }
    for p in v {
        text(&p.title, 300)?;
        text(&p.question, 3000)?;
        text(&p.contribution, 4000)?;
        text(&p.method, 3000)?;
        text(&p.first_test, 2000)?;
        strings(&p.required_evidence, 1, 12, 1500)?;
        strings(&p.related_work, 0, 12, 1500)?;
        strings(&p.risks, 1, 10, 1500)?;
    }
    Ok(())
}
pub fn assessments(
    v: &[CandidateAssessment],
    expected: &[u32],
    candidates: &[Candidate],
) -> Result<()> {
    let ids: BTreeSet<_> = v.iter().map(|a| a.candidate_id).collect();
    if ids.len() != v.len() || ids != expected.iter().copied().collect() {
        return Err("Assessment does not cover the exact candidate batch".into());
    }
    for a in v {
        for score in [a.contribution, a.feasibility, a.information_value] {
            if !(1..=5).contains(&score) {
                return Err("Assessment categories must use 1–5 anchored ratings".into());
            }
        }
        text(&a.cluster, 160)?;
        text(&a.strongest_objection, 4000)?;
        text(&a.resolution, 4000)?;
        text(&a.uncertainty, 3000)?;
        if a.duplicate_of
            .is_some_and(|id| id >= a.candidate_id || !candidates.iter().any(|c| c.id == id))
        {
            return Err("A duplicate must refer to an earlier existing candidate".into());
        }
    }
    Ok(())
}
pub fn selection(s: &Selection, p: &Portfolio) -> Result<()> {
    let count = p.run.definition.shortlist_count.min(
        p.candidates
            .iter()
            .filter(|c| p.run.deep_candidates.contains(&c.id) && c.eligible())
            .count(),
    );
    if s.shortlist.len() != count
        || s.recommended.len() != count.min(p.run.definition.paper_count)
        || s.shortlist.iter().collect::<BTreeSet<_>>().len() != s.shortlist.len()
        || s.recommended.iter().collect::<BTreeSet<_>>().len() != s.recommended.len()
        || s.recommended.iter().any(|id| !s.shortlist.contains(id))
        || s.shortlist.iter().any(|id| {
            !p.run.deep_candidates.contains(id)
                || !p.candidates.iter().any(|c| c.id == *id && c.eligible())
        })
    {
        return Err("Shortlist and selection must contain the requested number of distinct eligible, assessed candidates (or disclose a shortfall)".into());
    }
    text(&s.rationale, 8000)
}
pub fn investigation(v: &Investigation) -> Result<()> {
    text(&v.contract, 12000)?;
    text(&v.summary, 16000)?;
    text(&v.reason, 4000)?;
    strings(&v.claims, 0, 30, 3000)?;
    strings(&v.limitations, 0, 20, 3000)?;
    paths(&v.files)?;
    if v.sources.len() > 20 {
        return Err("Too many source references".into());
    }
    Ok(())
}
pub fn challenge(v: &Challenge) -> Result<()> {
    text(&v.assessment, 12000)?;
    strings(&v.unresolved, 0, 24, 2000)
}
pub fn paths(v: &[String]) -> Result<()> {
    strings(v, 0, 20, 500)?;
    for path in v {
        if std::path::Path::new(path)
            .components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
            || path.contains('\\')
        {
            return Err("Artifacts require confined relative file paths".into());
        }
    }
    Ok(())
}
pub fn manuscript(v: &Manuscript, p: &Paper) -> Result<()> {
    text(&v.title, 300)?;
    text(&v.abstract_text, 6000)?;
    text(&v.markdown, 100 * 1024)?;
    text(&v.latex, 100 * 1024)?;
    if v.bibliography.len() > 32 * 1024 || v.bibliography.contains('\0') {
        return Err("Bibliography exceeds its limit".into());
    }
    strings(&v.evidence_ids, 0, 100, 160)?;
    strings(&v.limitations, 0, 24, 2000)?;
    strings(&v.response_to_review, 0, 80, 2000)?;
    paths(&v.files)?;
    if v.evidence_ids.iter().any(|id| !p.evidence.contains_key(id)) {
        return Err("Manuscript cites evidence outside its research record".into());
    }
    if let Some(old) = p.versions.last() {
        if version_hash(v)? == old.hash {
            return Err("Revision is unchanged".into());
        }
    }
    Ok(())
}
pub fn version_hash(v: &Manuscript) -> Result<String> {
    hash(&(&v.markdown, &v.latex, &v.bibliography, &v.evidence_ids))
}
pub fn review(v: &Review) -> Result<()> {
    text(&v.summary, 12000)?;
    strings(&v.strengths, 0, 20, 2000)?;
    strings(&v.limitations, 0, 24, 2000)?;
    if v.findings.len() > 60 {
        return Err("Too many review findings".into());
    }
    for f in &v.findings {
        text(&f.claim, 3000)?;
        text(&f.evidence, 3000)?;
        text(&f.resolution, 3000)?;
    }
    Ok(())
}
pub fn assessment(v: &Assessment) -> Result<()> {
    for s in [
        &v.summary,
        &v.contribution,
        &v.support,
        &v.originality,
        &v.completeness,
    ] {
        text(s, 6000)?;
    }
    strings(&v.remaining_work, 0, 24, 2000)
}
pub fn ranking(v: &[RankedPaper], p: &Portfolio) -> Result<()> {
    let papers: BTreeSet<_> = p
        .papers
        .iter()
        .filter(|p| p.assessment.is_some())
        .map(|p| p.candidate_id)
        .collect();
    if v.len() != papers.len()
        || v.iter().map(|r| r.candidate_id).collect::<BTreeSet<_>>() != papers
    {
        return Err("Ranking must include each assessed paper exactly once".into());
    }
    for r in v {
        if r.rank == 0 || r.rank > v.len() {
            return Err("Invalid paper rank".into());
        }
        text(&r.reason, 6000)?;
        text(&r.uncertainty, 3000)?;
    }
    for a in v {
        let sound = p
            .papers
            .iter()
            .find(|p| p.candidate_id == a.candidate_id)
            .and_then(|p| p.assessment.as_ref())
            .is_some_and(|a| a.sound);
        if sound
            && v.iter().any(|b| {
                b.rank <= a.rank
                    && p.papers
                        .iter()
                        .find(|p| p.candidate_id == b.candidate_id)
                        .and_then(|p| p.assessment.as_ref())
                        .is_some_and(|a| !a.sound)
            })
        {
            return Err("A paper with unresolved material defects cannot rank above or tie an eligible paper".into());
        }
    }
    Ok(())
}

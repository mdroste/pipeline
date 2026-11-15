//! Evaluate one frozen manuscript against its own evidence and objections.
//! Preserve the assessor reply beside the host decision; delivery remains a separate milestone.
use super::{model::*, validate};
use crate::orchestration::store::Result;
use serde_json::Value;
pub(super) fn apply(p: &mut Portfolio, paper: usize, output: &Value) -> Result<()> {
    let v: Assessment = validate::response(output)?;
    validate::assessment(&v)?;
    let needs_work = super::engine::needs_revision(&p.papers[paper]);
    let work = &mut p.papers[paper];
    let version = work.versions.last().ok_or("No manuscript to assess")?;
    if version.claim_record.is_some() {
        return Err("This manuscript version already has an assessment".into());
    }
    let targets = version.subjects(work);
    if v.claim_assessments
        .iter()
        .any(|c| targets.get(c.subject) != Some(&c.claim))
    {
        return Err("Each assessed claim must match its frozen manuscript subject".into());
    }
    let mut retained = work.evidence.clone();
    retained.extend(version.artifacts.clone());
    retained.extend(
        p.run
            .input_artifacts
            .iter()
            .map(|(k, v)| (format!("input:{k}"), v.clone())),
    );
    for (i, review) in version.reviews.iter().enumerate() {
        retained.insert(
            format!("referee-{}", i + 1),
            serde_json::json!({"kind":"refereeAssessment","version":version.hash,"report":review}),
        );
    }
    if let Some(r) = &version.external_review {
        retained.insert(
            "structured-review".into(),
            serde_json::json!({"kind":"refereeAssessment","version":version.hash,"report":r}),
        );
    }
    let mut objections = Vec::new();
    let open = work
        .challenges
        .last()
        .map_or(&[][..], |c| c.unresolved.as_slice());
    let mut answered = std::collections::BTreeSet::new();
    if v.objection_responses.len() > open.len() {
        return Err("Too many objection responses".into());
    }
    for response in &v.objection_responses {
        if response.objection >= open.len()
            || !answered.insert(response.objection)
            || response.explanation.trim().is_empty()
            || response.explanation.len() > 2000
            || !v.claim_assessments.iter().any(|c| {
                c.subject == response.subject
                    && c.inspections.iter().any(|i| {
                        i.evidence_id == response.evidence_id
                            && i.role != crate::orchestration::claims::Role::Review
                    })
            })
        {
            return Err(
                "Objection response must identify the objection and an inspected claim reference"
                    .into(),
            );
        }
    }
    objections.extend(
        open.iter()
            .enumerate()
            .filter(|(i, _)| !answered.contains(i))
            .map(|(_, s)| s.clone()),
    );
    for review in &version.reviews {
        for f in &review.findings {
            if matches!(f.severity, Severity::Fatal | Severity::Major) {
                objections.push(format!("{}: {}", f.claim, f.resolution));
            }
        }
    }
    if needs_work && objections.is_empty() {
        objections.push("The structured Review is incomplete or retains material findings.".into());
    }
    if v.claim_assessments
        .iter()
        .any(|c| c.verdict == crate::orchestration::claims::Verdict::Refuted)
    {
        objections.push(
            "The frozen manuscript still asserts a central claim the assessor refuted.".into(),
        );
    }
    if !v.sound {
        objections.push(format!(
            "The assessor did not endorse this manuscript: {}",
            v.summary
        ));
    }
    let record = crate::orchestration::claims::assess(
        version.hash.clone(),
        (
            "Final paper assessor",
            p.run
                .active_child
                .as_deref()
                .ok_or("Assessment action identity is missing")?,
        ),
        &targets,
        &v.claim_assessments,
        &retained,
        objections,
        v.sound,
    )?;
    let supported = record.supported;
    work.versions.last_mut().unwrap().claim_record = Some(record);

    p.papers[paper].state = if supported { "complete" } else { "incomplete" }.into();
    p.papers[paper].reason = v.summary.clone();
    p.papers[paper].assessment = Some(v);
    Ok(())
}

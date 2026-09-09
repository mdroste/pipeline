//! Pure portfolio transitions. Model judgments never authorize tools or accept evidence.
use super::{
    super::store::{hash, Result},
    model::*,
    validate,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposals {
    proposals: Vec<Proposal>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Assessments {
    assessments: Vec<CandidateAssessment>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ranking {
    papers: Vec<RankedPaper>,
}

pub fn ready(p: &mut Portfolio, phase: Phase, reason: &str) {
    p.run.phase = phase;
    p.run.state = "running".into();
    p.run.reason = reason.into();
    p.run.due_at = Some(super::super::store::now());
}
fn sorted_candidates(p: &Portfolio) -> Vec<u32> {
    let mut candidates = p
        .candidates
        .iter()
        .filter(|c| c.eligible())
        .collect::<Vec<_>>();
    candidates.sort_by_key(|c| (std::cmp::Reverse(c.score()), c.id));
    candidates.into_iter().map(|c| c.id).collect()
}
fn after_screen(p: &mut Portfolio) {
    let count = p
        .run
        .definition
        .candidate_count
        .min(20.max(2 * p.run.definition.shortlist_count));
    p.run.deep_candidates = sorted_candidates(p).into_iter().take(count).collect();
    if p.run.deep_candidates.is_empty() {
        p.run.state = "partial".into();
        p.run.reason="No candidate cleared the feasibility and contribution screen; all proposals and objections are retained".into();
        p.run.due_at = None;
    } else if p.run.definition.proposal_reviewers == 0 {
        ready(p, Phase::Shortlist, "Preparing the project shortlist");
    } else {
        ready(
            p,
            Phase::Assess {
                index: 0,
                reviewer: 0,
            },
            "Independent project assessment",
        );
    }
}
fn after_research(p: &mut Portfolio, index: usize, next: ResearchNext) {
    if next == ResearchNext::Abandon {
        p.papers[index].state = "abandoned".into();
        p.papers[index].reason =
            "The investigation was assessed as infeasible; retained work is available".into();
        next_paper(p, index);
    } else if next == ResearchNext::Draft
        || p.papers[index].rounds.len() >= p.run.definition.research_rounds
        || p.papers[index].stagnant_rounds >= 2
    {
        p.papers[index].state = "drafting".into();
        ready(
            p,
            Phase::Draft { paper: index },
            "Writing from the retained research dossier",
        );
    } else {
        ready(
            p,
            Phase::Research { paper: index },
            "Investigating the next unresolved question",
        );
    }
}
fn needs_revision(p: &Paper) -> bool {
    p.versions.last().is_some_and(|v| {
        v.reviews.iter().any(|r| {
            r.findings
                .iter()
                .any(|f| matches!(f.severity, Severity::Fatal | Severity::Major))
        }) || v.external_review.as_ref().is_some_and(|r| {
            r["complete"] != true
                || r["highPriorityCount"].as_u64().unwrap_or(1) > 0
                || r["unknownPriorityCount"].as_u64().unwrap_or(1) > 0
        })
    })
}
fn after_reviews(p: &mut Portfolio, index: usize) {
    if needs_revision(&p.papers[index])
        && p.papers[index].versions.len() <= p.run.definition.revision_passes
    {
        ready(
            p,
            Phase::Draft { paper: index },
            "Revising the paper in response to its exact reviews",
        );
    } else {
        ready(
            p,
            Phase::AssessPaper { paper: index },
            "Independently assessing the final paper",
        );
    }
}
pub fn next_paper(p: &mut Portfolio, index: usize) {
    if index + 1 < p.papers.len() {
        ready(
            p,
            Phase::Research { paper: index + 1 },
            "Developing the next selected project",
        );
        return;
    }
    let count = p.papers.iter().filter(|p| p.state == "complete").count();
    if p.run.definition.mode == Mode::Unsupervised
        && count < p.run.definition.paper_count
        && p.run.replacements < p.run.definition.max_replacements
    {
        if let Some(id) = sorted_candidates(p)
            .into_iter()
            .find(|id| p.run.deep_candidates.contains(id) && !p.run.selected.contains(id))
        {
            p.run.replacements += 1;
            p.run.selected.push(id);
            p.papers.push(Paper::new(id));
            ready(
                p,
                Phase::Research {
                    paper: p.papers.len() - 1,
                },
                "Developing an eligible reserve project within the original budget",
            );
            return;
        }
    }
    if p.papers.iter().any(|p| p.assessment.is_some()) {
        ready(p, Phase::Rank, "Comparing the final assessed papers");
    } else {
        ready(
            p,
            Phase::Deliver,
            "Preparing the retained research record; no complete paper is available",
        );
    }
}
/// `evidence` and `artifacts` are host-captured values, never accepted from model JSON.
pub fn consume(
    p: &mut Portfolio,
    output: &Value,
    evidence: BTreeMap<String, Value>,
    artifacts: BTreeMap<String, Value>,
) -> Result<()> {
    match p.run.phase.clone() {
        Phase::Orient => {
            let value: Orientation = validate::response(output)?;
            validate::orientation(&value)?;
            let acquire =
                p.run.definition.acquire_literature && !value.literature_queries.is_empty();
            p.run.orientation = Some(value);
            ready(
                p,
                if acquire {
                    Phase::Acquire { index: 0 }
                } else {
                    Phase::Generate
                },
                "Research standards identified; exploring project proposals",
            );
        }
        Phase::Acquire { index } => {
            p.run.literature.push(output.clone());
            let n = p
                .run
                .orientation
                .as_ref()
                .ok_or("Orientation missing")?
                .literature_queries
                .len();
            ready(
                p,
                if index + 1 < n {
                    Phase::Acquire { index: index + 1 }
                } else {
                    Phase::Generate
                },
                "Literature access recorded",
            );
        }
        Phase::Generate => {
            let v: Proposals = validate::response(output)?;
            validate::proposals(
                &v.proposals,
                BATCH.min(p.run.definition.candidate_count - p.candidates.len()),
            )?;
            for proposal in v.proposals {
                p.candidates.push(Candidate {
                    id: p.candidates.len() as u32 + 1,
                    proposal,
                    assessments: Vec::new(),
                    previous_versions: Vec::new(),
                });
            }
            if p.candidates.len() == p.run.definition.candidate_count {
                ready(
                    p,
                    Phase::Screen { offset: 0 },
                    "Adversarially screening the generated projects",
                );
            }
        }
        Phase::Screen { offset } => {
            let ids = p
                .candidates
                .iter()
                .skip(offset)
                .take(BATCH)
                .map(|c| c.id)
                .collect::<Vec<_>>();
            let v: Assessments = validate::response(output)?;
            validate::assessments(&v.assessments, &ids, &p.candidates)?;
            for a in v.assessments {
                p.candidates
                    .iter_mut()
                    .find(|c| c.id == a.candidate_id)
                    .ok_or("Candidate missing")?
                    .assessments
                    .push(a);
            }
            if offset + BATCH >= p.candidates.len() {
                after_screen(p);
            } else {
                ready(
                    p,
                    Phase::Screen {
                        offset: offset + BATCH,
                    },
                    "Screening the next proposal batch",
                );
            }
        }
        Phase::Assess { index, reviewer } => {
            let id = p.run.deep_candidates[index];
            let v: Assessments = validate::response(output)?;
            validate::assessments(&v.assessments, &[id], &p.candidates)?;
            p.candidates
                .iter_mut()
                .find(|c| c.id == id)
                .ok_or("Candidate missing")?
                .assessments
                .extend(v.assessments);
            let next = if reviewer + 1 < p.run.definition.proposal_reviewers {
                Phase::Assess {
                    index,
                    reviewer: reviewer + 1,
                }
            } else if p.candidates.iter().find(|c| c.id == id).is_some_and(|c| {
                c.previous_versions.len() < p.run.definition.proposal_revision_passes
            }) {
                Phase::ReviseProposal { index }
            } else if index + 1 < p.run.deep_candidates.len() {
                Phase::Assess {
                    index: index + 1,
                    reviewer: 0,
                }
            } else {
                Phase::Shortlist
            };
            ready(p, next, "Project assessment retained");
        }
        Phase::ReviseProposal { index } => {
            let proposal: Proposal = validate::response(output)?;
            validate::proposals(std::slice::from_ref(&proposal), 1)?;
            let id = p.run.deep_candidates[index];
            let c = p
                .candidates
                .iter_mut()
                .find(|c| c.id == id)
                .ok_or("Candidate missing")?;
            c.previous_versions.push(ProposalVersion {
                proposal: std::mem::replace(&mut c.proposal, proposal),
                assessments: std::mem::take(&mut c.assessments),
            });
            ready(
                p,
                Phase::Assess { index, reviewer: 0 },
                "Independently reassessing the revised proposal",
            );
        }
        Phase::Shortlist => {
            let s: Selection = validate::response(output)?;
            validate::selection(&s, p)?;
            p.run.selection_hash = Some(hash(&(&s, &p.candidates))?);
            p.run.selection = Some(s.clone());
            if s.shortlist.is_empty() {
                ready(
                    p,
                    Phase::Deliver,
                    "No proposal cleared detailed assessment; retaining the research record",
                );
            } else if p.run.definition.mode == Mode::Supervised {
                p.run.state = "awaitingSelection".into();
                p.run.phase = Phase::Select;
                p.run.due_at = None;
                p.run.reason = "Choose the projects to develop from the reviewed shortlist".into();
            } else {
                p.run.selected = s.recommended.clone();
                p.papers = s.recommended.into_iter().map(Paper::new).collect();
                ready(
                    p,
                    Phase::Research { paper: 0 },
                    "Automatic project selection saved; developing the papers",
                );
            }
        }
        Phase::Research { paper } => {
            let v: Investigation = validate::response(output)?;
            validate::investigation(&v)?;
            if p.papers[paper]
                .rounds
                .last()
                .is_some_and(|old| old.summary == v.summary && old.claims == v.claims)
            {
                p.papers[paper].stagnant_rounds += 1;
            }
            let next = v.next;
            p.papers[paper].rounds.push(v);
            p.papers[paper].evidence.extend(evidence);
            if p.run.definition.challenge_research {
                ready(
                    p,
                    Phase::Challenge { paper },
                    "Independently challenging the investigation",
                );
            } else {
                after_research(p, paper, next);
            }
        }
        Phase::Challenge { paper } => {
            let v: Challenge = validate::response(output)?;
            validate::challenge(&v)?;
            let rounds = &p.papers[paper].rounds;
            let repeated = rounds.len() > 1
                && rounds[rounds.len() - 1].summary == rounds[rounds.len() - 2].summary
                && rounds[rounds.len() - 1].claims == rounds[rounds.len() - 2].claims;
            if v.progress && !repeated {
                p.papers[paper].stagnant_rounds = 0;
            } else if !v.progress {
                p.papers[paper].stagnant_rounds += 1;
            }
            let next = v.next;
            p.papers[paper].challenges.push(v);
            after_research(p, paper, next);
        }
        Phase::Draft { paper } => {
            let v: Manuscript = validate::response(output)?;
            validate::manuscript(&v, &p.papers[paper])?;
            let version = PaperVersion {
                version: p.papers[paper].versions.len() + 1,
                hash: validate::version_hash(&v)?,
                manuscript: v,
                artifacts,
                reviews: Vec::new(),
                external_review: None,
            };
            p.papers[paper].versions.push(version);
            p.papers[paper].state = "reviewing".into();
            if p.run.definition.paper_reviewers > 0 {
                ready(p, Phase::Review { paper }, "Reviewing the frozen paper");
            } else if p.run.definition.review_profile_id.is_some() {
                ready(
                    p,
                    Phase::ExternalReview { paper },
                    "Running the selected Review profile",
                );
            } else {
                ready(
                    p,
                    Phase::AssessPaper { paper },
                    "Assessing a paper with adversarial review disabled",
                );
            }
        }
        Phase::Review { paper } => {
            let v: Review = validate::response(output)?;
            validate::review(&v)?;
            let version = p.papers[paper]
                .versions
                .last_mut()
                .ok_or("Manuscript missing")?;
            version.reviews.push(v);
            if version.reviews.len() < p.run.definition.paper_reviewers {
                ready(
                    p,
                    Phase::Review { paper },
                    "Independent second assessment of the same paper",
                );
            } else if p.run.definition.review_profile_id.is_some() {
                ready(
                    p,
                    Phase::ExternalReview { paper },
                    "Running the selected Review profile",
                );
            } else {
                after_reviews(p, paper);
            }
        }
        Phase::ExternalReview { paper } => {
            let version = p.papers[paper]
                .versions
                .last_mut()
                .ok_or("Manuscript missing")?;
            let expected = version
                .artifacts
                .get("paper.md")
                .ok_or("Manuscript snapshot missing")?;
            if output["reviewedArtifact"]["hash"] != expected["hash"] {
                return Err("Review does not match the frozen manuscript".into());
            }
            version.external_review = Some(output.clone());
            after_reviews(p, paper);
        }
        Phase::AssessPaper { paper } => {
            let mut v: Assessment = validate::response(output)?;
            validate::assessment(&v)?;
            if needs_revision(&p.papers[paper]) {
                v.sound = false;
            }
            p.papers[paper].state = if v.sound { "complete" } else { "incomplete" }.into();
            p.papers[paper].reason = v.summary.clone();
            p.papers[paper].assessment = Some(v);
            next_paper(p, paper);
        }
        Phase::Rank => {
            let v: Ranking = validate::response(output)?;
            validate::ranking(&v.papers, p)?;
            p.run.ranking = v.papers;
            p.run.ranking.sort_by_key(|r| (r.rank, r.candidate_id));
            ready(p, Phase::Deliver, "Final paper ranking recorded");
        }
        Phase::Deliver => {
            let n = p.papers.iter().filter(|p| p.state == "complete").count();
            p.run.state = if n >= p.run.definition.paper_count {
                "completed"
            } else {
                "partial"
            }
            .into();
            p.run.reason=format!("Delivered {n} complete paper drafts of {} requested; reviews, limitations and other research are retained",p.run.definition.paper_count);
            p.run.due_at = None;
        }
        Phase::Select => return Err("Project selection requires a saved selection decision".into()),
    }
    p.run.repairs = 0;
    Ok(())
}

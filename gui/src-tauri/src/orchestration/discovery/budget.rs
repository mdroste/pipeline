//! Admission counts actions, not model calls. These are structural bounds on a
//! successful, unretried path to the requested portfolio, not expected outcomes.
use super::model::{Definition, Mode, BATCH};
use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub batches: usize,
    pub team_actions: usize,
    pub before_assessment: usize,
    pub screened_at_limit: usize,
    pub first_assessment: Option<usize>,
    pub shortlist_min: usize,
    pub portfolio_min: usize,
    pub conditional_high: usize,
    pub deep_limit: usize,
    pub reachable: bool,
}
pub(super) fn deep_limit(d: &Definition) -> usize {
    d.candidate_count.min(20.max(2 * d.shortlist_count))
}
pub(super) fn team_cost(count: u8) -> u32 {
    1 + u32::from(count)
}
pub(super) fn preview(d: &Definition) -> Preview {
    let batches = d.candidate_count.div_ceil(BATCH);
    let team_actions = if d.idea_agents == 0 {
        0
    } else {
        team_cost(d.idea_agents) as usize
    };
    let before_assessment = 1 + team_actions + 2 * batches;
    let screened_at_limit = (d.max_actions as usize)
        .saturating_sub(1 + team_actions + batches)
        .saturating_mul(BATCH)
        .min(d.candidate_count);
    let first_assessment = (d.proposal_reviewers > 0).then_some(before_assessment + 1);
    let assess = if d.proposal_reviewers == 0 {
        0
    } else {
        d.proposal_reviewers * (1 + d.proposal_revision_passes) + d.proposal_revision_passes
    };
    let deep_limit = deep_limit(d);
    let shortlist_min = before_assessment + d.shortlist_count * assess + 1;
    let round = 1 + usize::from(d.challenge_research);
    let version = 1 + d.paper_reviewers + usize::from(d.review_profile_id.is_some());
    let per_paper_min = round + version + 1; // research, draft & review, final assessment
    let portfolio_min = shortlist_min + d.paper_count * per_paper_min + 2; // rank & deliver
    let max_papers = d.paper_count
        + if d.mode == Mode::Unsupervised {
            d.max_replacements
        } else {
            0
        };
    let conditional_high = before_assessment
        + if d.acquire_literature { 8 } else { 0 }
        + deep_limit * assess
        + 1
        + max_papers * (round * d.research_rounds + version * (1 + d.revision_passes) + 1)
        + 2;
    Preview {
        batches,
        team_actions,
        before_assessment,
        screened_at_limit,
        first_assessment,
        shortlist_min,
        portfolio_min,
        conditional_high,
        deep_limit,
        reachable: d.max_actions as usize >= portfolio_min,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn twenty_actions_cannot_assess_one_of_seventy_five_candidates() {
        let mut d = Definition {
            max_actions: 20,
            acquire_literature: false,
            ..Default::default()
        };
        let p = preview(&d);
        assert_eq!(
            (p.batches, p.before_assessment, p.first_assessment),
            (15, 31, Some(32))
        );
        assert!(!p.reachable);
        assert_eq!(p.screened_at_limit, 20);
        d.max_actions = p.portfolio_min as u32 - 1;
        assert!(!preview(&d).reachable);
        d.max_actions += 1;
        assert!(preview(&d).reachable);
    }
    #[test]
    fn conditional_paths_charge_native_agents_reviews_revisions_and_replacements() {
        let mut d = Definition {
            candidate_count: 51,
            shortlist_count: 2,
            paper_count: 1,
            idea_agents: 3,
            proposal_reviewers: 0,
            proposal_revision_passes: 2,
            paper_reviewers: 0,
            review_profile_id: Some("external".into()),
            challenge_research: false,
            research_rounds: 2,
            revision_passes: 1,
            max_replacements: 1,
            mode: Mode::Unsupervised,
            ..Default::default()
        };
        let p = preview(&d);
        assert_eq!(
            (p.batches, p.before_assessment, p.first_assessment),
            (11, 27, None)
        );
        assert_eq!(p.shortlist_min, 28);
        assert_eq!(p.portfolio_min, 34);
        assert_eq!(p.conditional_high, 28 + 8 + 2 * (2 + 2 * 2 + 1) + 2);
        d.proposal_reviewers = 2;
        assert_eq!(preview(&d).portfolio_min, 34 + 2 * (3 * 2 + 2));
        d.mode = Mode::Supervised;
        assert_eq!(
            preview(&d).conditional_high,
            p.conditional_high + 20 * 8 - 7
        );
    }
}

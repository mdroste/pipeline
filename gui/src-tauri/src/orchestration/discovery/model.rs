use super::super::store::Scope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const CONTRACT: &str = "self-discovery-v1";
pub const BATCH: usize = 5;
pub const MAX_IDEA_AGENTS: u8 = 8;
fn no_agents(n: &u8) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Supervised,
    Unsupervised,
}
impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Supervised => "Research discovery — choose projects yourself",
            Self::Unsupervised => "Research discovery — let Pipeline choose projects",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definition {
    pub schema_version: u32,
    pub mode: Mode,
    pub prompt: String,
    pub candidate_count: usize,
    // Omit zero so pre-feature Start fingerprints remain identical.
    #[serde(default, skip_serializing_if = "no_agents")]
    pub idea_agents: u8,
    pub shortlist_count: usize,
    pub paper_count: usize,
    pub proposal_reviewers: usize,
    #[serde(default)]
    pub proposal_revision_passes: usize,
    pub paper_reviewers: usize,
    pub revision_passes: usize,
    pub research_rounds: usize,
    pub challenge_research: bool,
    pub max_replacements: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_api_cost_cents: Option<u32>,
    pub max_actions: u32,
    pub active_seconds: u64,
    pub action_timeout_seconds: u32,
    pub deadline_hours: u32,
    pub allow_computation: bool,
    pub acquire_literature: bool,
    #[serde(default)]
    pub input_paths: Vec<String>,
    pub review_profile_id: Option<String>,
    pub author_model: Option<String>,
    pub reviewer_model: Option<String>,
    pub ranking_priorities: String,
}
impl Default for Definition {
    fn default() -> Self {
        Self { schema_version: 1, mode: Mode::Supervised, prompt: String::new(),
            candidate_count: 75, idea_agents: 0, shortlist_count: 10, paper_count: 5,
            proposal_reviewers: 2, proposal_revision_passes: 1, paper_reviewers: 2, revision_passes: 2,
            research_rounds: 8, challenge_research: true, max_replacements: 2,
            max_api_cost_cents: None,
            max_actions: 500, active_seconds: 172800, action_timeout_seconds: 1800,
            deadline_hours: 168, allow_computation: true, acquire_literature: true, input_paths: Vec::new(),
            review_profile_id: None, author_model: None, reviewer_model: None,
            ranking_priorities: "Soundness, substantive contribution, evidence, originality, completeness and clarity; prefer complementary projects.".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Orientation {
    pub fields: Vec<String>,
    pub subject_ids: Vec<String>,
    pub method_ids: Vec<String>,
    pub research_standards: Vec<String>,
    pub contribution_forms: Vec<String>,
    pub proposal_lenses: Vec<String>,
    pub assumptions: Vec<String>,
    pub constraints: Vec<String>,
    pub literature_queries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Proposal {
    pub title: String,
    pub question: String,
    pub contribution: String,
    pub method: String,
    pub first_test: String,
    pub required_evidence: Vec<String>,
    pub related_work: Vec<String>,
    pub risks: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateAssessment {
    pub candidate_id: u32,
    pub eligible: bool,
    pub contribution: u8,
    pub feasibility: u8,
    pub information_value: u8,
    pub cluster: String,
    pub duplicate_of: Option<u32>,
    pub strongest_objection: String,
    pub resolution: String,
    pub uncertainty: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: u32,
    pub proposal: Proposal,
    pub assessments: Vec<CandidateAssessment>,
    #[serde(default)]
    pub previous_versions: Vec<ProposalVersion>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalVersion {
    pub proposal: Proposal,
    pub assessments: Vec<CandidateAssessment>,
}
impl Candidate {
    pub fn eligible(&self) -> bool {
        !self.assessments.is_empty()
            && self
                .assessments
                .iter()
                .all(|a| a.eligible && a.duplicate_of.is_none())
    }
    pub fn score(&self) -> u32 {
        self.assessments
            .iter()
            .map(|a| u32::from(a.contribution + a.feasibility + a.information_value))
            .min()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub shortlist: Vec<u32>,
    pub recommended: Vec<u32>,
    pub rationale: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Investigation {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub execution_ids: Vec<String>,
    pub contract: String,
    pub summary: String,
    pub claims: Vec<String>,
    pub limitations: Vec<String>,
    pub sources: Vec<crate::workbench::desk::ResearchObjectRef>,
    pub files: Vec<String>,
    pub next: ResearchNext,
    pub reason: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResearchNext {
    Continue,
    Draft,
    Abandon,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Challenge {
    pub assessment: String,
    pub unresolved: Vec<String>,
    pub progress: bool,
    pub next: ResearchNext,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manuscript {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub central_claims: Vec<String>,
    pub title: String,
    pub abstract_text: String,
    pub markdown: String,
    pub latex: String,
    pub bibliography: String,
    pub evidence_ids: Vec<String>,
    pub limitations: Vec<String>,
    pub response_to_review: Vec<String>,
    pub files: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Fatal,
    Major,
    Minor,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Finding {
    pub claim: String,
    pub severity: Severity,
    pub evidence: String,
    pub resolution: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Review {
    pub summary: String,
    pub findings: Vec<Finding>,
    pub strengths: Vec<String>,
    pub limitations: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperVersion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_delivered: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_record: Option<crate::orchestration::claims::Record>,
    pub version: usize,
    pub manuscript: Manuscript,
    pub hash: String,
    pub artifacts: BTreeMap<String, Value>,
    pub reviews: Vec<Review>,
    pub external_review: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectionResponse {
    pub objection: usize,
    pub subject: usize,
    pub evidence_id: String,
    pub explanation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assessment {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objection_responses: Vec<ObjectionResponse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claim_assessments: Vec<crate::orchestration::claims::Submission>,
    pub summary: String,
    pub sound: bool,
    pub contribution: String,
    pub support: String,
    pub originality: String,
    pub completeness: String,
    pub remaining_work: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paper {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infeasibility: Option<crate::orchestration::claims::Record>,
    pub candidate_id: u32,
    pub state: String,
    pub reason: String,
    pub scope: Option<Scope>,
    pub authority: Option<String>,
    pub rounds: Vec<Investigation>,
    pub challenges: Vec<Challenge>,
    pub evidence: BTreeMap<String, Value>,
    pub versions: Vec<PaperVersion>,
    pub assessment: Option<Assessment>,
    pub stagnant_rounds: usize,
}
impl PaperVersion {
    pub fn subjects(&self, paper: &Paper) -> Vec<String> {
        if !self.manuscript.central_claims.is_empty() {
            return self.manuscript.central_claims.clone();
        }
        paper
            .rounds
            .iter()
            .rev()
            .find(|r| !r.claims.is_empty())
            .map(|r| r.claims.clone())
            .unwrap_or_else(|| vec![self.manuscript.title.clone()])
    }
}
impl Paper {
    pub fn supported(&self) -> bool {
        self.versions
            .last()
            .and_then(|v| v.claim_record.as_ref())
            .is_some_and(|r| r.supported)
    }

    pub fn new(candidate_id: u32) -> Self {
        Self {
            candidate_id,
            infeasibility: None,
            state: "researching".into(),
            reason: String::new(),
            scope: None,
            authority: None,
            rounds: Vec::new(),
            challenges: Vec::new(),
            evidence: BTreeMap::new(),
            versions: Vec::new(),
            assessment: None,
            stagnant_rounds: 0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankedPaper {
    pub candidate_id: u32,
    pub rank: usize,
    pub reason: String,
    pub uncertainty: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Phase {
    Orient,
    Acquire { index: usize },
    Brainstorm,
    Generate,
    Screen { offset: usize },
    Assess { index: usize, reviewer: usize },
    ReviseProposal { index: usize },
    Shortlist,
    Select,
    Research { paper: usize },
    Challenge { paper: usize },
    Draft { paper: usize },
    Review { paper: usize },
    ExternalReview { paper: usize },
    AssessPaper { paper: usize },
    Rank,
    Deliver,
}
impl Phase {
    pub fn paper(&self) -> Option<usize> {
        match self {
            Self::Research { paper }
            | Self::Challenge { paper }
            | Self::Draft { paper }
            | Self::Review { paper }
            | Self::ExternalReview { paper }
            | Self::AssessPaper { paper } => Some(*paper),
            _ => None,
        }
    }
    pub fn reviewer(&self) -> bool {
        matches!(
            self,
            Self::Screen { .. }
                | Self::Assess { .. }
                | Self::Shortlist
                | Self::Challenge { .. }
                | Self::Review { .. }
                | Self::AssessPaper { .. }
                | Self::Rank
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub revision: i64,
    pub definition: Definition,
    pub workspace_id: String,
    pub source_session_id: String,
    pub source_scope: Scope,
    pub source_authority: String,
    pub source_context: String,
    pub input_artifacts: BTreeMap<String, Value>,
    pub catalog_revision: String,
    pub state: String,
    pub reason: String,
    pub phase: Phase,
    pub orientation: Option<Orientation>,
    pub literature: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub idea_seeds: Vec<String>,
    pub deep_candidates: Vec<u32>,
    pub selection: Option<Selection>,
    pub selection_hash: Option<String>,
    pub selected: Vec<u32>,
    pub ranking: Vec<RankedPaper>,
    pub actions_reserved: u32,
    pub active_seconds: u64,
    pub active_child: Option<String>,
    pub active_scope: Option<Scope>,
    pub replacements: usize,
    pub repairs: usize,
    pub created_at: i64,
    pub updated_at: i64,
    pub deadline_at: i64,
    pub due_at: Option<i64>,
    pub stop_outcome: Option<String>,
    /// When the shortlist began waiting. The elapsed clock stops until the
    /// researcher chooses, however long that takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_since: Option<i64>,
}
impl Run {
    pub fn next_generation_phase(&self) -> Phase {
        if self.definition.idea_agents > 0 {
            Phase::Brainstorm
        } else {
            Phase::Generate
        }
    }
    pub fn action_cost(&self) -> u32 {
        if self.phase == Phase::Brainstorm {
            super::budget::team_cost(self.definition.idea_agents)
        } else {
            1
        }
    }
    pub fn deadline_passed(&self) -> bool {
        self.selection_since.is_none() && self.deadline_at <= crate::orchestration::store::now()
    }
    pub fn terminal(&self) -> bool {
        matches!(
            self.state.as_str(),
            "completed" | "partial" | "exhausted" | "blocked" | "failed" | "cancelled"
        )
    }
}
#[derive(Debug, Clone)]
pub struct Portfolio {
    pub run: Run,
    pub candidates: Vec<Candidate>,
    pub papers: Vec<Paper>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub revision: i64,
    pub mode: Mode,
    pub prompt: String,
    pub state: String,
    pub reason: String,
    pub phase: Phase,
    pub updated_at: i64,
}
impl From<&Run> for Summary {
    fn from(r: &Run) -> Self {
        Self {
            id: r.id.clone(),
            revision: r.revision,
            mode: r.definition.mode,
            prompt: r.definition.prompt.chars().take(240).collect(),
            state: r.state.clone(),
            reason: r.reason.clone(),
            phase: r.phase.clone(),
            updated_at: r.updated_at,
        }
    }
}

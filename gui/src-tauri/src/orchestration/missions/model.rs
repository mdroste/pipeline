use crate::workbench::desk::ResearchObjectRef;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use super::super::store::Scope;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definition {
    pub schema_version: u32,
    pub name: String,
    pub objective: String,
    pub background: String,
    pub mode: Mode,
    pub criteria: Vec<String>,
    pub budget: Budget,
    pub policy: Policy,
    #[serde(default)]
    pub method_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Theory,
    Empirical,
    Quantitative,
    Literature,
    Discovery,
    Maintenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Budget {
    pub max_rounds: u32,
    pub max_actions: u32,
    pub active_seconds: u32,
    pub action_timeout_seconds: u32,
    pub deadline_hours: u32,
    pub max_stagnant_rounds: u32,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            max_rounds: 8,
            max_actions: 32,
            active_seconds: 21600,
            action_timeout_seconds: 1800,
            deadline_hours: 24,
            max_stagnant_rounds: 2,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
    #[serde(default)]
    pub allow_edits: bool,
    #[serde(default)]
    pub command_network: bool,
    #[serde(default)]
    pub check_profile_ids: Vec<String>,
    #[serde(default)]
    pub experiment_ids: Vec<String>,
    #[serde(default)]
    pub review_profile_id: Option<String>,
    #[serde(default)]
    pub monitor_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mission {
    pub id: String,
    pub revision: i64,
    pub definition: Definition,
    pub state: String,
    pub reason: String,
    pub source_session_id: String,
    pub workspace_id: String,
    pub scope: Scope,
    pub planner_scope: Scope,
    pub challenger_scope: Scope,
    pub authority_fingerprints: BTreeMap<String, String>,
    pub capabilities: Vec<Capability>,
    pub context: String,
    pub goals: Vec<Goal>,
    pub rounds: Vec<Round>,
    pub questions: Vec<Question>,
    pub methods: Vec<Method>,
    pub selected_methods: Vec<Method>,
    pub active_child: Option<String>,
    pub child_ids: Vec<String>,
    pub stop_outcome: Option<String>,
    pub phase: Phase,
    pub actions_reserved: u32,
    pub active_seconds: u64,
    pub stagnant_rounds: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub deadline_at: Option<i64>,
    pub due_at: Option<i64>,
    pub watch_cursor: i64,
    #[serde(default)]
    pub watch_snapshots: BTreeMap<String, Value>,
    #[serde(default)]
    pub watch_epoch: i64,
    pub changes: Vec<Value>,
    pub brief: String,
    pub attention_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub fingerprint: String,
    pub parameters: Value,
    pub execution: Value,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Plan,
    Investigate,
    Review,
    Challenge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoalSpec {
    pub id: String,
    pub question: String,
    pub rationale: String,
    pub resolving_evidence: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    #[serde(flatten)]
    pub spec: GoalSpec,
    pub state: String,
    pub assessment: String,
    pub resolved_round: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Candidate {
    pub id: String,
    pub goal_id: String,
    pub question: String,
    pub uncertainty: String,
    pub rationale: String,
    pub possible_outcomes: Vec<String>,
    pub expected_cost: String,
    pub instruction: String,
    pub kind: InvestigationKind,
    #[serde(default)]
    pub capability_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InvestigationKind {
    Workspace,
    Check,
    Experiment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuestionSpec {
    pub id: String,
    pub question: String,
    pub why_needed: String,
    #[serde(default)]
    pub goal_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    #[serde(flatten)]
    pub spec: QuestionSpec,
    pub answer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub summary: String,
    pub new_goals: Vec<GoalSpec>,
    pub candidates: Vec<Candidate>,
    pub selected_id: Option<String>,
    pub selection_reason: String,
    pub questions: Vec<QuestionSpec>,
    pub disposition: Disposition,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Disposition {
    Investigate,
    AssessCompletion,
    WaitForInput,
    WaitForChange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Finding {
    pub summary: String,
    pub outcome: String,
    pub method: String,
    pub tested_domain: String,
    pub limitations: Vec<String>,
    pub sources: Vec<ResearchObjectRef>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriterionAssessment {
    pub criterion: usize,
    pub status: CriterionStatus,
    pub explanation: String,
    pub evidence_ids: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CriterionStatus {
    Met,
    Unmet,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Challenge {
    pub summary: String,
    pub outcome: Outcome,
    pub goal_resolved: bool,
    pub mission_complete: bool,
    pub progress_made: bool,
    pub tested_domain: String,
    pub limitations: Vec<String>,
    pub unresolved: Vec<String>,
    pub criteria: Vec<CriterionAssessment>,
    pub questions: Vec<QuestionSpec>,
    pub methods: Vec<MethodSpec>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Supported,
    Refuted,
    Inconclusive,
    Infeasible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MethodSpec {
    pub name: String,
    pub when_to_use: String,
    pub procedure: String,
    pub limitations: String,
    pub evidence_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Method {
    pub id: String,
    pub mission_id: String,
    pub round: u32,
    pub workspace_id: String,
    #[serde(flatten)]
    pub spec: MethodSpec,
    pub retained: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Round {
    pub number: u32,
    pub change_cursor: i64,
    pub plan: Plan,
    pub selected: Option<Candidate>,
    pub finding: Option<Finding>,
    pub evidence: BTreeMap<String, Value>,
    pub review: Option<Value>,
    pub challenge: Option<Challenge>,
    pub child_ids: Vec<String>,
    pub result_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub name: String,
    pub state: String,
    pub reason: String,
    pub workspace_id: String,
    pub updated_at: i64,
    pub rounds: usize,
    pub open_questions: usize,
}
impl Mission {
    pub fn summary(&self) -> Summary {
        Summary {
            id: self.id.clone(),
            name: self.definition.name.clone(),
            state: self.state.clone(),
            reason: self.reason.clone(),
            workspace_id: self.workspace_id.clone(),
            updated_at: self.updated_at,
            rounds: self.rounds.len(),
            open_questions: self.questions.iter().filter(|q| q.answer.is_none()).count(),
        }
    }
    pub fn terminal(&self) -> bool {
        matches!(self.state.as_str(), "completed" | "exhausted" | "stopped")
    }
}

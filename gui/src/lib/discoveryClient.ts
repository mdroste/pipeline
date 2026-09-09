import { invoke } from "@tauri-apps/api/core";
import type { Json, TaskEvent } from "./taskClient";

export type DiscoveryMode = "supervised" | "unsupervised";
export const discoveryNames: Record<DiscoveryMode, string> = {
  supervised: "Full self-discovery (supervised)",
  unsupervised: "Full self-discovery (unsupervised)",
};
export interface DiscoveryDefinition {
  schemaVersion: 1;
  mode: DiscoveryMode;
  prompt: string;
  candidateCount: number;
  shortlistCount: number;
  paperCount: number;
  proposalReviewers: number;
  proposalRevisionPasses: number;
  paperReviewers: number;
  revisionPasses: number;
  researchRounds: number;
  challengeResearch: boolean;
  maxReplacements: number;
  maxActions: number;
  activeSeconds: number;
  actionTimeoutSeconds: number;
  deadlineHours: number;
  allowComputation: boolean;
  acquireLiterature: boolean;
  inputPaths: string[];
  reviewProfileId: string | null;
  authorModel: string | null;
  reviewerModel: string | null;
  rankingPriorities: string;
}
export const newDiscovery = (
  mode: DiscoveryMode = "supervised",
): DiscoveryDefinition => ({
  schemaVersion: 1,
  mode,
  prompt: "",
  candidateCount: 75,
  shortlistCount: 10,
  paperCount: 5,
  proposalReviewers: 2,
  proposalRevisionPasses: 1,
  paperReviewers: 2,
  revisionPasses: 2,
  researchRounds: 8,
  challengeResearch: true,
  maxReplacements: 2,
  maxActions: 500,
  activeSeconds: 172800,
  actionTimeoutSeconds: 1800,
  deadlineHours: 168,
  allowComputation: true,
  inputPaths: [],
  acquireLiterature: true,
  reviewProfileId: null,
  authorModel: null,
  reviewerModel: null,
  rankingPriorities:
    "Soundness, substantive contribution, evidence, originality, completeness and clarity; prefer complementary projects.",
});
export interface DiscoverySummary {
  id: string;
  revision: number;
  mode: DiscoveryMode;
  prompt: string;
  state: string;
  reason: string;
  phase: { kind: string };
  updatedAt: number;
}
export interface Candidate {
  id: number;
  proposal: {
    title: string;
    question: string;
    contribution: string;
    method: string;
    firstTest: string;
    requiredEvidence: string[];
    relatedWork: string[];
    risks: string[];
  };
  previousVersions: {
    proposal: Candidate["proposal"];
    assessments: Candidate["assessments"];
  }[];
  assessments: {
    candidateId: number;
    eligible: boolean;
    contribution: number;
    feasibility: number;
    informationValue: number;
    cluster: string;
    duplicateOf: number | null;
    strongestObjection: string;
    resolution: string;
    uncertainty: string;
  }[];
}
export interface PaperAssessment {
  summary: string;
  sound: boolean;
  contribution: string;
  support: string;
  originality: string;
  completeness: string;
  remainingWork: string[];
}
export interface PaperSummary {
  candidateId: number;
  title: string | null;
  state: string;
  reason: string;
  rounds: number;
  versions: number;
  reviewCount: number | null;
  hash: string | null;
  assessment: PaperAssessment | null;
}
export interface DiscoveryRun {
  id: string;
  revision: number;
  definition: DiscoveryDefinition;
  state: string;
  reason: string;
  phase: { kind: string; paper?: number };
  phaseLabel: string;
  candidateCount: number;
  orientation: {
    fields: string[];
    subjectIds: string[];
    methodIds: string[];
    researchStandards: string[];
    assumptions: string[];
    constraints: string[];
  } | null;
  selection: {
    shortlist: number[];
    recommended: number[];
    rationale: string;
  } | null;
  selectionHash: string | null;
  selected: number[];
  ranking: {
    candidateId: number;
    rank: number;
    reason: string;
    uncertainty: string;
  }[];
  actionsReserved: number;
  activeSeconds: number;
  deadlineAt: number;
  activeChild: string | null;
  workspaceId: string;
  sourceSessionId: string;
  papers: PaperSummary[];
}
export interface DiscoveryPaper {
  candidateId: number;
  state: string;
  reason: string;
  rounds: Json[];
  challenges: Json[];
  evidence: Record<string, Json>;
  assessment: PaperAssessment | null;
  versions: {
    version: number;
    hash: string;
    manuscript: {
      title: string;
      abstractText: string;
      markdown: string;
      latex: string;
      bibliography: string;
      evidenceIds: string[];
      limitations: string[];
      responseToReview: string[];
      files: string[];
    };
    artifacts: Record<string, Json>;
    reviews: {
      summary: string;
      findings: {
        claim: string;
        severity: string;
        evidence: string;
        resolution: string;
      }[];
      strengths: string[];
      limitations: string[];
    }[];
    externalReview: Json;
  }[];
}
export const discoveryClient = {
  start: (
    definition: DiscoveryDefinition,
    sessionId: string | null,
    operationId: string,
  ) =>
    invoke<DiscoveryRun>("discovery_start", {
      request: { definition, sessionId, operationId },
    }),
  list: (offset = 0, workspaceId: string | null = null) =>
    invoke<DiscoverySummary[]>("discovery_list", { offset, workspaceId }),
  get: (id: string) => invoke<DiscoveryRun>("discovery_get", { id }),
  candidates: (id: string, offset = 0) =>
    invoke<Candidate[]>("discovery_candidates", { id, offset }),
  paper: (id: string, candidateId: number) =>
    invoke<DiscoveryPaper>("discovery_paper", { id, candidateId }),
  select: (
    id: string,
    selectionHash: string,
    candidateIds: number[],
    operationId: string,
  ) =>
    invoke<DiscoveryRun>("discovery_select", {
      id,
      selectionHash,
      candidateIds,
      operationId,
    }),
  control: (run: DiscoveryRun, action: "pause" | "resume" | "stop") =>
    invoke<DiscoveryRun>("discovery_control", {
      id: run.id,
      revision: run.revision,
      action,
    }),
  export: (id: string, format: "markdown" | "zip") =>
    invoke<string | null>("discovery_export", { id, format }),
  events: (id: string, after = 0) =>
    invoke<TaskEvent[]>("discovery_events", { id, after }),
};
export function discoveryError(d: DiscoveryDefinition): string | null {
  if (!d.prompt.trim()) return "Describe the topics or questions to research.";
  if (
    ![d.candidateCount, d.shortlistCount, d.paperCount].every(Number.isInteger)
  )
    return "Project counts must be whole numbers.";
  if (d.candidateCount < 50 || d.candidateCount > 100)
    return "Explore between 50 and 100 candidates.";
  if (d.shortlistCount < 1 || d.shortlistCount > 25)
    return "Shortlist between 1 and 25 projects.";
  if (d.paperCount < 1 || d.paperCount > 9 || d.paperCount > d.shortlistCount)
    return "Choose 1–9 papers, no more than the shortlist size.";
  return null;
}
export const discoveryTerminal = (state: string) =>
  [
    "completed",
    "partial",
    "exhausted",
    "blocked",
    "failed",
    "cancelled",
  ].includes(state);

import { invoke } from "@tauri-apps/api/core";
import type { ContextItem, DeskRecord, OpenResearchObject } from "./deskClient";
import type { ExecutionProfile, ResearchExecution } from "./workbenchTypes";
import type { ProjectRecord } from "./projectClient";
import type { ResponseRecord, TheoryNote } from "./studioClient";
export interface ProgramAction {
  action: string;
  [field: string]: unknown;
}
export interface Artifact {
  id: string;
  hash: string;
  extension: string;
  sizeBytes: number;
}
export interface ObjectChoice {
  reference: OpenResearchObject;
  title: string;
  provenance: string;
}
export interface Choices {
  objects: ObjectChoice[];
  plans: DeskRecord<CapturedPlan>[];
  profiles: ExecutionProfile[];
  executions: ResearchExecution[];
  checkpoints: ProjectRecord<{ taskId: string; state: string }>[];
  responses: ProjectRecord<ResponseRecord>[];
  theory: ProjectRecord<TheoryNote>[];
}
export interface CapturedPlan {
  profile: ExecutionProfile;
  capturedFiles: { path: string; hash: string; artifactId: string }[];
  parameters: Record<string, unknown>;
  randomSeed: string | null;
  toolchainVersion: string;
}
export interface Factor {
  name: string;
  values: (string | number | boolean)[];
}
export interface Exclusion {
  matches: Record<string, string | number | boolean>;
  reason: string;
}
export interface GridPlan {
  question: string;
  specifications: {
    ordinal: number;
    factors: Record<string, unknown>;
    planId: string | null;
    excludedReason: string | null;
  }[];
  totalRuns: number;
  timeoutBudgetSeconds: number;
  capturedInputBytes: number;
  interpretation: string;
  selectedOutputs: string[];
}
export interface GridStatus {
  basePlan: DeskRecord<CapturedPlan>;
  record: DeskRecord<GridPlan>;
  plan: GridPlan;
  run: {
    state: string;
    revision: number;
    nextIndex: number;
    activeExecutionId: string | null;
    reason: string | null;
  };
  attempts: {
    ordinal: number;
    executionId: string | null;
    state: string;
    receipt: Partial<ResearchExecution>;
  }[];
}
export interface Entry {
  label: string;
  source: OpenResearchObject;
  manual: { value: number; reason: string } | null;
}
export interface AssetSpec {
  kind: "table" | "coefficient" | "irf";
  title: string;
  entries: Entry[];
  digits: number;
  uncertainty: "none" | "se" | "ci";
  notes: string;
  sampleComparisonRationale: string | null;
}
export interface Publication {
  spec: AssetSpec;
  entries: {
    label: string;
    source: OpenResearchObject;
    value: Record<string, unknown>;
    manual: boolean;
  }[];
  artifacts: Artifact[];
  sources: OpenResearchObject[];
  executionId: string | null;
}
export interface SymbolRecord {
  notation: string;
  scope: string;
  definition: string;
  domain: string;
  units: string;
  aliases: string[];
  sources: OpenResearchObject[];
}
export interface Section {
  heading: string;
  text: string;
  sources: OpenResearchObject[];
  assets: string[];
}
export interface Outline {
  title: string;
  template: string;
  sections: Section[];
}
export interface Deliverable {
  outline: Outline;
  artifacts: Artifact[];
  attachments: { path: string; artifact: Artifact }[];
  sources: OpenResearchObject[];
}
export interface Kit {
  id: string;
  version: number;
  title: string;
  instructions: string;
  sections: string[];
  tasks: string[];
  recipeSuggestions: string[];
}
export interface Campaign {
  round: number;
  manuscript: OpenResearchObject;
  comments: {
    response: OpenResearchObject;
    requiredOutputs: OpenResearchObject[];
    researcherAddressed: boolean;
    judgment: string;
  }[];
  changeSummary: string;
  reviewScope: string;
  broadChange: boolean;
  previousRound: string | null;
}
export interface CampaignStatus {
  record: DeskRecord<Campaign>;
  comments: {
    response: OpenResearchObject;
    number: string;
    draft: string;
    disposition: string;
    taskDone: boolean;
    fileAcceptedAndCurrent: boolean;
    checkExecuted: boolean;
    linkChecksPass: boolean;
    researcherAddressed: boolean;
    judgment: string;
    flags: string[];
    requiredOutputs: { source: OpenResearchObject; state: string }[];
  }[];
  reviewRecommendation: string;
  notice: string;
}
export interface Monitor {
  kind: string;
  target: string;
  intervalSeconds: number;
  notify: boolean;
  networkConsent: boolean;
}
export interface MonitorState {
  checks: {
    id: string;
    title: string;
    spec: Monitor;
    enabled: boolean;
    revision: number;
    nextDueAt: number;
    lastCheckedAt: number | null;
    lastOutcome: unknown;
  }[];
  attention: {
    id: string;
    body: { title: string; outcome: unknown };
    createdAt: string;
    acknowledgedAt: string | null;
  }[];
  lifecycle: string;
}
export interface Followup {
  id: string;
  position: number;
  revision: number;
  request: {
    text: string;
    model: string | null;
    effort: string | null;
    binding: { runtimeRoot: string };
    context: { items: ContextItem[] };
    turnBudget: number;
  };
  state: string;
  fingerprint: string;
  result: unknown;
}
export interface FollowupContextReview {
  id: string;
  revision: number;
  request: Followup["request"];
  previousContext: { items: ContextItem[] };
  previousRoot: string;
  conversationTitle: string;
  workspaceName: string | null;
  conversationChanged: boolean;
  settingsChanged: boolean;
  settings: {
    preset: string;
    permissions: string;
    commandNetwork: boolean;
    modules: string[];
    instructions: string;
  };
  fingerprint: string;
}
export interface Expected {
  output: string;
  pointer: string;
  value: number;
  absoluteTolerance: number;
  relativeTolerance: number;
}
export interface CapsuleSelection {
  planId: string;
  includedPaths: string[];
  externalRequirements: Record<string, string>;
  expected: Expected[];
}
export interface CapsuleRequest {
  workspaceId: string;
  title: string;
  selections: CapsuleSelection[];
  path: string;
}
export interface CapsuleManifest {
  title: string;
  plans: {
    label: string;
    environment: {
      variables: Record<string, string>;
      externalVariables: string[];
      platform: string;
      lockfiles: string[];
      coverage: string;
    };
    inputs: {
      path: string;
      hash: string;
      included: boolean;
      externalRequirement: string | null;
    }[];
    outputs: string[];
    expected: Expected[];
  }[];
  limitations: string;
}
export interface CapsulePreview {
  manifest: CapsuleManifest;
  fingerprint: string;
  includedBytes: number;
}
export const programClient = {
  call: <T = unknown>(workspaceId: string, action: ProgramAction) =>
    invoke<T>("workbench_program", { workspaceId, action }),
  queue: <T = Followup[]>(sessionId: string, action: ProgramAction) =>
    invoke<T>("workbench_followups", { sessionId, action }),
};

import type { WorkbenchEvent } from "./workbench/runtimeTypes";
import { invoke } from "@tauri-apps/api/core";
import type { ContextItem, DeskRecord, OpenResearchObject } from "./deskClient";
import type {
  ExecutionProfile,
  ResearchExecution,
} from "./workbench/executionTypes";
import type { ProjectRecord } from "./projectClient";
import type { ResponseRecord, TheoryNote } from "./studioClient";
export interface Artifact {
  id: string;
  hash: string;
  extension: string;
  sizeBytes: number;
}
export interface ObjectChoice {
  reference: OpenResearchObject;
  title: string;
  /** What to show a researcher: a passage's quote, a series' variable. */
  label: string;
  /** The kind of a project record; null for any other object. */
  recordKind: string | null;
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
export interface ResearchActivity {
  turns: {
    sessionId: string;
    workspaceId: string | null;
    title: string;
    threadId: string;
    turnId: string | null;
    state: string;
  }[];
  jobs: {
    id: string;
    workspaceId: string;
    title: string;
    adapter: string;
    state: string;
    owner: string | null;
  }[];
  pendingRequests: WorkbenchEvent[];
  researchAttention: number;
}

type ArtifactAction =
  | { action: "artifact"; id: string }
  | { action: "exportArtifact"; id: string; path: string }
  | {
      action: "stage";
      recordId: string;
      artifactId: string;
      checkpointId: string;
      path: string;
      expectedHash: string | null;
    };

type FollowupAction =
  | { action: "list"; historyOffset: number }
  | {
      action: "enqueue";
      text: string;
      model: string | null;
      effort: string | null;
      operationId: string;
    }
  | { action: "reviewContext"; id: string; revision: number }
  | { action: "run"; id: string; fingerprint: string }
  | {
      action: "control";
      id: string;
      revision: number;
      control: "up" | "down" | "cancel" | "dismiss";
    }
  | { action: "reconcile"; id: string }
  | {
      action: "refreshContext";
      id: string;
      revision: number;
      fingerprint: string;
    }
  | {
      action: "branch";
      title: string;
      items: ContextItem[];
      operationId: string;
    };

interface ArtifactResults {
  artifact: { base64: string };
  exportArtifact: unknown;
  stage: unknown;
}

interface FollowupResults {
  list: Followup[];
  enqueue: unknown;
  reviewContext: FollowupContextReview;
  run: unknown;
  control: unknown;
  reconcile: unknown;
  refreshContext: unknown;
  branch: { session: { id: string } };
}

export const programClient = {
  activity: () => invoke<ResearchActivity>("workbench_research_activity"),
  call: <T extends ArtifactAction>(workspaceId: string, action: T) =>
    invoke<ArtifactResults[T["action"]]>("workbench_program", {
      workspaceId,
      action,
    }),
  queue: <T extends FollowupAction>(sessionId: string, action: T) =>
    invoke<FollowupResults[T["action"]]>("workbench_followups", {
      sessionId,
      action,
    }),
};

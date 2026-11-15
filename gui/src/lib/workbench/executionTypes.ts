import type { ExecutionAdapter } from "./toolchains";

/** App-owned Workspace records. */

export interface ExecutionProfile {
  id: string;
  workspaceId: string;
  name: string;
  adapter: ExecutionAdapter;
  argv: string[];
  cwd: string;
  environment: Record<string, string>;
  inputs: string[];
  timeoutSeconds: number;
  outputs: string[];
  testedAt: string | null;
  testStatus: "passed" | "failed" | null;
  revision: number;
}

export interface ResearchExecution {
  id: string;
  workspaceId: string;
  sessionId: string | null;
  profileId: string | null;
  adapter: string;
  command: string[];
  cwd: string;
  inputManifest: Record<string, unknown>;
  dependencyHash: string;
  outcome:
    | "queued"
    | "running"
    | "completed"
    | "failed"
    | "interrupted"
    | "timed_out"
    | "outcome_unknown";
  startedAt: string | null;
  endedAt: string | null;
  exitStatus: number | null;
  stdout: string | null;
  stderr: string | null;
  outputManifest: Record<string, unknown>;
  validation: Record<string, unknown>;
  snapshotConsistency: "complete" | "partial" | "uncertain";
  createdAt: string;
}

export interface ResearchResultV1 {
  resultId: string;
  estimand: string;
  specificationId: string;
  sampleId: string;
  estimate: number;
  standardError: number | null;
  confidenceInterval: [number, number] | null;
  n: number | null;
  units: string;
  transformation: string | null;
  uncertaintyMethod: string | null;
  sourceExecutionId: string;
  artifactLocator: string;
}

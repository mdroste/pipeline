export type Json =
  null | boolean | number | string | Json[] | { [key: string]: Json };
export type Binding =
  | { kind: "literal"; value: Json }
  | { kind: "output"; step: string; pointer?: string }
  | { kind: "input"; key: string }
  | { kind: "firstAvailable"; values: Binding[] };
export type Condition =
  | { op: "equals"; left: Binding; right: Json }
  | { op: "lessThan"; left: Binding; right: number }
  | { op: "exists"; value: Binding }
  | { op: "all" | "any"; conditions: Condition[] }
  | { op: "not"; condition: Condition };
export type Step = { id: string; label: string } & (
  | { kind: "workspace"; prompt: string; model?: string; effort?: string }
  | {
      kind: "snapshot";
      input: Binding;
      filename: string;
      requireChange?: boolean;
    }
  | {
      kind: "review";
      input: Binding;
      profileId: string;
      interpretation?: string;
      variables?: Record<string, string>;
    }
  | { kind: "check"; profileId: string }
  | { kind: "capturedCheck"; planId: string }
  | { kind: "literatureLookup"; query: string }
  | { kind: "deliver"; input: Binding }
  | { kind: "delay"; seconds: number }
  | { kind: "until"; at: number }
  | { kind: "input"; prompt: string; timeoutSeconds?: number }
  | { kind: "if"; condition: Condition; thenSteps: Step[]; elseSteps?: Step[] }
  | { kind: "repeat"; maxIterations: number; until: Condition; steps: Step[] }
  | {
      kind: "while";
      maxIterations: number;
      condition: Condition;
      steps: Step[];
    }
  | { kind: "parallel"; branches: Step[][] }
  | { kind: "forEach"; input: Binding; maxItems: number; steps: Step[] }
  | { kind: "chain"; chain: Chain }
);
export interface Chain {
  schemaVersion: 1;
  name: string;
  description: string;
  steps: Step[];
  limits: {
    maxActions: number;
    deadlineHours: number;
    actionTimeoutSecs: number;
  };
}
export type Trigger =
  | { kind: "now" }
  | { kind: "once"; at: number }
  | { kind: "interval"; seconds: number; anchor: number }
  | {
      kind: "calendar";
      timezone: string;
      hour: number;
      minute: number;
      weekdays: number[];
    };
export interface TaskSummary {
  id: string;
  revision: number;
  name: string;
  state: string;
  reason: string | null;
  createdAt: number;
  updatedAt: number;
  dueAt: number | null;
  sessionId: string | null;
  scheduleId: string | null;
}
export interface Receipt {
  sequence?: number;
  address: string;
  stepId: string;
  label: string;
  state: string;
  operation: string;
  startedAt: number;
  finishedAt: number | null;
  wakeAt: number | null;
  output: Json;
  error: string | null;
  child?: Json;
}
export interface Scope {
  sessionId: string | null;
  workspaceId: string | null;
  runtimeRoot: string | null;
  profiles: Record<string, { profileName: string }>;
  checks: Record<string, string>;
}
export interface TaskRun extends TaskSummary {
  missionId?: string;
  chain: Chain;
  scope: Scope;
  inputs: Record<string, Json>;
  progress: {
    receipts: Record<string, Receipt>;
    outputs: Record<string, Json>;
    actions: number;
    limitReached: boolean;
  };
  deadlineAt: number;
}
export interface Schedule {
  id: string;
  revision: number;
  name: string;
  enabled: boolean;
  chain: Chain;
  scope: Scope;
  trigger: Trigger;
  nextDueAt: number | null;
}
export interface TaskEvent {
  sequence: number;
  at: number;
  kind: string;
  detail: string;
  attempts?: Receipt[];
}
export interface SessionChoice {
  id: string;
  title: string;
  workspaceName: string | null;
}
export interface TaskProposal {
  id: string;
  chain: { name: string };
}

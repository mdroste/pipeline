import { invoke } from "@tauri-apps/api/core";
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type Binding = { kind: "literal"; value: Json } | { kind: "output"; step: string; pointer?: string } | { kind: "input"; key: string } | { kind: "firstAvailable"; values: Binding[] };
export type Condition = { op: "equals"; left: Binding; right: Json } | { op: "lessThan"; left: Binding; right: number } | { op: "exists"; value: Binding } | { op: "all" | "any"; conditions: Condition[] } | { op: "not"; condition: Condition };
export type Step = { id: string; label: string } & (
  { kind: "workspace"; prompt: string; model?: string; effort?: string } |
  { kind: "snapshot"; input: Binding; filename: string; requireChange?: boolean } |
  { kind: "review"; input: Binding; profileId: string; interpretation?: string; variables?: Record<string, string> } |
  { kind: "check"; profileId: string } | { kind: "capturedCheck"; planId: string } | { kind: "deliver"; input: Binding } |
  { kind: "delay"; seconds: number } | { kind: "until"; at: number } |
  { kind: "input"; prompt: string; timeoutSeconds?: number } |
  { kind: "if"; condition: Condition; thenSteps: Step[]; elseSteps?: Step[] } |
  { kind: "repeat"; maxIterations: number; until: Condition; steps: Step[] } |
  { kind: "while"; maxIterations: number; condition: Condition; steps: Step[] } |
  { kind: "parallel"; branches: Step[][] } |
  { kind: "forEach"; input: Binding; maxItems: number; steps: Step[] } |
  { kind: "chain"; chain: Chain });
export interface Chain { schemaVersion: 1; name: string; description: string; steps: Step[]; limits: { maxActions: number; deadlineHours: number; actionTimeoutSecs: number } }
export type Trigger = { kind: "now" } | { kind: "once"; at: number } | { kind: "interval"; seconds: number; anchor: number } | { kind: "calendar"; timezone: string; hour: number; minute: number; weekdays: number[] };
export interface TaskSummary { id: string; revision: number; name: string; state: string; reason: string | null; createdAt: number; updatedAt: number; dueAt: number | null; sessionId: string | null; scheduleId: string | null }
export interface Receipt { sequence?: number; address: string; stepId: string; label: string; state: string; operation: string; startedAt: number; finishedAt: number | null; wakeAt: number | null; output: Json; error: string | null; child?: Json }
export interface Scope { sessionId: string | null; workspaceId: string | null; runtimeRoot: string | null; profiles: Record<string, { profileName: string }>; checks: Record<string, string> }
export interface TaskRun extends TaskSummary { missionId?: string; chain: Chain; scope: Scope; inputs: Record<string, Json>; progress: { receipts: Record<string, Receipt>; outputs: Record<string, Json>; actions: number; limitReached: boolean }; deadlineAt: number }
export interface Schedule { id: string; revision: number; name: string; enabled: boolean; chain: Chain; scope: Scope; trigger: Trigger; nextDueAt: number | null }
export interface TaskEvent { sequence: number; at: number; kind: string; detail: string; attempts?: Receipt[] }
export interface SessionChoice { id: string; title: string; workspaceName: string | null }
export const taskClient = {
  list: (view = "active", sessionId: string | null = null, offset = 0) => invoke<TaskSummary[]>("task_list", { view, sessionId, offset }),
  get: (id: string) => invoke<TaskRun>("task_get", { id }),
  events: (id: string, after = 0) => invoke<TaskEvent[]>("task_events", { id, after }),
  prepare: (chain: Chain, sessionId: string | null, inputs: Record<string, Json>, trigger: Trigger, operationId: string) => invoke<TaskRun>("task_prepare", { request: { chain, sessionId, inputs, trigger, operationId } }),
  control: (run: TaskRun, action: string) => invoke<TaskRun>("task_control", { id: run.id, revision: run.revision, action }),
  input: (id: string, address: string, value: Json, operationId: string) => invoke<TaskRun>("task_input", { id, address, value, operationId }),
  schedules: () => invoke<Schedule[]>("task_schedules"),
  updateSchedule: (schedule: Schedule, enabled: boolean, trigger: Trigger | null = null) => invoke<Schedule>("task_update_schedule", { id: schedule.id, revision: schedule.revision, enabled, trigger }),
  times: (trigger: Trigger) => invoke<number[]>("task_preview_times", { trigger, after: Math.floor(Date.now() / 1000) - 1 }),
  sessions: () => invoke<SessionChoice[]>("task_sessions"),
  chains: () => invoke<{ id: string; chain: Chain }[]>("task_saved_chains"),
  saveChain: (chain: Chain) => invoke<string>("task_save_chain", { chain }),
  validate: (json: string) => invoke<Chain>("task_validate_chain", { json }),
  background: (enabled: boolean | null = null) => invoke<boolean>("task_background", { enabled }),
  artifact: (id: string, address: string, operation: string | null = null) => invoke<string | null>("task_export_artifact", { id, address, operation }),
  exportChain: (chain: Chain) => invoke<string | null>("task_export_chain", { chain }),
};
export const taskTime = (value: number | null) => value === null ? "Waiting for input" : new Date(value * 1000).toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
export function stateLabel(state: string) { return ({ attention: "Needs attention", draft: "Ready to start", cancelling: "Stopping", finished: "Finished", cancelled: "Stopped" } as Record<string, string>)[state] ?? state.charAt(0).toUpperCase() + state.slice(1); }
export const output = (step: string, pointer = ""): Binding => ({ kind: "output", step, pointer });
export function template(kind: "review" | "prompt" | "input", prompt: string, profileId: string, rounds = 3, path = ""): Chain {
  const limits = { maxActions: 64, deadlineHours: 168, actionTimeoutSecs: 7200 };
  if (kind === "prompt") return { schemaVersion: 1, name: "Workspace follow-up", description: "Continue a conversation at the right time.", limits, steps: [{ id: "followup", label: "Continue in Workspace", kind: "workspace", prompt }] };
  if (kind === "input") return { schemaVersion: 1, name: "Wait and continue", description: "Resume this conversation when you provide the missing input.", limits, steps: [{ id: "answer", label: "Wait for input", kind: "input", prompt: "Provide the input needed to continue." }, { id: "followup", label: "Continue in Workspace", kind: "workspace", prompt: `${prompt}\n\nNew input:\n{{output:answer}}` }] };
  const steps: Step[] = path ? [{ id: "initialPaper", label: "Capture the selected paper", kind: "snapshot", input: { kind: "literal", value: { path } }, filename: path.split(/[\\/]/).pop() || "paper.md" }] : [{ id: "draft", label: "Draft the paper", kind: "workspace", prompt: `${prompt}\n\nReturn the complete paper as Markdown, with no enclosing code fence. This text will be saved as paper.md and reviewed. Keep uncertain claims explicit.` }, { id: "initialPaper", label: "Save the paper", kind: "snapshot", input: output("draft", "/text"), filename: "paper.md" }];
  steps.push({ id: "reviewCycle", label: "Review and revise", kind: "repeat", maxIterations: rounds, until: { op: "all", conditions: [{ op: "equals", left: output("review", "/complete"), right: true }, { op: "lessThan", left: output("review", "/highPriorityCount"), right: 1 }, { op: "lessThan", left: output("review", "/unknownPriorityCount"), right: 1 }] }, steps: [
    { id: "needsRevision", label: "Revise after an unsuccessful review", kind: "if", condition: { op: "exists", value: output("review") }, thenSteps: [
      { id: "revise", label: "Address the review", kind: "workspace", prompt: "Revise the paper below to address the review. Preserve the argument and flag claims you cannot verify. Return the complete revised paper as Markdown, without a surrounding code fence.\n\nPaper:\n{{output:review#/paperText}}\n\nReview:\n{{output:review#/report}}\n\nReview quality and limitations:\n{{output:review#/quality}}" },
      { id: "revisedPaper", label: "Capture the revised paper", kind: "snapshot", input: output("revise", "/text"), filename: "paper.md", requireChange: true },
    ] },
    { id: "review", label: "Run the Review profile", kind: "review", profileId, interpretation: "document", input: { kind: "firstAvailable", values: [output("revisedPaper"), output("initialPaper")] } },
  ] });
  steps.push({ id: "result", label: "Return the reviewed paper and findings", kind: "deliver", input: output("review") });
  return { schemaVersion: 1, name: "Review and revise", description: "Stop when a complete review has no high-priority or unclassified findings, or the review limit is reached. The final paper is always the reviewed revision.", limits, steps };
}

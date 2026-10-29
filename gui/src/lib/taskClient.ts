import { invoke } from "@tauri-apps/api/core";
import type {
  TaskSummary,
  TaskRun,
  TaskEvent,
  Chain,
  Json,
  Trigger,
  Schedule,
  SessionChoice,
  TaskProposal,
} from "./taskTypes";

export const taskClient = {
  list: (view = "active", sessionId: string | null = null, offset = 0) =>
    invoke<TaskSummary[]>("task_list", { view, sessionId, offset }),
  get: (id: string) => invoke<TaskRun>("task_get", { id }),
  events: (id: string, after = 0) =>
    invoke<TaskEvent[]>("task_events", { id, after }),
  prepare: (
    chain: Chain,
    sessionId: string | null,
    inputs: Record<string, Json>,
    trigger: Trigger,
    operationId: string,
  ) =>
    invoke<TaskRun>("task_prepare", {
      request: { chain, sessionId, inputs, trigger, operationId },
    }),
  control: (run: TaskRun, action: string) =>
    invoke<TaskRun>("task_control", {
      id: run.id,
      revision: run.revision,
      action,
    }),
  input: (id: string, address: string, value: Json, operationId: string) =>
    invoke<TaskRun>("task_input", { id, address, value, operationId }),
  schedules: () => invoke<Schedule[]>("task_schedules"),
  updateSchedule: (
    schedule: Schedule,
    enabled: boolean,
    trigger: Trigger | null = null,
  ) =>
    invoke<Schedule>("task_update_schedule", {
      id: schedule.id,
      revision: schedule.revision,
      enabled,
      trigger,
    }),
  times: (trigger: Trigger) =>
    invoke<number[]>("task_preview_times", {
      trigger,
      after: Math.floor(Date.now() / 1000) - 1,
    }),
  sessions: () => invoke<SessionChoice[]>("task_sessions"),
  proposals: (sessionId: string) =>
    invoke<TaskProposal[]>("task_proposals", { sessionId }),
  prepareProposal: (sessionId: string, proposalId: string) =>
    invoke<{ id: string }>("task_prepare_proposal", { sessionId, proposalId }),
  chains: () => invoke<{ id: string; chain: Chain }[]>("task_saved_chains"),
  saveChain: (chain: Chain) => invoke<string>("task_save_chain", { chain }),
  validate: (json: string) => invoke<Chain>("task_validate_chain", { json }),
  background: (enabled: boolean | null = null) =>
    invoke<boolean>("task_background", { enabled }),
  artifact: (id: string, address: string, operation: string | null = null) =>
    invoke<string | null>("task_export_artifact", { id, address, operation }),
  stepOutput: (id: string, address: string, operation: string) =>
    invoke<Json>("task_step_output", { id, address, operation }),
  exportChain: (chain: Chain) =>
    invoke<string | null>("task_export_chain", { chain }),
};

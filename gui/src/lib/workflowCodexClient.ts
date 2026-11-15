import { invoke } from "@tauri-apps/api/core";

export interface WorkflowCodexStatus {
  epoch: number;
  unresolvedAttempts?: Array<{ id: string; label: string; state: string }>;
}

type Attempt = { id: string; epoch: number };

/** Review recovery commands use the Review runtime's attempt records. */
export const workflowCodexClient = {
  status: () => invoke<WorkflowCodexStatus>("workflow_codex_status"),
  reconcile: (args: Attempt) =>
    invoke<{ status: string; preview: string; recordPath: string }>(
      "workflow_codex_reconcile_attempt",
      args,
    ),
  acknowledge: (args: Attempt) =>
    invoke<void>("workflow_codex_acknowledge_attempt", args),
};

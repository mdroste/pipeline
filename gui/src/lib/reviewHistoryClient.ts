import { invoke } from "@tauri-apps/api/core";
import type {
  RunSummary,
  RunsDiskUsage,
  TrashedRun,
  PipelineReport,
} from "./reviewTypes";
export interface PurgePreview {
  delete_count: number;
  delete_bytes: number;
  remaining_count: number;
  remaining_bytes: number;
  preview_token: string;
}

export const reviewHistoryClient = {
  list: () => invoke<RunSummary[]>("list_runs"),
  usage: () => invoke<RunsDiskUsage>("runs_disk_usage"),
  annotations: (args: { runId: string }) =>
    invoke<string>("get_annotations", args),
  saveAnnotations: (args: { runId: string; content: string }) =>
    invoke<void>("save_annotations", args),
  trashed: () => invoke<TrashedRun[]>("list_trashed_runs"),
  updateMeta: (args: { runId: string; title: string; tags: string[] }) =>
    invoke<unknown>("update_run_meta", args),
  trash: (args: { runId: string }) => invoke<unknown>("delete_run", args),
  restore: (args: { runId: string }) =>
    invoke<unknown>("restore_trashed_run", args),
  remove: (args: { runId: string }) =>
    invoke<unknown>("permanently_delete_trashed_run", args),
  report: (args: { runId: string }) =>
    invoke<PipelineReport>("get_run_report", args),
  reconcile: (args: { runA: string; runB: string }) =>
    invoke<string>("reconcile_runs", args),
  previewPurge: (args: { keep: number; maxBytes: number }) =>
    invoke<PurgePreview>("preview_purge_runs", args),
  purge: (args: { keep: number; maxBytes: number; previewToken: string }) =>
    invoke<number>("purge_runs", args),
};

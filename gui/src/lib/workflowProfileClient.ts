import { invoke } from "@tauri-apps/api/core";
import type {
  PipelineConfig,
  ProfileSummary,
  ExportEnvelope,
} from "./workflowTypes";

export interface CalibrationDraft {
  addendum: string;
  target_step_id: string;
  target_label: string;
  rejected_count: number;
}

// Keep the standard review workflows first in every workflow picker.
const profileOrder = new Map([
  ["auto-paper-review", 0],
  ["auto-paper-review-fast", 1],
  ["slide-review", 2],
]);

export const workflowProfileClient = {
  calibration: () => invoke<CalibrationDraft>("draft_calibration"),
  config: () => invoke<PipelineConfig>("get_pipeline_config"),
  profiles: async () => {
    const profiles = await invoke<ProfileSummary[]>("list_profiles");
    return [...profiles].sort(
      (a, b) =>
        (profileOrder.get(a.id) ?? profileOrder.size) -
        (profileOrder.get(b.id) ?? profileOrder.size),
    );
  },
  active: () => invoke<string>("get_active_profile"),
  switch: (args: { id: string }) =>
    invoke<PipelineConfig>("switch_profile", args),
  create: (args: { name: string }) =>
    invoke<ProfileSummary>("create_profile", args),
  duplicate: (args: { sourceId: string; newName: string }) =>
    invoke<ProfileSummary>("duplicate_profile", args),
  rename: (args: { id: string; newName: string }) =>
    invoke<ProfileSummary>("rename_profile", args),
  remove: (args: { id: string }) => invoke<unknown>("delete_profile", args),
  save: (args: { config: PipelineConfig; profileId: string }) =>
    invoke<unknown>("save_pipeline_config", args),
  reset: () => invoke<PipelineConfig>("reset_pipeline_config"),
  exportItem: (args: { json: string; suggestedName: string }) =>
    invoke<unknown>("export_item", args),
  exportProfile: (args: { id: string; suggestedName: string }) =>
    invoke<unknown>("export_profile", args),
  exportBundle: (args: { suggestedName: string }) =>
    invoke<unknown>("export_bundle", args),
  importItem: (args: { path: string }) =>
    invoke<ExportEnvelope>("import_item", args),
  importProfile: (args: { path: string }) =>
    invoke<ProfileSummary>("import_profile", args),
  importBundle: (args: { path: string }) =>
    invoke<unknown>("import_bundle", args),
};

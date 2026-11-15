import { invoke } from "@tauri-apps/api/core";
import type { AutoReviewCatalog } from "./reviewCatalogTypes";
import type { StepConfig } from "./workflowTypes";
export interface ProviderSchemaPreview {
  projected: Record<string, unknown>;
  bytes: number;
  limit: number;
  openaiStrict: boolean;
}

type OrientationDefaults = { prompt: string; schema: Record<string, unknown> };

export const workflowSchemaClient = {
  orientationDefaults: (args: { name: string }) =>
    invoke<OrientationDefaults>("get_orientation_defaults", args),
  autoOrientationDefaults: (args: { profileId?: string }) =>
    invoke<OrientationDefaults>("get_auto_review_orientation_defaults", args),
  defaultPrompt: (args: { name: string }) =>
    invoke<string>("get_default_prompt", args),
  parallelTemplate: () => invoke<string>("get_default_parallel_template"),
  specialistStep: (args: { id: string }) =>
    invoke<StepConfig>("get_auto_review_specialist_step", args),
  findings: () => invoke<Record<string, unknown>>("get_findings_output_schema"),
  catalog: () => invoke<AutoReviewCatalog>("get_auto_review_catalog"),
  preview: (args: { schema: Record<string, unknown> }) =>
    invoke<ProviderSchemaPreview>("preview_provider_schema", args),
  resolve: (args: { schema: Record<string, unknown> }) =>
    invoke<Record<string, unknown>>(
      "resolve_orientation_schema_catalogs",
      args,
    ),
};

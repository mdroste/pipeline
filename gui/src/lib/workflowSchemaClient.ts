import { invoke } from "@tauri-apps/api/core";
import type { AutoReviewCatalog } from "./types";
export interface ProviderSchemaPreview {
  projected: Record<string, unknown>;
  bytes: number;
  limit: number;
  openaiStrict: boolean;
}

export const workflowSchemaClient = {
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

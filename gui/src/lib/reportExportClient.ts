import { invoke } from "@tauri-apps/api/core";
import type { PipelineReport } from "./types";
export type ExportMode = "shareable" | "forensic" | "custom";

export interface ExportSelection {
  report: boolean;
  verifiedFindings: boolean;
  provenance: boolean;
  workflow: boolean;
  sourceDocuments: boolean;
  rawResponses: boolean;
  logs: boolean;
  supportingArtifacts: boolean;
}

export interface ExportPackageResult {
  exportedPath: string;
  fileCount: number;
  bytes: number;
  checksum: string;
  mode: ExportMode;
  sensitivity: "shareable" | "sensitive";
}

export const reportExportClient = {
  saveMarkdown: (args: { markdown: string; suggestedName: string }) =>
    invoke<unknown>("save_report_md", args),
  print: (args: { markdown: string; provenanceMarkdown: string | null }) =>
    invoke<unknown>("print_report_html", args),
  saveCore: (args: {
    dir: string;
    markdown: string;
    extractedText: string;
    report: PipelineReport;
  }) => invoke<unknown>("save_all_artifacts", args),
  package: (args: {
    runId: string;
    destination: string;
    mode: ExportMode;
    selection: ExportSelection;
  }) => invoke<ExportPackageResult>("export_run_package", args),
  reveal: (args: { path: string }) =>
    invoke<unknown>("reveal_export_in_folder", args),
  saveText: (args: { content: string; suggestedName: string }) =>
    invoke<unknown>("save_text_file", args),
};

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

/** Shareable report formats built from a saved run. */
export type ReportFormat = "html" | "docx" | "docxComments" | "annotatedPdf";

export interface ReportFormatExport {
  path: string;
  findings: number;
  /** Findings placed on a quoted passage (comments or highlights). */
  anchored: number;
  /** Findings listed in a summary comment or page note instead. */
  unanchored: number;
}

const REPORT_FORMAT_COMMANDS: Record<ReportFormat, string> = {
  html: "save_report_html",
  docx: "save_report_docx",
  docxComments: "save_report_docx_comments",
  annotatedPdf: "save_report_annotated_pdf",
};

export const reportExportClient = {
  saveMarkdown: (args: { markdown: string; suggestedName: string }) =>
    invoke<unknown>("save_report_md", args),
  /** Resolves to null when the save dialog is cancelled. */
  saveFormat: (format: ReportFormat, args: { runId: string }) =>
    invoke<ReportFormatExport | null>(REPORT_FORMAT_COMMANDS[format], args),
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

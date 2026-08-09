import { save, open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import type { PipelineReport } from "../lib/types";

interface Props {
  runId?: string | null;
  markdown: string;
  pdfMarkdown?: string;
  provenanceMarkdown?: string;
  report?: PipelineReport;
  extractedText?: string;
}

interface ExportRunArtifactsResult {
  exportedPath: string;
  fileCount: number;
  bytes: number;
}

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(1)} KB`;
  return `${bytes} bytes`;
}

export default function ExportControls({
  runId,
  markdown,
  pdfMarkdown,
  provenanceMarkdown,
  report,
  extractedText,
}: Props) {
  const [exportingRun, setExportingRun] = useState(false);
  const [exportingCoreFiles, setExportingCoreFiles] = useState(false);

  const handleSaveMd = async () => {
    try {
      const path = await save({
        defaultPath: `PIPELINE_REPORT_${new Date().toISOString().slice(0, 10)}.md`,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (path) {
        await invoke("save_report_md", { path, markdown });
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      alert(`Failed to save: ${msg}`);
    }
  };

  const handlePrint = async () => {
    try {
      await invoke("print_report_html", {
        markdown: pdfMarkdown ?? markdown,
        provenanceMarkdown: provenanceMarkdown ?? null,
      });
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      alert(`Failed to generate PDF: ${msg}`);
    }
  };

  const handleExportCoreFiles = async () => {
    if (!report || extractedText === undefined) return;
    setExportingCoreFiles(true);
    try {
      const dir = await open({
        directory: true,
        multiple: false,
        title: "Choose folder for core report files",
      });
      if (!dir) return;
      const confirmed = window.confirm(
        `Export a core report package under "${dir}"?\n\n` +
        "Pipeline will create a new unique pipeline-core-export folder atomically. " +
        "Existing files and earlier exports will not be replaced.\n\n" +
        "The package includes report.md, extracted_text.md, orientation.json, and step reports. " +
        "Saved-run source files, page images, figures, and logs are not included.",
      );
      if (!confirmed) return;
      await invoke("save_all_artifacts", {
        dir,
        markdown,
        extractedText,
        report,
      });
      alert(
        "Core report files exported to a new pipeline-core-export folder. Existing files were not replaced.",
      );
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      alert(`Failed to export core report files: ${msg}`);
    } finally {
      setExportingCoreFiles(false);
    }
  };

  const handleExportRun = async () => {
    if (!runId) return;
    setExportingRun(true);
    try {
      const destination = await open({
        directory: true,
        multiple: false,
        title: "Choose parent folder for complete run export",
      });
      if (!destination) return;
      const result = await invoke<ExportRunArtifactsResult>("export_run_artifacts", {
        runId,
        destination,
      });
      alert(
        `Complete run exported to ${result.exportedPath} ` +
        `(${result.fileCount.toLocaleString()} file${result.fileCount === 1 ? "" : "s"}, ` +
        `${formatBytes(result.bytes)}).`,
      );
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      alert(`Failed to export complete run: ${msg}`);
    } finally {
      setExportingRun(false);
    }
  };

  return (
    <div className="flex gap-2">
      <button
        onClick={handleSaveMd}
        className="py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                   text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
      >
        Save MD
      </button>
      <button
        onClick={handlePrint}
        className="py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                   text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
      >
        Save PDF
      </button>
      {runId ? (
        <button
          onClick={handleExportRun}
          disabled={exportingRun}
          title="Exports the complete durable run, including its manifest, source artifacts, page images, figures, reports, and logs."
          className="py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                     text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors
                     disabled:cursor-wait disabled:opacity-50"
        >
          {exportingRun ? "Exporting…" : "Export complete run"}
        </button>
      ) : report && extractedText !== undefined ? (
        <button
          onClick={handleExportCoreFiles}
          disabled={exportingCoreFiles}
          title="Exports the report, extracted text, orientation, and step reports. It does not copy the complete saved run."
          className="py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                     text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors
                     disabled:cursor-wait disabled:opacity-50"
        >
          {exportingCoreFiles ? "Exporting…" : "Export core files"}
        </button>
      ) : null}
    </div>
  );
}

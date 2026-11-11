import { reportExportClient } from "../lib/reportExportClient";
import type {
  ExportMode,
  ExportSelection,
  ExportPackageResult,
} from "../lib/reportExportClient";
import { open } from "@tauri-apps/plugin-dialog";
import { useMemo, useRef, useState } from "react";
import type { PipelineReport } from "../lib/types";
import ExportDialog from "./ExportDialog";
import { confirmDialog, notify } from "./DialogService";
import ReportFormatItems from "./ReportFormatItems";

interface Props {
  runId?: string | null;
  markdown: string;
  pdfMarkdown?: string;
  /** Replaces `markdown` and `pdfMarkdown` when those are a cut preview. */
  loadFullReport?: () => Promise<{ markdown: string; pdfMarkdown: string }>;
  provenanceMarkdown?: string;
  report?: PipelineReport;
  extractedText?: string;
  /** Saved input path; enables exports onto the reviewed .docx or PDF. */
  inputPath?: string | null;
}

const SAFE_SELECTION: ExportSelection = {
  report: true,
  verifiedFindings: true,
  provenance: true,
  workflow: false,
  sourceDocuments: false,
  rawResponses: false,
  logs: false,
  supportingArtifacts: false,
};

const packageName = (mode: ExportMode) =>
  mode === "forensic"
    ? "Complete archive"
    : `${mode[0].toUpperCase()}${mode.slice(1)} package`;

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
  loadFullReport,
  provenanceMarkdown,
  report,
  extractedText,
  inputPath,
}: Props) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [purpose, setPurpose] = useState<"read" | "share" | "archive">("read");
  const [customOpen, setCustomOpen] = useState(false);
  const [selection, setSelection] = useState<ExportSelection>(SAFE_SELECTION);
  const [progress, setProgress] = useState<string | null>(null);
  const [result, setResult] = useState<ExportPackageResult | null>(null);

  const customSensitive = useMemo(
    () =>
      selection.report ||
      selection.sourceDocuments ||
      selection.rawResponses ||
      selection.logs ||
      selection.supportingArtifacts,
    [selection],
  );

  const handleSaveMd = async () => {
    setMenuOpen(false);
    try {
      const suggestedName = `PIPELINE_REPORT_${new Date().toISOString().slice(0, 10)}.md`;
      const full = await loadFullReport?.();
      await reportExportClient.saveMarkdown({
        markdown: full?.markdown ?? markdown,
        suggestedName,
      });
    } catch (e: unknown) {
      notify(`Failed to save: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handlePrint = async () => {
    setMenuOpen(false);
    try {
      const full = await loadFullReport?.();
      await reportExportClient.print({
        markdown: full?.pdfMarkdown ?? pdfMarkdown ?? markdown,
        provenanceMarkdown: provenanceMarkdown ?? null,
      });
    } catch (e: unknown) {
      notify(
        `Failed to open the print view: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  };

  const handleLegacyCore = async () => {
    if (!report || extractedText === undefined) return;
    setMenuOpen(false);
    setProgress("Preparing core report package…");
    try {
      const dir = await open({
        directory: true,
        multiple: false,
        title: "Choose folder for core report files",
      });
      if (!dir) return;
      await reportExportClient.saveCore({
        dir,
        markdown,
        extractedText,
        report,
      });
      notify(
        "Core report package exported. Existing files were not replaced.",
        "success",
      );
    } catch (e: unknown) {
      notify(
        `Failed to export core report files: ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setProgress(null);
    }
  };

  const runPackageExport = async (
    mode: ExportMode,
    chosen: ExportSelection,
  ) => {
    if (!runId) return;
    setMenuOpen(false);
    if (mode === "forensic") {
      const confirmed = await confirmDialog(
        "Export the complete review? It includes source documents, raw model responses, logs, the review design, settings, and file list. These may contain private information.",
        {
          title: "Export sensitive complete archive",
          confirmLabel: "Export archive",
        },
      );
      if (!confirmed) return;
    } else if (mode === "custom" && customSensitive) {
      const confirmed = await confirmDialog(
        "This custom package includes sensitive items, such as the full report with unverified findings, source material, raw responses, logs, or supporting artifacts. Export the selected items?",
        {
          title: "Export sensitive custom package",
          confirmLabel: "Export selected items",
        },
      );
      if (!confirmed) return;
    }
    const name = packageName(mode);
    setProgress(`Preparing ${name.toLowerCase()}…`);
    setResult(null);
    try {
      const destination = await open({
        directory: true,
        multiple: false,
        title: `Choose parent folder for ${name.toLowerCase()}`,
      });
      if (!destination) return;
      setProgress("Copying files and calculating checksums…");
      const next = await reportExportClient.package({
        runId,
        destination,
        mode,
        selection: chosen,
      });
      setResult(next);
      setCustomOpen(false);
      notify(`${name} exported.`, "success");
    } catch (e: unknown) {
      notify(
        `Failed to export package: ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setProgress(null);
    }
  };

  const reveal = async () => {
    if (!result) return;
    try {
      await reportExportClient.reveal({ path: result.exportedPath });
    } catch (e: unknown) {
      notify(
        `Could not reveal the package: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  };

  const setCustomItem = (key: keyof ExportSelection, checked: boolean) =>
    setSelection((current) => ({ ...current, [key]: checked }));

  return (
    <div className="relative flex flex-col items-end gap-2">
      <button
        ref={trigger}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={menuOpen}
        onClick={() => {
          setMenuOpen((open) => !open);
          setCustomOpen(false);
        }}
        disabled={Boolean(progress)}
        className="py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                   text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors
                   disabled:cursor-wait disabled:opacity-50"
      >
        {progress ?? "Export"}
      </button>

      {menuOpen && (
        <ExportDialog
          onClose={() => {
            setMenuOpen(false);
            setCustomOpen(false);
          }}
        >
          <div
            aria-label="Export purpose"
            className="mb-5 grid grid-cols-3 gap-2"
          >
            {(["read", "share", "archive"] as const).map((value) => (
              <button
                key={value}
                aria-pressed={purpose === value}
                className="rounded-lg border border-line px-3 py-3 text-sm font-medium aria-pressed:bg-inverse aria-pressed:text-on-inverse"
                onClick={() => {
                  setPurpose(value);
                  setCustomOpen(false);
                }}
              >
                {value === "read"
                  ? "Read or print"
                  : value === "share"
                    ? "Share findings"
                    : "Keep an archive"}
              </button>
            ))}
          </div>
          {purpose === "read" ? (
            <>
              <p className="mb-3 text-sm text-ink-muted">
                Choose a report format or add findings to a copy of your
                document.
              </p>
              {runId && (
                <>
                  {" "}
                  <ReportFormatItems
                    inDialog
                    runId={runId}
                    inputPath={inputPath}
                    busy={Boolean(progress)}
                    onStart={(next) => {
                      setMenuOpen(false);
                      setProgress(next);
                    }}
                    onFinish={() => setProgress(null)}
                  />
                </>
              )}
              {!runId && report && extractedText !== undefined && (
                <button
                  onClick={() => void handleLegacyCore()}
                  className="w-full rounded px-3 py-2 text-left text-sm text-gray-800 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-800"
                >
                  Export current report files…
                </button>
              )}
              <button
                onClick={() => void handleSaveMd()}
                className="w-full rounded px-3 py-2 text-left text-sm text-gray-800 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-800"
              >
                Save Markdown…
              </button>
              <button
                onClick={() => void handlePrint()}
                className="w-full rounded px-3 py-2 text-left text-sm text-gray-800 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-800"
              >
                Print / save as PDF…
              </button>
            </>
          ) : purpose === "share" ? (
            <>
              <p className="mb-3 text-sm text-ink-muted">
                Create a folder of verified findings and report details for
                collaborators.
              </p>
              {runId ? (
                <>
                  {" "}
                  <button
                    onClick={() =>
                      void runPackageExport("shareable", SAFE_SELECTION)
                    }
                    className="w-full rounded px-3 py-2 text-left hover:bg-gray-100 dark:hover:bg-gray-800"
                  >
                    <span className="block text-sm font-medium text-gray-900 dark:text-gray-100">
                      Shareable report
                    </span>
                    <span className="block text-xs text-gray-500 dark:text-gray-400">
                      Verified findings, limitations, and report details with
                      private information removed. Excludes unverified findings,
                      source documents, and raw model responses.
                    </span>
                  </button>
                </>
              ) : (
                <p>
                  Sharing a findings package is available for saved reviews.
                </p>
              )}
            </>
          ) : customOpen ? (
            <>
              {" "}
              <div className="mb-3 flex items-start justify-between gap-3">
                <div>
                  <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">
                    Custom export
                  </h3>
                  <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                    Choose the types of files to export. Pipeline includes a
                    file list and checksums to help detect changes.
                  </p>
                </div>
                <button
                  aria-label="Close custom export"
                  onClick={() => setCustomOpen(false)}
                  className="text-gray-500 hover:text-gray-900 dark:hover:text-gray-100"
                >
                  ✕
                </button>
              </div>
              <div className="space-y-2 text-sm text-gray-700 dark:text-gray-300">
                {(
                  [
                    ["report", "Full report · Includes unverified findings"],
                    ["verifiedFindings", "Verified findings and limitations"],
                    [
                      "provenance",
                      "Report details with private information removed",
                    ],
                    ["workflow", "Saved review design"],
                    [
                      "supportingArtifacts",
                      "Supporting files · May contain private information",
                    ],
                    ["sourceDocuments", "Source documents · Sensitive"],
                    [
                      "rawResponses",
                      "Raw responses and step outputs · Sensitive",
                    ],
                    ["logs", "Console logs · Sensitive"],
                  ] as [keyof ExportSelection, string][]
                ).map(([key, label]) => (
                  <label key={key} className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      checked={selection[key]}
                      onChange={(event) =>
                        setCustomItem(key, event.target.checked)
                      }
                    />
                    {label}
                  </label>
                ))}
              </div>
              {customSensitive && (
                <p className="mt-3 rounded bg-amber-50 px-2 py-1.5 text-xs text-amber-800 dark:bg-amber-950/40 dark:text-amber-300">
                  This selection includes private material. Check the files
                  before sharing them.
                </p>
              )}
              <button
                onClick={() => void runPackageExport("custom", selection)}
                disabled={!Object.values(selection).some(Boolean)}
                className="mt-4 w-full rounded-lg bg-gray-900 px-3 py-2 text-sm text-white disabled:cursor-not-allowed disabled:opacity-40 dark:bg-gray-100 dark:text-gray-900"
              >
                Export selected items…
              </button>
            </>
          ) : (
            <>
              <p className="mb-3 text-sm text-ink-muted">
                Keep the full review, or choose which files to include.
              </p>
              {runId ? (
                <>
                  <button
                    onClick={() =>
                      void runPackageExport("forensic", SAFE_SELECTION)
                    }
                    className="w-full rounded px-3 py-2 text-left hover:bg-gray-100 dark:hover:bg-gray-800"
                  >
                    <span className="block text-sm font-medium text-amber-700 dark:text-amber-300">
                      Complete review archive · Includes private material
                    </span>
                    <span className="block text-xs text-gray-500 dark:text-gray-400">
                      All review files, including source documents, raw model
                      responses, logs, the review design, settings, and file
                      list.
                    </span>
                  </button>
                  <button
                    onClick={() => {
                      setCustomOpen(true);
                    }}
                    className="w-full rounded px-3 py-2 text-left text-sm text-gray-800 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-800"
                  >
                    Custom selection…
                  </button>
                </>
              ) : (
                <p>Archives are available for saved reviews.</p>
              )}
            </>
          )}
        </ExportDialog>
      )}

      {/* Floats over the report so the header keeps its height. */}
      <div
        aria-live="polite"
        className="absolute right-0 top-full z-30 mt-2 w-[28rem] max-w-[calc(100vw-2rem)] text-right"
      >
        {progress && (
          <p className="text-xs text-gray-500 dark:text-gray-400">{progress}</p>
        )}
        {result && (
          <div className="rounded border border-green-200 bg-green-50 px-3 py-2 text-left text-xs text-green-900 shadow-lg dark:border-green-900 dark:bg-green-950 dark:text-green-200">
            <div className="flex items-start justify-between gap-3">
              <p className="font-medium">
                {packageName(result.mode)} ready · {result.fileCount} files ·{" "}
                {formatBytes(result.bytes)}
              </p>
              <button
                type="button"
                aria-label="Dismiss export result"
                onClick={() => setResult(null)}
                className="shrink-0 hover:opacity-70"
              >
                ✕
              </button>
            </div>
            <p className="mt-1 break-all" title={result.exportedPath}>
              {result.exportedPath}
            </p>
            <p
              className="mt-1 font-mono break-all"
              title="SHA-256 of checksums.sha256"
            >
              SHA-256 {result.checksum}
            </p>
            <button
              onClick={() => void reveal()}
              className="mt-2 rounded border border-green-300 px-2 py-1 hover:bg-green-100 dark:border-green-800 dark:hover:bg-green-900/40"
            >
              Reveal in folder
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

import ExportControls from "../ExportControls";
import { router } from "../../lib/router";
import {
  formatRunDuration,
  provenanceMarkdown as renderProvenanceMarkdown,
  type RunProvenance,
} from "../../lib/runProvenance";
import type { PipelineReport } from "../../lib/reviewTypes";
import type { RunManifest } from "../../lib/artifactTypes";
import { reportStatusDisplay } from "../../lib/reportPresentation";

interface Props {
  onBack?: () => void;
  status: string;
  title: string;
  provenance: RunProvenance | null;
  report: PipelineReport | null;
  runManifest: RunManifest | null;
  runId?: string | null;
  markdown: string;
  pdfMarkdown: string;
  loadFullReport?: () => Promise<{ markdown: string; pdfMarkdown: string }>;
  extractedText?: string;
}
export default function ReportHeader({
  onBack,
  status,
  title,
  provenance,
  report,
  runManifest,
  runId,
  markdown,
  pdfMarkdown,
  loadFullReport,
  extractedText,
}: Props) {
  const display = reportStatusDisplay(status, report?.quality);
  return (
    <header className="shrink-0 border-b border-line bg-surface px-[var(--ui-page-gutter)] py-3">
      <div className="flex min-w-0 flex-wrap items-start gap-3">
        {onBack && (
          <button
            type="button"
            onClick={onBack}
            aria-label="Back to history"
            className="mt-0.5 rounded-lg p-1.5 text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-800
                         focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
          >
            <svg
              aria-hidden="true"
              className="h-4 w-4"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={1.8}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="m15 18-6-6 6-6"
              />
            </svg>
          </button>
        )}
        <div className="min-w-0 flex-[1_1_16rem]">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span
              className={`h-1.5 w-1.5 rounded-full ${
                display.tone === "good"
                  ? "bg-emerald-500"
                  : display.tone === "caution"
                    ? "bg-amber-500"
                    : display.tone === "stopped"
                      ? "bg-gray-400"
                      : "bg-red-500"
              }`}
            />
            <span className="text-ui-meta font-semibold text-ink">
              {display.heading}
            </span>
            {display.notes.length > 0 && (
              <span className="min-w-0 break-words text-ui-meta text-ink-muted">
                {display.notes.join(" · ")}
              </span>
            )}
          </div>
          <h1
            className="mt-1 line-clamp-2 break-words text-xl font-semibold tracking-tight text-ink"
            title={title}
          >
            {title}
          </h1>
          <p className="mt-1 truncate text-ui-meta text-ink-muted">
            {provenance ? (
              <>
                {provenance.workflow}
                <span className="mx-1.5 text-gray-300 dark:text-gray-700">
                  ·
                </span>
                <span title={provenance.provider_summary}>
                  {provenance.provider_summary}
                </span>
                <span className="mx-1.5 text-gray-300 dark:text-gray-700">
                  ·
                </span>
                <span title={provenance.model_summary}>
                  {provenance.model_summary}
                </span>
                <span className="mx-1.5 text-gray-300 dark:text-gray-700">
                  ·
                </span>
                {formatRunDuration(provenance.duration_secs)}
              </>
            ) : (
              "Report details unavailable · saved outputs are still accessible"
            )}
          </p>
        </div>
        {report && provenance && (
          <div className="flex shrink-0 items-center gap-2">
            {runManifest?.input_path && (
              <button
                type="button"
                title="Chain this review into an automation — for example, address the findings and review again"
                onClick={() =>
                  void router.navigate({
                    page: "tasks",
                    automatePath: runManifest.input_path,
                  })
                }
                className="ui-button ui-button--sm"
              >
                Automate follow-up
              </button>
            )}
            <ExportControls
              runId={runId}
              markdown={markdown}
              pdfMarkdown={pdfMarkdown}
              loadFullReport={loadFullReport}
              provenanceMarkdown={renderProvenanceMarkdown(provenance)}
              report={report}
              extractedText={extractedText}
              inputPath={runManifest?.input_path}
            />
          </div>
        )}
      </div>
    </header>
  );
}

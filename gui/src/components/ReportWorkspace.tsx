import {
  reportTitle,
  splitUnexpectedPreamble,
  reportBodyForPdf,
  canonicalReportProblem,
  canonicalProblemMessage,
} from "../lib/reportPresentation";
import ProvenancePanel from "./report-workspace/ProvenancePanel";
import LocalSources, { hasPageHeadings } from "./report-workspace/LocalSources";
import ReportHeader from "./report-workspace/ReportHeader";
import KeptTabPanel from "./report-workspace/KeptTabPanel";
import useReportResources from "./report-workspace/useReportResources";
import type { WorkspaceTab, OutputView } from "./report-workspace/types";
import useTabList from "../hooks/useTabList";
import RunMarkdown from "./file-workspace/RunMarkdown";
import { Fragment, lazy, useMemo, useState, type ReactNode } from "react";
import { detectReportIssues, type IssueEvidence } from "../lib/issues";
import { buildRunProvenance } from "../lib/runProvenance";
import {
  isPaperOrientation,
  type PipelineReport,
  type RunSummary,
} from "../lib/types";
import { REPORT_WORKSPACE_TABS } from "../lib/productMetadata";
import type {
  ArtifactSelectionRequest,
  ArtifactSelectionTarget,
} from "../lib/artifactTypes";

const loadArtifactExplorer = () => import("./ArtifactExplorer");
const ArtifactExplorer = lazy(loadArtifactExplorer);
const IssuesTable = lazy(() => import("./IssuesTable"));
const ReportViewer = lazy(() => import("./ReportViewer"));

interface Props {
  runId?: string | null;
  markdown?: string;
  report?: PipelineReport | null;
  extractedText?: string;
  summary?: RunSummary | null;
  durationSecs?: number | null;
  onBack?: () => void;
  /** Open a cited page or artifact when navigating here from a project ledger. */
  initialSourceSelection?: ArtifactSelectionTarget | null;
  /** Shown above the report header; receives the open tab. */
  notice?: (tab: WorkspaceTab) => ReactNode;
}

const TAB_LABELS = Object.fromEntries(
  REPORT_WORKSPACE_TABS.map((tab) => [tab.id, tab.label]),
) as Record<WorkspaceTab, string>;

function TabButton({
  active,
  count,
  label,
  tabProps,
}: {
  active: boolean;
  count?: number;
  label: string;
  tabProps: React.ComponentPropsWithRef<"button">;
}) {
  return (
    <button
      {...tabProps}
      className={`relative h-11 px-1 text-sm font-medium transition-colors focus-visible:outline-none
                  focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-gray-400 ${
                    active
                      ? "text-gray-950 dark:text-gray-50"
                      : "text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-200"
                  }`}
    >
      {label}
      {count !== undefined && (
        <span className="ml-1.5 text-xs tabular-nums text-gray-500 dark:text-gray-400">
          {count}
        </span>
      )}
      {active && (
        <span className="absolute inset-x-0 bottom-0 h-0.5 rounded-full bg-gray-950 dark:bg-gray-100" />
      )}
    </button>
  );
}

export default function ReportWorkspace({
  runId,
  markdown: initialMarkdown = "",
  report: initialReport = null,
  extractedText,
  summary,
  durationSecs,
  onBack,
  initialSourceSelection = null,
  notice,
}: Props) {
  const [tab, setTab] = useState<WorkspaceTab>(
    initialSourceSelection ? "sources" : "report",
  );
  const [sourceRequest, setSourceRequest] =
    useState<ArtifactSelectionRequest | null>(
      initialSourceSelection
        ? { key: Date.now(), ...initialSourceSelection }
        : null,
    );

  const [outputView, setOutputView] = useState<OutputView>("clean");
  const { report, markdown, loading, reportError, markdownError, runManifest } =
    useReportResources({
      runId,
      initialReport,
      initialMarkdown,
      initialSourceSelection,
      setSourceRequest,
      setTab,
      setOutputView,
    });

  const issues = useMemo(
    () => (report ? detectReportIssues(report) : null),
    [report],
  );
  const canOpenEvidence = !!runId || hasPageHeadings(extractedText);
  const openEvidence = (evidence: IssueEvidence) => {
    if (!evidence.page && !evidence.artifactPath) return;
    setSourceRequest({
      key: Date.now(),
      page: evidence.page,
      relPath: evidence.artifactPath,
    });
    setTab("sources");
  };
  const paperTitle =
    report && isPaperOrientation(report.orientation)
      ? (report.orientation.metadata?.title ?? "")
      : "";
  const presentation = useMemo(
    () => splitUnexpectedPreamble(markdown, paperTitle),
    [markdown, paperTitle],
  );
  const visibleMarkdown =
    outputView === "clean" ? presentation.clean : markdown;
  const reportProblem = useMemo(
    () => canonicalReportProblem(report, presentation.clean),
    [presentation.clean, report],
  );
  const pdfMarkdown = useMemo(
    () => reportBodyForPdf(presentation.clean),
    [presentation.clean],
  );
  const provenance = useMemo(() => {
    if (!report) return null;
    const built = buildRunProvenance({
      report,
      summary,
      manifest: runManifest,
      runId,
      durationSecs,
    });
    return reportProblem && built.status === "done"
      ? { ...built, status: "partial" }
      : built;
  }, [durationSecs, report, reportProblem, runId, runManifest, summary]);
  const title = reportTitle(report, summary);
  const loadProblem = [reportError, markdownError].filter(Boolean).join(" · ");
  const reportedStatus =
    report?.quality?.status ||
    provenance?.status ||
    runManifest?.status ||
    (report?.failed_steps?.length || loadProblem ? "partial" : "done");
  const status =
    reportProblem && reportedStatus === "done" ? "partial" : reportedStatus;
  const availableTabs: WorkspaceTab[] = [
    "report",
    ...(provenance ? ["provenance" as const] : []),
    ...(issues && issues.length > 0 ? ["issues" as const] : []),
    "sources",
  ];
  const openAgentReports = () => {
    const first = runManifest?.artifacts.find(
      (artifact) => artifact.group === "agent_response",
    );
    if (first) {
      setSourceRequest({ key: Date.now(), relPath: first.rel_path });
    }
    setTab("sources");
  };
  const tabList = useTabList(availableTabs, tab, setTab);
  const activeTab = availableTabs.includes(tab) ? tab : "report";
  // Tabs opened since this report loaded. Their panels stay mounted; a newly
  // loaded report starts over with fresh panels.
  const [panels, setPanels] = useState({
    runId,
    report,
    generation: 0,
    visited: [activeTab],
  });
  if (panels.runId !== runId || panels.report !== report) {
    setPanels({
      runId,
      report,
      generation: panels.generation + 1,
      visited: [activeTab],
    });
  } else if (!panels.visited.includes(activeTab)) {
    setPanels({ ...panels, visited: [...panels.visited, activeTab] });
  }

  const panelContent = (id: WorkspaceTab) => {
    if (id === "issues")
      return (
        issues && (
          <div className="h-full overflow-auto">
            <IssuesTable
              issues={issues}
              runId={runId ?? null}
              onOpenEvidence={canOpenEvidence ? openEvidence : undefined}
            />
          </div>
        )
      );
    if (id === "provenance")
      return (
        provenance && (
          <div className="h-full">
            <ProvenancePanel
              provenance={provenance}
              quality={report?.quality}
            />
          </div>
        )
      );
    if (id === "sources")
      return (
        <div className="h-full">
          {runId ? (
            <ArtifactExplorer
              runId={runId}
              fallbackMarkdown={markdown}
              deferInitialArtifact
              selectionRequest={sourceRequest}
            />
          ) : report ? (
            <LocalSources
              ReportViewer={ReportViewer}
              extractedText={extractedText ?? ""}
              report={report}
              selectionRequest={sourceRequest}
            />
          ) : (
            <div className="flex h-full items-center justify-center p-8 text-sm text-gray-500 dark:text-gray-400">
              No local source material is available.
            </div>
          )}
        </div>
      );
    return (
      <div className="h-full overflow-hidden">
        <div className="flex h-full min-h-0 flex-col">
          {presentation.preamble && (
            <div
              role="status"
              className="flex shrink-0 items-center gap-3 border-b border-amber-200 bg-amber-50/80 px-6 py-2
                               text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-300"
            >
              <span className="font-semibold">Output-quality warning</span>
              <span className="min-w-0 flex-1 truncate">
                Clean view hides the model’s process commentary.
              </span>
              <div className="flex rounded-md border border-amber-300/80 p-0.5 dark:border-amber-800">
                {(["clean", "raw"] as OutputView[]).map((view) => (
                  <button
                    key={view}
                    type="button"
                    onClick={() => setOutputView(view)}
                    className={`rounded px-2 py-0.5 text-[11px] font-medium ${
                      outputView === view
                        ? "bg-amber-900 text-white dark:bg-amber-200 dark:text-amber-950"
                        : "hover:bg-amber-100 dark:hover:bg-amber-900/50"
                    }`}
                  >
                    {view === "clean" ? "Clean view" : "Raw output"}
                  </button>
                ))}
              </div>
            </div>
          )}
          <div className="min-h-0 flex-1 overflow-hidden">
            {reportProblem ? (
              <div className="flex h-full items-center justify-center p-8 text-center">
                <div role="alert" className="max-w-lg">
                  <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-300">
                    <svg
                      aria-hidden="true"
                      className="h-5 w-5"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                      strokeWidth={1.8}
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        d="M12 9v4m0 4h.01M10.3 3.6 2.6 17a2 2 0 0 0 1.7 3h15.4a2 2 0 0 0 1.7-3L13.7 3.6a2 2 0 0 0-3.4 0Z"
                      />
                    </svg>
                  </div>
                  <p className="mt-3 font-medium text-gray-800 dark:text-gray-100">
                    No final report was produced.
                  </p>
                  <p className="mt-1 text-xs leading-relaxed text-gray-500 dark:text-gray-400">
                    {canonicalProblemMessage(reportProblem)}
                  </p>
                  <button
                    type="button"
                    onClick={openAgentReports}
                    className="mt-4 rounded border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
                  >
                    Inspect preserved outputs
                  </button>
                </div>
              </div>
            ) : markdown.trim() ? (
              runId ? (
                <RunMarkdown
                  runId={runId}
                  onOpen={(location) => {
                    setSourceRequest({
                      key: Date.now(),
                      relPath: location.path,
                      ...location,
                    });
                    setTab("sources");
                  }}
                >
                  <ReportViewer markdown={visibleMarkdown} />
                </RunMarkdown>
              ) : (
                <ReportViewer markdown={visibleMarkdown} />
              )
            ) : (
              <div className="flex h-full items-center justify-center p-8 text-center text-sm text-gray-500 dark:text-gray-400">
                <div>
                  <p className="font-medium text-gray-700 dark:text-gray-200">
                    The final report is unavailable.
                  </p>
                  <p className="mt-1 max-w-md text-xs">
                    Open Agent reports to read all saved responses, including
                    attempts that failed checks.
                  </p>
                  {runId && (
                    <button
                      type="button"
                      onClick={openAgentReports}
                      className="mt-3 rounded border border-gray-300 px-3 py-1.5 text-xs font-medium hover:bg-gray-100 dark:border-gray-700 dark:hover:bg-gray-800"
                    >
                      View agent reports
                    </button>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    );
  };

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-gray-500 dark:text-gray-400">
        Loading report…
      </div>
    );
  }

  if (!runId && (!report || !provenance)) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <div
          role="alert"
          className="max-w-md rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-300"
        >
          <p className="font-medium">Could not open this report.</p>
          <p className="mt-1 text-xs">The structured report is unavailable.</p>
          {onBack && (
            <button
              type="button"
              onClick={onBack}
              className="mt-3 font-medium underline"
            >
              Back to history
            </button>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col bg-white dark:bg-gray-950">
      {notice?.(activeTab)}
      <ReportHeader
        onBack={onBack}
        status={status}
        title={title}
        provenance={provenance}
        report={report}
        runManifest={runManifest}
        runId={runId}
        markdown={markdown}
        pdfMarkdown={pdfMarkdown}
        extractedText={extractedText}
      />

      {loadProblem && (
        <div className="flex shrink-0 items-center gap-3 border-b border-amber-200 bg-amber-50 px-6 py-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/35 dark:text-amber-300">
          <span className="min-w-0 flex-1 truncate">
            <span className="font-semibold">
              The final report is incomplete.
            </span>{" "}
            {loadProblem} Agent reports and other report artifacts remain
            available.
          </span>
          <button
            type="button"
            onClick={openAgentReports}
            className="shrink-0 rounded border border-amber-300 px-2 py-1 font-medium hover:bg-amber-100 dark:border-amber-800 dark:hover:bg-amber-900/40"
          >
            View agent reports
          </button>
        </div>
      )}

      {report?.failed_steps && report.failed_steps.length > 0 && (
        <div
          role="alert"
          className="shrink-0 border-b border-amber-200 bg-amber-50 px-6 py-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/35 dark:text-amber-300"
        >
          <div>
            <span className="font-semibold">Incomplete report.</span>{" "}
            {report.failed_steps
              .map((failure) => failure.step_label)
              .join(", ")}{" "}
            did not complete.
            {runId && (
              <button
                type="button"
                onClick={openAgentReports}
                className="ml-2 font-semibold underline underline-offset-2"
              >
                View agent reports
              </button>
            )}
          </div>
          <details className="mt-1">
            <summary className="cursor-pointer font-medium">
              Show failure details
            </summary>
            <ul className="mt-1 list-disc space-y-0.5 pl-4">
              {report.failed_steps.map((failure) => (
                <li key={`${failure.step_id}-${failure.step_label}`}>
                  <span className="font-medium">{failure.step_label}:</span>{" "}
                  <span className="break-words">{failure.error}</span>
                </li>
              ))}
            </ul>
          </details>
        </div>
      )}

      <div
        role="tablist"
        aria-label="Report workspace"
        className="flex h-11 shrink-0 items-center gap-6 border-b border-gray-200 bg-white px-6 dark:border-gray-800 dark:bg-gray-950"
      >
        <TabButton
          active={tab === "report"}
          tabProps={tabList.tabProps("report")}
          label={TAB_LABELS.report}
        />
        {provenance && (
          <TabButton
            active={tab === "provenance"}
            tabProps={tabList.tabProps("provenance")}
            label={TAB_LABELS.provenance}
          />
        )}
        {issues && issues.length > 0 && (
          <TabButton
            active={tab === "issues"}
            count={issues.length}
            tabProps={tabList.tabProps("issues")}
            label={TAB_LABELS.issues}
          />
        )}
        <TabButton
          active={tab === "sources"}
          tabProps={tabList.tabProps("sources")}
          label={TAB_LABELS.sources}
        />
      </div>

      <Fragment key={panels.generation}>
        {availableTabs.map((id) => (
          <KeptTabPanel
            key={id}
            active={id === activeTab}
            panelProps={tabList.panelProps(id)}
          >
            {panels.visited.includes(id) && panelContent(id)}
          </KeptTabPanel>
        ))}
      </Fragment>
    </div>
  );
}

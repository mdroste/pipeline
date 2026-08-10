import { useState } from "react";
import PaperSelector from "./PaperSelector";
import ResizeHandle from "./ResizeHandle";
import WorkflowPanel from "./WorkflowPanel";
import type { PipelineConfig, PrimaryInputSelection } from "../lib/types";

const PRIVACY_NOTICE_KEY = "pipeline.privacyNoticeAcknowledged.v1";

interface Props {
  configVersion: number;
  configError: string | null;
  configLoading: boolean;
  dependenciesError: string | null;
  dependenciesLoading: boolean;
  dependenciesReady: boolean;
  inputMode: string;
  listenersReady: boolean;
  paperPath: string | null;
  preparingRun: boolean;
  selectionKey: number;
  width: number;
  onConfigureWorkflow: () => void;
  onGenerate: () => void;
  onPaperPathChange: (path: string | null) => void;
  onInputSelectionChange: (selection: PrimaryInputSelection | null) => void;
  onPrivacyDetails: () => void;
  onProfileChange: (config?: PipelineConfig) => void;
  onRetryConfig: () => void;
  onRetryDependencies: () => void;
  onResize: (width: number) => void;
}

export default function RunSetupPanel({
  configVersion,
  configError,
  configLoading,
  dependenciesError,
  dependenciesLoading,
  dependenciesReady,
  inputMode,
  listenersReady,
  paperPath,
  preparingRun,
  selectionKey,
  width,
  onConfigureWorkflow,
  onGenerate,
  onPaperPathChange,
  onInputSelectionChange,
  onPrivacyDetails,
  onProfileChange,
  onRetryConfig,
  onRetryDependencies,
  onResize,
}: Props) {
  const [showPrivacyNotice, setShowPrivacyNotice] = useState(() => {
    try {
      return localStorage.getItem(PRIVACY_NOTICE_KEY) !== "true";
    } catch {
      return true;
    }
  });
  const canRun =
    (paperPath !== null || inputMode === "none") &&
    !configLoading &&
    !configError &&
    !dependenciesLoading &&
    dependenciesReady &&
    listenersReady &&
    !preparingRun;

  return (
    <aside
      data-testid="run-setup-panel"
      style={{ width }}
      className="relative flex shrink-0 flex-col border-r border-gray-200/80 bg-white
                 dark:border-gray-800 dark:bg-gray-900"
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-5 py-6">
        <header className="mb-6">
          <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-gray-500 dark:text-gray-400">
            Workspace
          </p>
          <h1 className="mt-1 text-xl font-semibold tracking-[-0.02em] text-gray-950 dark:text-gray-50">
            New run
          </h1>
          <p className="mt-1 text-xs leading-5 text-gray-500 dark:text-gray-400">
            Choose inputs, say how Pipeline should use them, and select a workflow.
          </p>
        </header>

        <div className="space-y-5">
          {inputMode === "none" ? (
            <p className="rounded-lg border border-gray-200 bg-gray-50 p-3 text-xs leading-5 text-gray-600 dark:border-gray-700 dark:bg-gray-800/50 dark:text-gray-300">
              This workflow runs without a source document.
            </p>
          ) : (
            <PaperSelector
              key={selectionKey}
              inputMode={inputMode}
              onPathChange={onPaperPathChange}
              onSelectionChange={onInputSelectionChange}
              disabled={configLoading}
            />
          )}

          <WorkflowPanel
            disabled={configLoading || preparingRun}
            editorOpen={false}
            onConfigure={onConfigureWorkflow}
            onProfileChange={onProfileChange}
            refreshKey={configVersion}
          />
        </div>

        <div className="mt-auto pt-6">
          {configError && (
            <div role="alert" className="mb-3 rounded-lg border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/35 dark:text-red-300">
              <p className="font-medium">Workflow setup could not be loaded.</p>
              <p className="mt-1 break-words">{configError}</p>
              <button type="button" onClick={onRetryConfig} className="mt-2 font-medium underline underline-offset-2">
                Retry workflow
              </button>
            </div>
          )}
          {dependenciesError && (
            <div role="alert" className="mb-3 rounded-lg border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/35 dark:text-red-300">
              <p className="font-medium">System readiness check failed.</p>
              <p className="mt-1 break-words">{dependenciesError}</p>
              <button type="button" onClick={onRetryDependencies} className="mt-2 font-medium underline underline-offset-2">
                Retry system check
              </button>
            </div>
          )}
          {showPrivacyNotice && (
            <div
              role="note"
              aria-label="Data and privacy"
              className="mb-3 rounded-lg border border-amber-200 bg-amber-50 p-3 text-xs leading-5
                         text-amber-900 dark:border-amber-900 dark:bg-amber-950/35 dark:text-amber-200"
            >
              <p>
                Your selected source and the context granted to a step may be sent to its
                configured model provider. WebSearch steps also send queries to an external
                search service. Run records and artifacts persist locally until removed.
              </p>
              <div className="mt-2 flex items-center gap-3">
                <button
                  type="button"
                  onClick={onPrivacyDetails}
                  className="font-medium underline underline-offset-2 hover:text-amber-700 dark:hover:text-amber-100"
                >
                  Privacy details
                </button>
                <button
                  type="button"
                  onClick={() => {
                    try {
                      localStorage.setItem(PRIVACY_NOTICE_KEY, "true");
                    } catch {
                      // The notice can still be dismissed for this session.
                    }
                    setShowPrivacyNotice(false);
                  }}
                  className="ml-auto rounded border border-amber-300 px-2 py-0.5 font-medium
                             hover:bg-amber-100 dark:border-amber-800 dark:hover:bg-amber-900/50"
                >
                  Got it
                </button>
              </div>
            </div>
          )}
          <button
            type="button"
            onClick={onGenerate}
            disabled={!canRun}
            className="flex w-full items-center justify-center gap-2 rounded-lg bg-gray-950 px-4 py-2.5 text-sm font-medium
                       text-white transition-colors hover:bg-gray-800 focus-visible:outline-none focus-visible:ring-2
                       focus-visible:ring-gray-400 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:bg-gray-300
                       dark:bg-gray-100 dark:text-gray-950 dark:hover:bg-white dark:focus-visible:ring-offset-gray-900
                       dark:disabled:bg-gray-700 dark:disabled:text-gray-400"
          >
            {preparingRun
              ? "Preparing run…"
              : configLoading
                ? "Loading workflow…"
                : dependenciesLoading
                  ? "Checking dependencies…"
                  : !listenersReady
                    ? "Preparing event stream…"
                    : "Run"}
            {!dependenciesLoading && !configLoading && !preparingRun && listenersReady && (
              <svg aria-hidden="true" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.7}>
                <path strokeLinecap="round" strokeLinejoin="round" d="m9 5 7 7-7 7" />
              </svg>
            )}
          </button>
        </div>
      </div>
      <ResizeHandle
        currentWidth={width}
        defaultWidth={288}
        label="Resize run setup"
        min={240}
        max={440}
        onResize={onResize}
      />
    </aside>
  );
}

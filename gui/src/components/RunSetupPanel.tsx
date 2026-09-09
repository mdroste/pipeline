import { useState } from "react";
import PaperSelector from "./PaperSelector";
import SidebarPanel, { SidebarHeader } from "./SidebarPanel";
import WorkflowPanel from "./WorkflowPanel";
import RunParallelAgents from "./RunParallelAgents";
import type {
  PipelineConfig,
  PrimaryInputSelection,
  RunParallelOverrides,
} from "../lib/types";

const PRIVACY_NOTICE_KEY = "pipeline.privacyNoticeAcknowledged.v1";

interface Props {
  configVersion: number;
  configError: string | null;
  configLoading: boolean;
  dependenciesError: string | null;
  inputMode: string;
  listenersReady: boolean;
  localLlmActive: boolean;
  paperPath: string | null;
  preparingRun: boolean;
  parallelOverrides: RunParallelOverrides | null;
  selectionKey: number;
  width: number;
  onConfigureWorkflow: () => void;
  onGenerate: () => void;
  onPaperPathChange: (path: string | null) => void;
  onInputSelectionChange: (selection: PrimaryInputSelection | null) => void;
  onPrivacyDetails: () => void;
  onParallelOverridesChange: (overrides: RunParallelOverrides | null) => void;
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
  inputMode,
  listenersReady,
  localLlmActive,
  paperPath,
  preparingRun,
  parallelOverrides,
  selectionKey,
  width,
  onConfigureWorkflow,
  onGenerate,
  onPaperPathChange,
  onInputSelectionChange,
  onPrivacyDetails,
  onParallelOverridesChange,
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
    listenersReady &&
    !preparingRun;

  return (
    <SidebarPanel
      data-testid="run-setup-panel"
      width={width}
      defaultWidth={288}
      min={240}
      max={440}
      onResize={onResize}
      resizeLabel="Resize report setup"
    >
      <SidebarHeader title="New run" heading="h1" />
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-5 pb-6 pt-1">
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

          <RunParallelAgents
            value={parallelOverrides}
            disabled={configLoading || preparingRun}
            localLlmActive={localLlmActive}
            refreshKey={configVersion}
            onChange={onParallelOverridesChange}
          />
        </div>

        <div className="mt-auto pt-6">
          {configError && (
            <div
              role="alert"
              className="mb-3 rounded-lg border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/35 dark:text-red-300"
            >
              <p className="font-medium">Workflow setup could not be loaded.</p>
              <p className="mt-1 break-words">{configError}</p>
              <button
                type="button"
                onClick={onRetryConfig}
                className="mt-2 font-medium underline underline-offset-2"
              >
                Retry workflow
              </button>
            </div>
          )}
          {dependenciesError && (
            <div
              role="alert"
              className="mb-3 rounded-lg border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/35 dark:text-red-300"
            >
              <p className="font-medium">System readiness check failed.</p>
              <p className="mt-1 break-words">{dependenciesError}</p>
              <button
                type="button"
                onClick={onRetryDependencies}
                className="mt-2 font-medium underline underline-offset-2"
              >
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
                Your selected source and the context granted to a step may be
                sent to its configured model provider. WebSearch steps also send
                queries to an external search service. Report records and
                artifacts persist locally until removed.
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
              ? "Preparing report…"
              : configLoading
                ? "Loading workflow…"
                : !listenersReady
                  ? "Preparing event stream…"
                  : "Review report"}
            {!configLoading && !preparingRun && listenersReady && (
              <svg
                aria-hidden="true"
                className="h-4 w-4"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={1.7}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="m9 5 7 7-7 7"
                />
              </svg>
            )}
          </button>
        </div>
      </div>
    </SidebarPanel>
  );
}

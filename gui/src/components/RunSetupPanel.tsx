import PaperSelector from "./PaperSelector";
import ResizeHandle from "./ResizeHandle";
import WorkflowPanel from "./WorkflowPanel";

interface Props {
  configVersion: number;
  dependenciesLoading: boolean;
  inputMode: string;
  listenersReady: boolean;
  paperPath: string | null;
  selectionKey: number;
  width: number;
  onConfigureWorkflow: () => void;
  onGenerate: () => void;
  onPaperPathChange: (path: string | null) => void;
  onProfileChange: () => void;
  onResize: (width: number) => void;
}

export default function RunSetupPanel({
  configVersion,
  dependenciesLoading,
  inputMode,
  listenersReady,
  paperPath,
  selectionKey,
  width,
  onConfigureWorkflow,
  onGenerate,
  onPaperPathChange,
  onProfileChange,
  onResize,
}: Props) {
  const canRun =
    (paperPath !== null || inputMode === "none") &&
    !dependenciesLoading &&
    listenersReady;

  return (
    <aside
      data-testid="run-setup-panel"
      style={{ width }}
      className="relative flex shrink-0 flex-col border-r border-gray-200/80 bg-white
                 dark:border-gray-800 dark:bg-gray-900"
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-5 py-6">
        <header className="mb-6">
          <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-gray-400 dark:text-gray-500">
            Workspace
          </p>
          <h1 className="mt-1 text-xl font-semibold tracking-[-0.02em] text-gray-950 dark:text-gray-50">
            New run
          </h1>
          <p className="mt-1 text-xs leading-5 text-gray-500 dark:text-gray-400">
            Choose an input and the workflow to run.
          </p>
        </header>

        <div className="space-y-5">
          <PaperSelector
            key={selectionKey}
            onPathChange={onPaperPathChange}
            disabled={false}
          />
          {inputMode === "none" && (
            <p className="-mt-3 text-xs leading-5 text-gray-400 dark:text-gray-500">
              This workflow runs without a source document.
            </p>
          )}

          <WorkflowPanel
            disabled={false}
            editorOpen={false}
            onConfigure={onConfigureWorkflow}
            onProfileChange={onProfileChange}
            refreshKey={configVersion}
          />
        </div>

        <div className="mt-auto pt-6">
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
            {dependenciesLoading ? "Checking dependencies…" : "Run"}
            {!dependenciesLoading && (
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

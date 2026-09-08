// Manifest-driven Workflow artifact surface. Reads stay behind the run-owned client.
import { lazy, Suspense, useMemo } from "react";
import type { ArtifactExplorerProps } from "../lib/artifactTypes";
export type {
  ArtifactContent,
  ArtifactExplorerPreload,
  ArtifactSelectionRequest,
  ArtifactSelectionTarget,
  RunManifest,
} from "../lib/artifactTypes";
import { artifactFileAdapter } from "../lib/fileWorkspaceClient";
import RunMarkdown from "./file-workspace/RunMarkdown";
import ReportViewer from "./ReportViewer";
import ArtifactTree from "./artifact-explorer/ArtifactTree";
import { Viewer } from "./artifact-explorer/ArtifactViewer";
import useArtifactSelection from "./artifact-explorer/useArtifactSelection";
import { PAGE_SELECTION_PREFIX } from "./artifact-explorer/navigation";
const FileWorkspace = lazy(() => import("./file-workspace/FileWorkspace"));

export default function ArtifactExplorer({
  runId,
  fallbackMarkdown,
  deferInitialArtifact = false,
  preload = null,
  selectionRequest = null,
}: ArtifactExplorerProps) {
  const {
    fileTab,
    setFileTab,
    manifest,
    manifestError,
    retryManifest,
    selected,
    selectionHistory,
    content,
    loadError,
    navigationOpen,
    setNavigationOpen,
    artifactsByGroup,
    selectArtifact,
    goBack,
    entry,
  } = useArtifactSelection({
    runId,
    deferInitialArtifact,
    preload,
    selectionRequest,
  });
  const fileAdapter = useMemo(
    () =>
      artifactFileAdapter(
        runId,
        manifest?.artifacts.map((a) => a.rel_path) ?? [],
      ),
    [runId, manifest],
  );

  // Graceful degradation: no manifest → plain report view.
  if (manifestError) {
    return (
      <div className="h-full overflow-y-auto">
        <div
          role="alert"
          className="m-4 rounded-lg border border-red-200 dark:border-red-900 bg-red-50 dark:bg-red-950/30 p-3 text-sm text-red-700 dark:text-red-300"
        >
          <p className="font-medium">
            Could not load this run's artifact manifest.
          </p>
          <p className="mt-1 text-xs">{manifestError}</p>
          <button
            type="button"
            onClick={retryManifest}
            className="mt-2 rounded border border-red-300 dark:border-red-800 px-2 py-1 text-xs hover:bg-red-100 dark:hover:bg-red-900/40"
          >
            Retry
          </button>
        </div>
        {fallbackMarkdown && (
          <section aria-label="Fallback report">
            <p className="mx-4 text-xs text-gray-500">
              Showing the fallback report.
            </p>
            <ReportViewer markdown={fallbackMarkdown} />
          </section>
        )}
      </div>
    );
  }
  if (!manifest) {
    return (
      <div className="flex items-center justify-center h-full text-gray-400">
        <div className="animate-spin w-5 h-5 border-2 border-gray-300 border-t-gray-600 rounded-full" />
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0">
      <ArtifactTree
        navigationOpen={navigationOpen}
        setNavigationOpen={setNavigationOpen}
        manifest={manifest}
        artifactsByGroup={artifactsByGroup}
        selected={selected}
        selectArtifact={selectArtifact}
      />
      {/* Viewer */}
      <div className="flex min-w-0 flex-1 flex-col">
        {selected && !selected.startsWith(PAGE_SELECTION_PREFIX) && (
          <div className="file-toolbar">
            <button
              onClick={() => setFileTab(fileTab ? null : { path: selected })}
            >
              {fileTab ? "Return to artifact preview" : "Open in file tabs"}
            </button>
          </div>
        )}
        {fileTab && (
          <div className="min-h-0 flex-1">
            <Suspense fallback={<p>Loading files…</p>}>
              <FileWorkspace
                key={runId}
                adapter={fileAdapter}
                paths={manifest.artifacts.map((a) => a.rel_path)}
                initial={fileTab}
              />
            </Suspense>
          </div>
        )}
        <div className={fileTab ? "hidden" : "flex min-h-0 flex-1 flex-col"}>
          {content && (content.kind === "image" || content.kind === "pdf") && (
            <div
              className="flex shrink-0 items-center gap-3 border-b border-gray-200 bg-white px-4 py-2
                          dark:border-gray-800 dark:bg-gray-950"
            >
              <button
                type="button"
                onClick={goBack}
                disabled={selectionHistory.length === 0}
                className="inline-flex items-center gap-1.5 rounded px-2 py-1 text-xs font-medium text-gray-600
                         hover:bg-gray-100 hover:text-gray-900 disabled:cursor-default disabled:opacity-40
                         dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-white"
              >
                <svg
                  aria-hidden="true"
                  className="h-3.5 w-3.5"
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
                Back
              </button>
              <span className="min-w-0 truncate text-xs text-gray-500 dark:text-gray-400">
                {entry?.label ?? "Figure"}
              </span>
            </div>
          )}
          <div className="min-h-0 flex-1 overflow-auto">
            {loadError ? (
              <div className="p-6 text-sm text-gray-500 dark:text-gray-400">
                Could not read this artifact: {loadError}
              </div>
            ) : !selected ? (
              <div className="flex h-full items-center justify-center p-8 text-center text-sm text-gray-500 dark:text-gray-400">
                Select an artifact to preview it.
              </div>
            ) : content ? (
              <RunMarkdown
                runId={runId}
                path={selected}
                onOpen={(location) => setFileTab(location)}
              >
                <Viewer
                  runId={runId}
                  entry={entry}
                  content={content}
                  artifacts={manifest.artifacts}
                  pageArtifacts={manifest.page_artifacts}
                  onSelectArtifact={selectArtifact}
                />
              </RunMarkdown>
            ) : (
              <div className="flex items-center justify-center h-full text-gray-400">
                <div className="animate-spin w-5 h-5 border-2 border-gray-300 border-t-gray-600 rounded-full" />
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

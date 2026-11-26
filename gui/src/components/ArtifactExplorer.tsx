import { Icon } from "../ui/icons";
// Manifest-driven Workflow artifact surface. Reads stay behind the run-owned client.
import { lazy, Suspense, useCallback, useMemo } from "react";
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
import ReportIssueLink from "./ReportIssueLink";
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
    selectionNotice,
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
  // A choice in the file list or a preview shows that file, so it leaves the
  // file tabs, which would otherwise cover it.
  const showArtifact = useCallback(
    (next: string) => {
      setFileTab(null);
      selectArtifact(next);
    },
    [selectArtifact, setFileTab],
  );

  // Graceful degradation: no manifest → plain report view.
  if (manifestError) {
    return (
      <div className="h-full overflow-y-auto">
        <div
          role="alert"
          className="m-4 rounded-lg border border-danger bg-danger-soft p-3 text-sm text-danger"
        >
          <p className="font-medium">Could not load this review’s file list.</p>
          <p className="mt-1 text-xs">{manifestError}</p>
          <button
            type="button"
            onClick={retryManifest}
            className="mt-2 rounded border border-danger px-2 py-1 text-xs hover:bg-danger-soft"
          >
            Retry
          </button>
          <ReportIssueLink />
        </div>
        {fallbackMarkdown && (
          <section aria-label="Saved report text">
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
        selectArtifact={showArtifact}
      />
      {/* Viewer */}
      <div className="flex min-w-0 flex-1 flex-col">
        {selectionNotice && (
          <p
            role="status"
            className="shrink-0 border-b border-line bg-[var(--ui-warning-soft)] px-4 py-2 text-ui-label text-[var(--ui-warning)]"
          >
            {selectionNotice}
          </p>
        )}
        {(fileTab ||
          (selected && !selected.startsWith(PAGE_SELECTION_PREFIX))) && (
          <div className="file-toolbar">
            <button
              onClick={() => setFileTab(fileTab ? null : { path: selected })}
            >
              {fileTab ? "Return to file preview" : "Open in file tabs"}
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
            <div className="file-toolbar bg-surface">
              <button
                type="button"
                onClick={goBack}
                disabled={selectionHistory.length === 0}
                className="inline-flex items-center gap-1.5 font-medium"
              >
                <Icon name="chevron-left" className="h-3.5 w-3.5" />
                Back
              </button>
              <span className="min-w-0 truncate text-ui-meta text-ink-muted">
                {entry?.label ?? "Figure"}
              </span>
            </div>
          )}
          <div className="min-h-0 flex-1 overflow-auto">
            {loadError ? (
              <div className="p-6 text-sm text-gray-500 dark:text-gray-400">
                Could not read this file: {loadError}
              </div>
            ) : !selected ? (
              <div className="flex h-full items-center justify-center p-8 text-center text-sm text-gray-500 dark:text-gray-400">
                Select a file to preview it.
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
                  onSelectArtifact={showArtifact}
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

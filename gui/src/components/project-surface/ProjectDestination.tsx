import { lazy, Suspense, useMemo, useState, useEffect } from "react";
import {
  destinationHints,
  workspaceDestinations,
} from "../../lib/workspaceNavigation";
import { projectClient } from "../../lib/projectClient";
import {
  captureSourceContext,
  researchDraftInstruction,
} from "../../lib/researchContext";
import type { FilePassage } from "../file-workspace/FileWorkspace";
import { appEvents } from "../../lib/appEvents";
import { documentSourcePath } from "../../lib/project-overview/documentSource";
import {
  rememberWork,
  type RecentWork,
} from "../../lib/project-overview/continuation";
import type { OverviewAction } from "../../lib/project-overview/types";
import type { OpenResearchObject } from "../../lib/deskClient";
import type { FileLocation } from "../../lib/fileLinks";
import RetainedWorkspaceView from "../RetainedWorkspaceView";
import ResearchReaderBoundary from "../ResearchReaderBoundary";
import ObjectPane from "../research-desk/ObjectPane";
import ProjectOverview from "./ProjectOverview";
import ProjectSettings from "./ProjectSettings";
import OverviewOptions from "./OverviewOptions";
import { Icon } from "../../ui/icons";
import ProjectActionItems from "./ProjectActionItems";
import ProjectReviews from "./ProjectReviews";
import ProjectEdits from "./ProjectEdits";
import ProjectDocuments from "./ProjectDocuments";
import ProjectPage from "./ProjectPage";
import type { SurfaceApi, ProjectTab } from "./shared";
import type { ProjectReadingState } from "./useProjectReadingState";
import type { ProjectSelectionActions } from "./useProjectSelectionActions";
import type { WorkspaceProjectSurfaceProps } from "./types";
const ProjectFiles = lazy(() => import("../file-workspace/ProjectFiles"));
const VersionControl = lazy(() => import("../version-control/VersionControl"));
const ResearchStudio = lazy(() => import("../WorkspaceResearchStudio"));
const ResearchPanel = lazy(() => import("../WorkspaceResearchPanel"));

/** The research panel tab each panel-backed view shows. */
const panelTab = {
  memory: "memory",
  execution: "results",
  review: "release",
  exchange: "release",
} as const;

interface Props extends Pick<
  WorkspaceProjectSurfaceProps,
  | "sessions"
  | "onResumeSession"
  | "onStartConversation"
  | "onAllProjects"
  | "onConversation"
  | "onReviewHandoff"
  | "navigationInSidebar"
  | "snapshot"
  | "onSnapshot"
> {
  api: SurfaceApi;
  reading: ProjectReadingState;
  selectionActions: ProjectSelectionActions;
  busy: boolean;
  tab: ProjectTab;
  object: OpenResearchObject | null;
  setObject: (object: OpenResearchObject | null) => void;
  comparison: OpenResearchObject | null;
  setComparison: (object: OpenResearchObject | null) => void;
  fileLocation: FileLocation | null;
  refresh: () => Promise<void>;
  /** Reloads project data after a write in an embedded view. */
  onChanged: () => void;
  clearError: () => void;
  error?: string | null;
  watch: boolean;
  setWatch: (watch: boolean) => void;
  attachFolder: () => Promise<void>;
  importPaper: (folder?: boolean) => Promise<void>;
  addVersion: () => Promise<void>;
}

export default function ProjectDestination({
  api,
  reading,
  selectionActions,
  busy,
  object,
  setObject,
  comparison,
  setComparison,
  fileLocation,
  refresh,
  onChanged,
  clearError,
  error,
  watch,
  setWatch,
  attachFolder,
  importPaper,
  addVersion,
  sessions = [],
  onResumeSession,
  onStartConversation,
  onAllProjects,
  onConversation,
  onReviewHandoff,
  navigationInSidebar,
  snapshot,
  onSnapshot,
  tab,
}: Props) {
  const { workspaceId, data, workspace, run, reportError, setTab } = api;
  const [checkpointId, setCheckpointId] = useState<string>();
  const [reviewsBeside, setReviewsBeside] = useState(false);
  const [responseId, setResponseId] = useState<string>();
  const [initialScript, setInitialScript] = useState<{
    path: string;
    request: number;
  }>();
  const [executionTargetId, setExecutionTargetId] = useState<string>();
  /** The script whose run setup or output is open, to go back to. */
  const [runSource, setRunSource] = useState<string | null>(null);
  useEffect(() => {
    if (tab !== "execution") setRunSource(null);
  }, [tab]);
  const askSource = async (
    passage: FilePassage & { checkpointId?: string | null },
  ) => {
    const { path, hash, checkpointId, ...selection } = passage;
    const context = await captureSourceContext(workspaceId, {
      path,
      expectedHash: hash,
      checkpointId: checkpointId ?? null,
      selection,
    });
    await onConversation(
      `Please help with the selected passage in ${path}. ${researchDraftInstruction}`,
      undefined,
      context,
    );
  };
  const askFile = async (path: string) => {
    const context = await captureSourceContext(workspaceId, {
      path,
      expectedHash: null,
      checkpointId: null,
      selection: null,
    });
    await onConversation(
      `Please help with ${path}. ${researchDraftInstruction}`,
      undefined,
      context,
    );
  };
  const openTarget = (action: OverviewAction) => {
    setObject(null);
    if (action.kind === "file")
      appEvents.emit("open-file", { workspaceId, location: action.location });
    if (action.kind === "tab") {
      if (action.tab === "tasks" && action.targetId)
        api.setTaskId(action.targetId);
      if (action.tab === "edits") setCheckpointId(action.targetId);
      if (action.tab === "responses") setResponseId(action.targetId);
      if (action.tab === "execution") setExecutionTargetId(action.targetId);
      setTab(action.tab);
    }
  };
  // Stable across renders: the file tree re-lists the folder when this changes.
  const filePaths = useMemo(
    () =>
      data.inventory?.body.files.filter((f) => f.hash).map((f) => f.path) ?? [],
    [data.inventory],
  );
  const {
    revisionId,
    setRevisionId,
    pinnedRevision,
    setPinnedRevision,
    initialSelection,
    setInitialSelection,
    pinnedSelection,
  } = reading;
  const {
    selectionAction,
    setSelectionAction,
    selectionBody,
    setSelectionBody,
    selection,
    submitSelection,
    openAnchors,
  } = selectionActions;
  const [historicalOwner, setHistoricalOwner] = useState<{
    revisionId: string;
    paperId: string;
  } | null>(null);
  const selectedDocument =
    data.papers.find((p) => p.revision?.id === revisionId) ??
    data.papers.find(
      (p) =>
        historicalOwner?.revisionId === revisionId &&
        p.paper.id === historicalOwner.paperId,
    );
  useEffect(() => {
    if (object) return;
    if (
      (tab === "documents" || tab === "writing") &&
      revisionId &&
      selectedDocument
    )
      rememberWork(workspaceId, {
        id: `${tab}:${revisionId}`,
        title: selectedDocument.paper.title,
        tab,
        revisionId,
      });
  }, [workspaceId, tab, revisionId, selectedDocument?.paper.title, object]);
  const openDocument = (
    id: string,
    mode: "read" | "edit" | "compare" = "read",
  ) => {
    setObject(null);
    setRevisionId(id);
    setInitialSelection(undefined);
    if (mode === "compare") setPinnedRevision(id);
    setTab(mode === "edit" ? "writing" : "documents");
  };
  const openWork = async (work: RecentWork) => {
    if (work.revisionId) {
      // Check historical captures too; never silently substitute another version.
      await projectClient.read(workspaceId, work.revisionId);
      openDocument(work.revisionId, work.tab === "writing" ? "edit" : "read");
    } else if (work.location)
      openTarget({ kind: "file", label: "Open", location: work.location });
    else
      openTarget({
        kind: "tab",
        label: "Open",
        tab: work.tab,
        targetId: work.targetId,
      });
  };
  useEffect(() => {
    let live = true;
    if (revisionId && !selectedDocument)
      void projectClient
        .read(workspaceId, revisionId)
        .then((view) => {
          if (live)
            setHistoricalOwner({ revisionId, paperId: view.revision.paperId });
        })
        .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [workspaceId, revisionId, selectedDocument?.paper.id]);
  const documentPath = selectedDocument
    ? documentSourcePath(selectedDocument, workspace?.root, filePaths)
    : null;
  const studio = (view: "writing" | "responses") =>
    view === "writing" && revisionId && !selectedDocument ? (
      <p className="p-4 text-sm text-ink-muted">Loading document…</p>
    ) : (
      <Suspense fallback={<p>Loading research tools…</p>}>
        <ResearchStudio
          key={
            view === "writing"
              ? `${view}:${selectedDocument?.paper.id ?? "project"}`
              : view
          }
          documentId={
            view === "writing" ? selectedDocument?.paper.id : undefined
          }
          documentPath={view === "writing" ? documentPath : undefined}
          active={!object && tab === view}
          tool={view === "writing" ? "manuscript" : view}
          workspaceId={workspaceId}
          data={data}
          onRefresh={refresh}
          responseTargetId={responseId}
          onResponseOpen={(id, title) =>
            rememberWork(workspaceId, {
              id: `response:${id}`,
              tab: "responses",
              targetId: id,
              title,
            })
          }
          onAskSource={(passage) => void askSource(passage).catch(reportError)}
          onAsk={(prompt, context) =>
            onConversation(prompt, undefined, context)
          }
          onRelatedTarget={(tab, targetId) =>
            openTarget({
              kind: "tab",
              label: "Open linked work",
              tab,
              targetId,
            })
          }
          onDocument={(id, passage) => {
            setRevisionId(id);
            setInitialSelection(
              passage && {
                revisionId: id,
                revisionHash:
                  data.papers.find((p) => p.revision?.id === id)?.revision
                    ?.contentHash ?? "",
                start: passage.start,
                end: passage.end,
                page: null,
                region: null,
                quote: "",
              },
            );
            setTab("documents");
          }}
          onAnchors={openAnchors}
          onReviewHandoff={onReviewHandoff}
        />
      </Suspense>
    );
  return (
    <fieldset disabled={busy} className="contents">
      {object && (
        <div className="flex min-h-0 flex-1 gap-3 p-4">
          <ResearchReaderBoundary
            key={`object:${object.id}:${object.revision}`}
          >
            <ObjectPane
              workspaceId={workspaceId}
              object={object}
              onError={reportError}
              onClose={() => setObject(null)}
              onPin={() => setComparison(object)}
            />
          </ResearchReaderBoundary>
          {comparison && (
            <ResearchReaderBoundary
              key={`comparison:${comparison.id}:${comparison.revision}`}
            >
              <ObjectPane
                workspaceId={workspaceId}
                object={comparison}
                onError={reportError}
                onClose={() => setComparison(null)}
              />
            </ResearchReaderBoundary>
          )}
        </div>
      )}
      <RetainedWorkspaceView active={!object && tab === "overview"}>
        <div className="project-overview-host">
          <ProjectOverview
            busy={busy}
            key={workspaceId}
            {...api}
            error={error}
            onDismissError={clearError}
            options={(onClose) => (
              <OverviewOptions
                {...api}
                watch={watch}
                setWatch={setWatch}
                onImportPaper={(folder) => {
                  onClose();
                  void importPaper(folder);
                }}
                onAttachFolder={() => {
                  onClose();
                  void attachFolder();
                }}
              />
            )}
            sessions={sessions}
            active={!object && tab === "overview"}
            transcript={
              snapshot
                ? { sessionId: snapshot.session.id, items: snapshot.items }
                : null
            }
            onAsk={(prompt, context) =>
              void run(() => onConversation(prompt, undefined, context))
            }
            showProjectName={navigationInSidebar}
            onResume={(id) =>
              void run(() => onResumeSession?.(id) ?? Promise.resolve())
            }
            onStartConversation={() =>
              void run(() =>
                onStartConversation
                  ? onStartConversation()
                  : onConversation(""),
              )
            }
            onAllProjects={onAllProjects}
            onOpenDocument={openDocument}
            onOpenTarget={openTarget}
            onOpenWork={(work) => void openWork(work).catch(reportError)}
            onImportPaper={(folder) => void importPaper(folder)}
            onAttachFolder={() => void attachFolder()}
          />
        </div>
      </RetainedWorkspaceView>
      <RetainedWorkspaceView active={!object && tab === "tasks"}>
        <ProjectPage view="tasks">
          <ProjectActionItems {...api} />
        </ProjectPage>
      </RetainedWorkspaceView>
      <RetainedWorkspaceView active={!object && tab === "reviews"}>
        <ProjectPage view="reviews">
          {tab === "reviews" && (
            <ProjectReviews
              workspaceId={workspaceId}
              documents={data.papers}
              workspaceRoot={workspace?.root ?? null}
              workspaceName={workspace?.name ?? "This project"}
              onChanged={onChanged}
            />
          )}
        </ProjectPage>
      </RetainedWorkspaceView>
      <RetainedWorkspaceView active={!object && tab === "project-settings"}>
        <ProjectPage view="project-settings">
          <ProjectSettings
            {...api}
            watch={watch}
            setWatch={setWatch}
            onImportPaper={(folder) => void importPaper(folder)}
            onAttachFolder={() => void attachFolder()}
          />
        </ProjectPage>
      </RetainedWorkspaceView>
      <RetainedWorkspaceView active={!object && tab === "files"}>
        <div className="min-h-0 flex-1">
          <Suspense fallback={<p>Loading files…</p>}>
            <ProjectFiles
              onRunFile={(path) => {
                setInitialScript({ path, request: Date.now() });
                setRunSource(path);
                setTab("execution");
              }}
              onShowRun={(path, runId) => {
                setExecutionTargetId(runId);
                setRunSource(path);
                setTab("execution");
              }}
              workspaceId={workspaceId}
              paths={filePaths}
              initial={fileLocation}
              onNavigate={(location) => {
                if (tab === "files")
                  rememberWork(workspaceId, {
                    id: `file:${location.path}`,
                    title: location.path.split("/").at(-1) ?? location.path,
                    tab: "files",
                    location,
                  });
              }}
              onSaved={() => void refresh()}
              onAsk={(passage) => void askSource(passage).catch(reportError)}
              onAskFile={(path) => void askFile(path).catch(reportError)}
            />
          </Suspense>
        </div>
      </RetainedWorkspaceView>
      <RetainedWorkspaceView active={!object && tab === "version-control"}>
        {tab === "version-control" && (
          <div className="min-h-0 flex-1">
            <Suspense fallback={<p>Loading version control…</p>}>
              <VersionControl
                data={data}
                workspaceId={workspaceId}
                active={!object}
                onAttachFolder={() => void attachFolder()}
                onFilesChanged={() => void refresh()}
              />
            </Suspense>
          </div>
        )}
      </RetainedWorkspaceView>
      <RetainedWorkspaceView active={!object && tab === "documents"}>
        <ProjectDocuments
          {...api}
          revisionId={revisionId}
          setRevisionId={setRevisionId}
          pinnedRevision={pinnedRevision}
          setPinnedRevision={setPinnedRevision}
          initialSelection={initialSelection}
          setInitialSelection={setInitialSelection}
          pinnedSelection={pinnedSelection}
          selectionAction={selectionAction}
          selectionBody={selectionBody}
          setSelectionBody={setSelectionBody}
          onSelection={selection}
          onSubmitSelection={() => void submitSelection()}
          onCancelSelection={() => setSelectionAction(null)}
          onImport={() => void importPaper()}
          onAddVersion={() => void addVersion()}
        />
      </RetainedWorkspaceView>
      <RetainedWorkspaceView active={!object && tab === "edits"}>
        <ProjectPage view="edits">
          <ProjectEdits
            {...api}
            selectedCheckpointId={checkpointId}
            onConversation={onConversation}
          />
        </ProjectPage>
      </RetainedWorkspaceView>
      {(["writing", "responses"] as const).map((view) => (
        <RetainedWorkspaceView key={view} active={!object && tab === view}>
          {view === "writing" ? (
            <div className="project-writing-workspace">
              <div className="project-writing-main">
                <button
                  type="button"
                  className="file-action self-start"
                  aria-expanded={reviewsBeside}
                  onClick={() => setReviewsBeside(!reviewsBeside)}
                >
                  {reviewsBeside
                    ? "Hide review findings"
                    : "Review findings beside source"}
                </button>
                {studio(view)}
              </div>
              {reviewsBeside && (
                <aside
                  className="project-writing-reviews"
                  aria-label="Review findings beside source"
                >
                  <ProjectReviews
                    workspaceId={workspaceId}
                    documents={data.papers}
                    workspaceRoot={workspace?.root ?? null}
                    workspaceName={workspace?.name}
                    onChanged={onChanged}
                  />
                </aside>
              )}
            </div>
          ) : (
            <ProjectPage view={view}>{studio(view)}</ProjectPage>
          )}
        </RetainedWorkspaceView>
      ))}
      {(["memory", "execution", "review", "exchange"] as const).map((view) => (
        <RetainedWorkspaceView key={view} active={!object && tab === view}>
          <div className="project-panel">
            {view === "execution" && runSource && (
              <button
                type="button"
                className="project-run-back"
                onClick={() => setTab("files")}
              >
                <Icon name="arrow-left" className="h-4 w-4" />
                Back to {runSource.split("/").pop()}
              </button>
            )}
            <Suspense
              fallback={
                <section className="workspace-panel-loading">
                  Opening {workspaceDestinations[view].label}…
                </section>
              }
            >
              <ResearchPanel
                embedded
                hint={destinationHints[view]}
                active={!object && tab === view}
                title={workspaceDestinations[view].label}
                workspace={workspace}
                snapshot={
                  snapshot?.session.workspaceId === workspaceId
                    ? snapshot
                    : null
                }
                onSnapshot={(next) => onSnapshot?.(next)}
                onError={reportError}
                onChanged={onChanged}
                onActionStart={clearError}
                onClose={() => setTab("overview")}
                initialTab={panelTab[view]}
                allowedTabs={[panelTab[view]]}
                resultsView="all"
                initialScript={view === "execution" ? initialScript : undefined}
                executionTargetId={
                  view === "execution" ? executionTargetId : undefined
                }
                releaseView={view === "exchange" ? "share" : "review"}
                onReviewHandoff={onReviewHandoff}
              />
            </Suspense>
          </div>
        </RetainedWorkspaceView>
      ))}
    </fieldset>
  );
}

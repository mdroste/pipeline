import { lazy, Suspense } from "react";
import { workspaceDestinations } from "../../lib/workspaceNavigation";
import type { OpenResearchObject } from "../../lib/deskClient";
import type { FileLocation } from "../../lib/fileLinks";
import RetainedWorkspaceView from "../RetainedWorkspaceView";
import ResearchReaderBoundary from "../ResearchReaderBoundary";
import ObjectPane from "../research-desk/ObjectPane";
import ProjectOverview from "./ProjectOverview";
import ProjectSettings from "./ProjectSettings";
import ProjectActionItems from "./ProjectActionItems";
import ProjectReviews from "./ProjectReviews";
import ProjectEdits from "./ProjectEdits";
import ProjectDocuments from "./ProjectDocuments";
import type { SurfaceApi, ProjectTab } from "./shared";
import type { ProjectReadingState } from "./useProjectReadingState";
import type { ProjectSelectionActions } from "./useProjectSelectionActions";
import type { WorkspaceProjectSurfaceProps } from "./types";
const DeskSearch = lazy(() => import("../research-desk/Search"));
const DeskDiscovery = lazy(() => import("../research-desk/Discovery"));
const DeskData = lazy(() => import("../research-desk/Data"));
const DeskDecisions = lazy(() => import("../research-desk/Decisions"));
const DeskPlans = lazy(() => import("../research-desk/Plans"));
const ProgramExperiments = lazy(
  () => import("../research-programs/Experiments"),
);
const ProgramAssets = lazy(() => import("../research-programs/Assets"));
const ProgramTheory = lazy(() => import("../research-programs/Theory"));
const ProgramCampaigns = lazy(() => import("../research-programs/Campaigns"));
const ProgramDelivery = lazy(() => import("../research-programs/Delivery"));
const ProgramSharing = lazy(() => import("../research-programs/Sharing"));
const ProgramChecks = lazy(() => import("../research-programs/Checks"));
const ProjectFiles = lazy(() => import("../file-workspace/ProjectFiles"));
const ResearchStudio = lazy(() => import("../WorkspaceResearchStudio"));
const ResearchPanel = lazy(() => import("../WorkspaceResearchPanel"));

interface Props extends Pick<
  WorkspaceProjectSurfaceProps,
  | "sessionId"
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
  openObject: (object: OpenResearchObject) => void;
  refresh: () => Promise<void>;
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
  openObject,
  refresh,
  watch,
  setWatch,
  attachFolder,
  importPaper,
  addVersion,
  sessionId,
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
      {(["search", "acquisition", "data", "decisions", "plans"] as const).map(
        (view) => (
          <RetainedWorkspaceView key={view} active={!object && tab === view}>
            <div className="min-h-0 flex-1 overflow-auto p-5">
              <Suspense fallback={<p>Loading research tools…</p>}>
                {view === "search" && (
                  <DeskSearch
                    workspaceId={workspaceId}
                    sessionId={sessionId}
                    onOpen={openObject}
                    onError={reportError}
                  />
                )}{" "}
                {view === "acquisition" && (
                  <DeskDiscovery
                    workspaceId={workspaceId}
                    onOpen={openObject}
                    onError={reportError}
                  />
                )}{" "}
                {view === "data" && (
                  <DeskData
                    workspaceId={workspaceId}
                    onOpen={openObject}
                    onError={reportError}
                  />
                )}{" "}
                {view === "decisions" && (
                  <DeskDecisions
                    workspaceId={workspaceId}
                    sessionId={sessionId}
                    onOpen={openObject}
                    onError={reportError}
                    onRefresh={refresh}
                  />
                )}{" "}
                {view === "plans" && (
                  <DeskPlans
                    workspaceId={workspaceId}
                    onOpen={openObject}
                    onError={reportError}
                    active={!object && tab === view}
                  />
                )}
              </Suspense>
            </div>
          </RetainedWorkspaceView>
        ),
      )}
      {(
        [
          "grids",
          "assets",
          "symbols",
          "campaigns",
          "delivery",
          "sharing",
          "checks",
        ] as const
      ).map((view) => (
        <RetainedWorkspaceView key={view} active={!object && tab === view}>
          <div className="min-h-0 flex-1 overflow-auto p-5">
            <Suspense fallback={<p>Loading research tools…</p>}>
              {view === "grids" && (
                <ProgramExperiments
                  active={!object && tab === view}
                  workspaceId={workspaceId}
                  sessionId={sessionId}
                  onOpen={openObject}
                  onError={reportError}
                />
              )}
              {view === "assets" && (
                <ProgramAssets
                  workspaceId={workspaceId}
                  sessionId={sessionId}
                  onOpen={openObject}
                  onError={reportError}
                />
              )}
              {view === "symbols" && (
                <ProgramTheory
                  workspaceId={workspaceId}
                  sessionId={sessionId}
                  onOpen={openObject}
                  onError={reportError}
                />
              )}
              {view === "campaigns" && (
                <ProgramCampaigns
                  workspaceId={workspaceId}
                  sessionId={sessionId}
                  onOpen={openObject}
                  onError={reportError}
                  onReviewHandoff={onReviewHandoff}
                />
              )}
              {view === "delivery" && (
                <ProgramDelivery
                  workspaceId={workspaceId}
                  sessionId={sessionId}
                  onOpen={openObject}
                  onError={reportError}
                />
              )}
              {view === "sharing" && (
                <ProgramSharing
                  workspaceId={workspaceId}
                  sessionId={sessionId}
                  onOpen={openObject}
                  onError={reportError}
                />
              )}
              {view === "checks" && (
                <ProgramChecks
                  active={!object && tab === view}
                  workspaceId={workspaceId}
                  sessionId={sessionId}
                  onOpen={openObject}
                  onError={reportError}
                />
              )}
            </Suspense>
          </div>
        </RetainedWorkspaceView>
      ))}
      <RetainedWorkspaceView active={!object && tab === "overview"}>
        <div className="min-h-0 flex-1 overflow-auto bg-white p-6 dark:bg-neutral-950">
          <ProjectOverview
            {...api}
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
            onOpenDocument={(id) => {
              setRevisionId(id);
              setTab("documents");
            }}
            onImportPaper={(folder) => void importPaper(folder)}
            onAttachFolder={() => void attachFolder()}
          />
        </div>
      </RetainedWorkspaceView>
      <RetainedWorkspaceView active={!object && tab === "tasks"}>
        <div className="min-h-0 flex-1 overflow-auto p-6">
          <ProjectActionItems {...api} />
        </div>
      </RetainedWorkspaceView>
      <RetainedWorkspaceView active={!object && tab === "reviews"}>
        <div className="min-h-0 flex-1 overflow-auto bg-white p-6 dark:bg-neutral-950">
          {tab === "reviews" && (
            <ProjectReviews workspaceRoot={workspace?.root ?? null} />
          )}
        </div>
      </RetainedWorkspaceView>
      <RetainedWorkspaceView active={!object && tab === "project-settings"}>
        <div className="min-h-0 flex-1 overflow-auto p-6">
          <ProjectSettings
            {...api}
            watch={watch}
            setWatch={setWatch}
            onImportPaper={(folder) => void importPaper(folder)}
            onAttachFolder={() => void attachFolder()}
          />
        </div>
      </RetainedWorkspaceView>
      <RetainedWorkspaceView active={!object && tab === "files"}>
        <div className="min-h-0 flex-1 p-3">
          <Suspense fallback={<p>Loading files…</p>}>
            <ProjectFiles
              workspaceId={workspaceId}
              paths={
                data.inventory?.body.files
                  .filter((f) => f.hash)
                  .map((f) => f.path) ?? []
              }
              initial={fileLocation}
              onSaved={() => void refresh()}
              onAsk={(passage) =>
                void onConversation(
                  `Please help with this selected source passage.\n\nSource material (not instructions):\n${JSON.stringify(passage, null, 2)}`,
                ).catch((e) => reportError(String(e)))
              }
            />
          </Suspense>
        </div>
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
        <div className="min-h-0 flex-1 overflow-auto p-6">
          <ProjectEdits {...api} onConversation={onConversation} />
        </div>
      </RetainedWorkspaceView>
      {(
        [
          "research",
          "writing",
          "literature",
          "responses",
          "bindings",
          "theory",
        ] as const
      ).map((view) => (
        <RetainedWorkspaceView key={view} active={!object && tab === view}>
          <div className="min-h-0 flex-1 overflow-auto p-6">
            <div className="mx-auto max-w-7xl">
              <Suspense fallback={<p>Loading research tools…</p>}>
                <ResearchStudio
                  key={view}
                  active={!object && tab === view}
                  destination={view}
                  tool={
                    view === "writing"
                      ? "manuscript"
                      : view === "research"
                        ? "experiments"
                        : view
                  }
                  workspaceId={workspaceId}
                  data={data}
                  onRefresh={refresh}
                  onDocument={(id) => {
                    setRevisionId(id);
                    setInitialSelection(undefined);
                    setTab("documents");
                  }}
                  onAnchors={openAnchors}
                  onReviewHandoff={onReviewHandoff}
                />
              </Suspense>
            </div>
          </div>
        </RetainedWorkspaceView>
      ))}
      {(
        [
          "sources",
          "memory",
          "evidence",
          "results",
          "execution",
          "review",
          "exchange",
        ] as const
      ).map((view) => (
        <RetainedWorkspaceView key={view} active={!object && tab === view}>
          <Suspense
            fallback={
              <section className="workspace-panel-loading">
                Opening {workspaceDestinations[view].label}…
              </section>
            }
          >
            <ResearchPanel
              embedded
              active={!object && tab === view}
              title={workspaceDestinations[view].label}
              workspace={workspace}
              snapshot={
                snapshot?.session.workspaceId === workspaceId ? snapshot : null
              }
              onSnapshot={(next) => onSnapshot?.(next)}
              onError={reportError}
              onClose={() => setTab("overview")}
              initialTab={
                view === "sources"
                  ? "documents"
                  : view === "execution"
                    ? "results"
                    : view === "review" || view === "exchange"
                      ? "release"
                      : view
              }
              allowedTabs={[
                view === "sources"
                  ? "documents"
                  : view === "execution"
                    ? "results"
                    : view === "review" || view === "exchange"
                      ? "release"
                      : view,
              ]}
              resultsView={view === "execution" ? "profiles" : "runs"}
              releaseView={view === "exchange" ? "share" : "review"}
              onReviewHandoff={onReviewHandoff}
            />
          </Suspense>
        </RetainedWorkspaceView>
      ))}
    </fieldset>
  );
}

import {
  destinationsInSection,
  isWorkspaceDestination,
  workspaceDestinations,
  workspaceSections,
} from "../lib/workspaceNavigation";
import RetainedWorkspaceView from "./RetainedWorkspaceView";
import { loadDeskLayout, saveDeskLayout } from "../lib/deskLayout";
import { type ContextItem, type OpenResearchObject } from "../lib/deskClient";
import ResearchReaderBoundary from "./ResearchReaderBoundary";
import ObjectPane from "./research-desk/ObjectPane";
const DeskSearch = lazy(() => import("./research-desk/Search"));
const DeskDiscovery = lazy(() => import("./research-desk/Discovery"));
const DeskData = lazy(() => import("./research-desk/Data"));
const DeskDecisions = lazy(() => import("./research-desk/Decisions"));
const DeskPlans = lazy(() => import("./research-desk/Plans"));
const ProgramExperiments = lazy(
  () => import("./research-programs/Experiments"),
);
const ProgramAssets = lazy(() => import("./research-programs/Assets"));
const ProgramTheory = lazy(() => import("./research-programs/Theory"));
const ProgramCampaigns = lazy(() => import("./research-programs/Campaigns"));
const ProgramDelivery = lazy(() => import("./research-programs/Delivery"));
const ProgramSharing = lazy(() => import("./research-programs/Sharing"));
const ProgramChecks = lazy(() => import("./research-programs/Checks"));
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import {
  projectClient,
  type Annotation,
  type DocumentSelection,
  type HomeSettings,
  type ProjectAction,
  type ProjectHome,
  type ProjectRecord,
  type ResearchTask,
} from "../lib/projectClient";
import type {
  Workspace,
  ReviewHandoff,
  ConversationSnapshot,
} from "../lib/workbenchTypes";
import ProjectOverview from "./project-surface/ProjectOverview";
import ProjectSettings from "./project-surface/ProjectSettings";
import ProjectActionItems from "./project-surface/ProjectActionItems";
import type { WorkbenchSession } from "../lib/workbenchTypes";
import ProjectEdits from "./project-surface/ProjectEdits";
import ProjectDocuments, {
  type SelectionActionKind,
} from "./project-surface/ProjectDocuments";
import {
  button,
  folderName,
  muted,
  op,
  type ProjectTab,
  type SurfaceApi,
} from "./project-surface/shared";
const ProjectFiles = lazy(() => import("./file-workspace/ProjectFiles"));
const ResearchStudio = lazy(() => import("./WorkspaceResearchStudio"));
const ResearchPanel = lazy(() => import("./WorkspaceResearchPanel"));

const PAPER_EXTENSIONS = [
  "pdf",
  "tex",
  "md",
  "txt",
  "docx",
  "bib",
  "py",
  "r",
  "jl",
  "do",
  "json",
  "csv",
  "tsv",
];

export default function WorkspaceProjectSurface({
  workspaceId,
  sessionId,
  sessions = [],
  onResumeSession,
  onStartConversation,
  onAllProjects,
  onConversation,
  onWorkspaceChanged,
  onReviewHandoff,
  destination,
  onDestination,
  navigationInSidebar = false,
  snapshot = null,
  onSnapshot,
}: {
  onReviewHandoff?: (handoff: ReviewHandoff) => void;
  destination?: ProjectTab;
  onDestination?: (tab: ProjectTab) => void;
  navigationInSidebar?: boolean;
  snapshot?: ConversationSnapshot | null;
  onSnapshot?: (snapshot: ConversationSnapshot) => void;
  workspaceId: string;
  sessionId?: string | null;
  sessions?: WorkbenchSession[];
  onResumeSession?: (id: string) => Promise<void>;
  onStartConversation?: () => Promise<void>;
  onAllProjects?: () => void;
  onConversation: (
    text: string,
    checkpoint?: string,
    contextItems?: ContextItem[],
  ) => Promise<void>;
  onWorkspaceChanged: (workspace: Workspace) => void;
}) {
  const [data, setData] = useState<ProjectHome | null>(null);
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [layout] = useState(() => loadDeskLayout(workspaceId));
  const [tab, setLocalTab] = useState<ProjectTab>(
    destination ?? (layout.tab as ProjectTab),
  );
  const setTab = useCallback(
    (next: ProjectTab) => {
      setLocalTab(next);
      onDestination?.(next);
    },
    [onDestination],
  );
  const [object, setObject] = useState<OpenResearchObject | null>(
    layout.object,
  );
  useEffect(() => {
    const open = (event: Event) => {
      const d = (event as CustomEvent<{ workspaceId: string; tab: string }>)
        .detail;
      if (d.workspaceId === workspaceId && isWorkspaceDestination(d.tab)) {
        setObject(null);
        setTab(d.tab as ProjectTab);
      }
    };
    window.addEventListener("pipeline:research-destination", open);
    return () =>
      window.removeEventListener("pipeline:research-destination", open);
  }, [workspaceId]);
  useEffect(() => {
    if (destination && destination !== tab) {
      setLocalTab(destination);
      setObject(null);
    }
  }, [destination]);
  const [fileLocation, setFileLocation] = useState<
    import("../lib/fileLinks").FileLocation | null
  >(() => {
    try {
      return JSON.parse(
        sessionStorage.getItem(`pipeline.openFile.${workspaceId}`) ?? "null",
      );
    } catch {
      return null;
    }
  });
  useEffect(() => {
    const open = (event: Event) => {
      const detail = (
        event as CustomEvent<{
          workspaceId: string;
          location: import("../lib/fileLinks").FileLocation;
        }>
      ).detail;
      if (detail.workspaceId === workspaceId) {
        setFileLocation(detail.location);
        setObject(null);
        setTab("files");
      }
    };
    window.addEventListener("pipeline:open-file", open);
    try {
      if (sessionStorage.getItem(`pipeline.openFile.${workspaceId}`)) {
        setObject(null);
        setTab("files");
        sessionStorage.removeItem(`pipeline.openFile.${workspaceId}`);
      }
    } catch {
      /* Navigation storage is optional. */
    }
    return () => window.removeEventListener("pipeline:open-file", open);
  }, [workspaceId]);
  const [comparison, setComparison] = useState<OpenResearchObject | null>(
    layout.comparison,
  );
  const deskRoot = useRef<HTMLElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [taskId, setTaskId] = useState("");
  const [watch, setWatch] = useState(false);
  const [revisionId, setRevisionId] = useState<string | null>(
    layout.revisionId,
  );
  const [pinnedRevision, setPinnedRevision] = useState<string | null>(() => {
    try {
      return (
        layout.pinnedRevision ??
        localStorage.getItem(`pipeline.project.${workspaceId}.pinned`)
      );
    } catch {
      return null;
    }
  });
  useEffect(() => {
    try {
      if (pinnedRevision)
        localStorage.setItem(
          `pipeline.project.${workspaceId}.pinned`,
          pinnedRevision,
        );
      else localStorage.removeItem(`pipeline.project.${workspaceId}.pinned`);
    } catch {
      /* Layout storage is optional. */
    }
  }, [workspaceId, pinnedRevision]);
  useEffect(
    () =>
      saveDeskLayout(workspaceId, {
        tab,
        revisionId,
        pinnedRevision,
        object,
        comparison,
        assistantWidth: layout.assistantWidth,
      }),
    [
      workspaceId,
      tab,
      revisionId,
      pinnedRevision,
      object,
      comparison,
      layout.assistantWidth,
    ],
  );
  const [initialSelection, setInitialSelection] = useState<
    DocumentSelection | undefined
  >();
  const [pinnedSelection, setPinnedSelection] = useState<
    DocumentSelection | undefined
  >();
  const [selectionAction, setSelectionAction] = useState<{
    selection: DocumentSelection;
    action: SelectionActionKind;
  } | null>(null);
  const [selectionBody, setSelectionBody] = useState("");

  const reportError = useCallback((message: string) => setError(message), []);
  const refresh = useCallback(async () => {
    const [home, ws] = await Promise.all([
      projectClient.home(workspaceId),
      workbenchClient.getWorkspace(workspaceId),
    ]);
    setData(home);
    setWorkspace(ws);
    setRevisionId(
      (current) =>
        current ??
        home.settings.body.manuscriptRevisionId ??
        home.papers[0]?.revision?.id ??
        null,
    );
  }, [workspaceId]);
  useEffect(() => {
    void refresh().catch((e) => reportError(workbenchErrorMessage(e)));
  }, [refresh, reportError]);
  const run = useCallback(
    async (operation: () => Promise<unknown>) => {
      if (busyRef.current) return;
      busyRef.current = true;
      setBusy(true);
      setError(null);
      try {
        await operation();
      } catch (e) {
        setError(workbenchErrorMessage(e));
      } finally {
        try {
          await refresh();
        } catch (e) {
          setError(workbenchErrorMessage(e));
        }
        busyRef.current = false;
        setBusy(false);
      }
    },
    [refresh],
  );
  const act = useCallback(
    async (action: ProjectAction) =>
      run(() => projectClient.mutate(workspaceId, action)),
    [run, workspaceId],
  );
  const saveSettings = useCallback(
    (patch: Partial<HomeSettings>) =>
      data
        ? act({
            action: "saveHome",
            expectedRevision: data.settings.revision,
            settings: { ...data.settings.body, ...patch },
          })
        : Promise.resolve(),
    [act, data],
  );

  // Bounded polling is optional; a focus refresh handles the usual external-editor
  // round trip. Neither mechanism changes captured document revisions or selection.
  useEffect(() => {
    if (!workspace?.root) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const focus = () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        if (!busyRef.current) void act({ action: "refresh" });
      }, 400);
    };
    window.addEventListener("focus", focus);
    const interval = watch
      ? setInterval(() => {
          if (document.visibilityState === "visible") focus();
        }, 30_000)
      : undefined;
    return () => {
      window.removeEventListener("focus", focus);
      clearTimeout(timer);
      if (interval) clearInterval(interval);
    };
  }, [act, watch, workspace?.root]);

  const attachFolder = () =>
    run(async () => {
      if (!workspace) return;
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected !== "string") return;
      const result = await workbenchClient.registerWorkspaceRoot({
        workspaceId,
        root: selected,
        expectedRevision: workspace.revision,
        operationId: op(),
      });
      onWorkspaceChanged(result.record);
      await projectClient.mutate(workspaceId, { action: "refresh" });
    });
  const importPaper = (folder = false) =>
    run(async () => {
      const selected = await open({
        multiple: false,
        directory: folder,
        filters: folder
          ? undefined
          : [{ name: "Research document", extensions: PAPER_EXTENSIONS }],
      });
      if (typeof selected !== "string") return;
      const result = await workbenchClient.importPaper({
        workspaceId,
        paperId: null,
        title: folderName(selected),
        role: "manuscript",
        path: selected,
        operationId: op(),
      });
      setRevisionId(result.revision?.id ?? null);
      setInitialSelection(undefined);
      setTab("documents");
    });
  const addVersion = () =>
    run(async () => {
      const paper = data?.papers.find((p) => p.revision?.id === revisionId);
      if (!paper) return;
      const path = await open({
        multiple: false,
        directory: paper.revision?.inputKind === "source_tree",
      });
      if (typeof path !== "string") return;
      const imported = await workbenchClient.importPaper({
        workspaceId,
        paperId: paper.paper.id,
        title: paper.paper.title,
        role: paper.paper.role,
        path,
        operationId: op(),
      });
      setRevisionId(imported.revision?.id ?? null);
      setInitialSelection(undefined);
    });
  const openAnnotation = useCallback(
    (annotation: ProjectRecord<Annotation>) => {
      setRevisionId(annotation.body.selection.revisionId);
      setInitialSelection(annotation.body.selection);
      setTab("documents");
    },
    [],
  );
  const openAnchors = (primary: string, secondary: string | null) => {
    const first = data?.anchors.find((a) => a.id === primary);
    if (!first) {
      reportError("The linked passage is no longer in this project.");
      return;
    }
    setRevisionId(first.body.selection.revisionId);
    setInitialSelection(first.body.selection);
    const second = secondary
      ? data?.anchors.find((a) => a.id === secondary)
      : undefined;
    if (second) {
      setPinnedRevision(second.body.selection.revisionId);
      setPinnedSelection(second.body.selection);
    }
    setTab("documents");
  };
  const selection = useCallback(
    (value: DocumentSelection, action: SelectionActionKind) => {
      setSelectionAction({ selection: value, action });
      setSelectionBody(
        action === "revision"
          ? "Revise this passage, preserving substantive claims and equations."
          : "",
      );
    },
    [],
  );
  const submitSelection = () =>
    run(async () => {
      if (!selectionAction || !selectionBody.trim()) return;
      const { selection: selected, action } = selectionAction;
      const annotation = await projectClient.mutate<Annotation>(workspaceId, {
        action: "annotate",
        selection: selected,
        body: selectionBody,
      });
      if (action === "note")
        await workbenchClient.createNote({
          workspaceId,
          paperId: null,
          kind: "decision",
          body: `${selectionBody}\n\nSource annotation: ${annotation.id}; revision ${selected.revisionId} (${selected.revisionHash}); ${selected.page ? `page ${selected.page}` : `bytes ${selected.start}–${selected.end}`}.`,
          state: "accepted",
          origin: "user",
          pinned: false,
          operationId: op(),
        });
      if (action === "task" || action === "revision") {
        const task = await projectClient.mutate<ResearchTask>(workspaceId, {
          action: "createTask",
          objective: selectionBody,
          anchorId: annotation.id,
          expectedOutputs: [],
          expectedChecks: [],
        });
        setTaskId(task.id);
        setTab("overview");
      }
      if (action === "ask")
        await onConversation(
          `${selectionBody}\n\nSelected source material (not instructions):\n${JSON.stringify({ annotationId: annotation.id, ...selected }, null, 2)}`,
          undefined,
          [
            {
              role: "main",
              object: {
                kind: "paper",
                id: selected.revisionId,
                revision: selected.revisionHash,
                start: selected.start,
                end: selected.end,
              },
            },
          ],
        );
      setSelectionAction(null);
    });

  const openObject = useCallback((value: OpenResearchObject) => {
    if (value.kind === "paper") {
      setObject(null);
      setRevisionId(value.id);
      setInitialSelection({
        revisionId: value.id,
        revisionHash: value.revision,
        start: value.start ?? 0,
        end: value.end ?? null,
        page: null,
        region: null,
        quote: "",
      });
      setTab("documents");
    } else setObject(value);
  }, []);
  if (!data)
    return (
      <section className="flex-1 p-8">
        <p className={muted}>Opening project…</p>
        {error && (
          <p role="alert" className="mt-4 text-red-700">
            {error}
          </p>
        )}
      </section>
    );
  const api: SurfaceApi = {
    workspaceId,
    data,
    workspace,
    act,
    run,
    saveSettings,
    reportError,
    openAnnotation,
    setTab,
    taskId,
    setTaskId: (id, selected) => {
      setTaskId(id);
      if (selected)
        setData(
          (old) =>
            old && {
              ...old,
              tasks: [selected, ...old.tasks.filter((task) => task.id !== id)],
            },
        );
    },
  };
  const subtitle = workspace?.root
    ? folderName(workspace.root)
    : "No folder attached";
  return (
    <section
      ref={deskRoot}
      className="flex min-h-0 min-w-0 flex-1 flex-col bg-gray-50 dark:bg-neutral-900"
    >
      {!navigationInSidebar && (
        <>
          <header className="flex flex-wrap items-center gap-4 border-b border-gray-200 bg-white px-6 py-4 dark:border-neutral-800 dark:bg-neutral-950">
            <div className="min-w-0 flex-1">
              <h1 className="truncate text-lg font-semibold">
                {workspace?.name ?? "Project"}
              </h1>
              <p
                className={`truncate ${muted}`}
                title={workspace?.root ?? undefined}
              >
                {subtitle}
              </p>
            </div>
            {!navigationInSidebar && (
              <nav
                aria-label="Project sections"
                className="flex flex-wrap gap-2"
              >
                {workspaceSections.map((section) => (
                  <button
                    key={section.id}
                    type="button"
                    className={button}
                    aria-current={
                      workspaceDestinations[tab].section === section.id
                        ? "page"
                        : undefined
                    }
                    onClick={() => {
                      setObject(null);
                      setTab(section.destination);
                    }}
                  >
                    {section.label}
                  </button>
                ))}
              </nav>
            )}
          </header>
          <div className="workspace-project-tool-bar">
            <label>
              View
              <select
                aria-label="Project view"
                value={tab}
                onChange={(event) => {
                  setObject(null);
                  setTab(event.target.value as ProjectTab);
                }}
              >
                {destinationsInSection(tab).map((id) => (
                  <option key={id} value={id}>
                    {workspaceDestinations[id].label}
                  </option>
                ))}
              </select>
            </label>
            <span className="flex-1" />
            <button
              type="button"
              className={button}
              onClick={() => {
                setObject(null);
                setTab("exchange");
              }}
            >
              Share project
            </button>
          </div>
        </>
      )}
      {error && (
        <div
          role="alert"
          className="m-4 rounded-md border border-red-300 bg-red-50 p-3 text-sm text-red-800 dark:bg-red-950/30 dark:text-red-200"
        >
          {error}
        </div>
      )}
      <fieldset disabled={busy} className="contents">
        {object && (
          <div className="flex min-h-0 flex-1 gap-3 p-4">
            <ResearchReaderBoundary key={object.id + object.revision}>
              <ObjectPane
                workspaceId={workspaceId}
                object={object}
                onError={reportError}
                onClose={() => setObject(null)}
                onPin={() => setComparison(object)}
              />
            </ResearchReaderBoundary>
            {comparison && (
              <ResearchReaderBoundary key={comparison.id + comparison.revision}>
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
                  snapshot?.session.workspaceId === workspaceId
                    ? snapshot
                    : null
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
      {busy && (
        <div
          role="status"
          className={`border-t border-gray-200 bg-white px-6 py-2 dark:border-neutral-800 dark:bg-neutral-950 ${muted}`}
        >
          Saving…
        </div>
      )}
    </section>
  );
}

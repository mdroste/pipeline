import {
  destinationsInSection,
  isWorkspaceDestination,
  workspaceDestinations,
  workspaceSections,
} from "../lib/workspaceNavigation";
import { loadDeskLayout, saveDeskLayout } from "../lib/deskLayout";
import { appEvents } from "../lib/appEvents";
import { router, type RouterSnapshot } from "../lib/router";
import type { OpenResearchObject } from "../lib/deskClient";
import { memo, useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { workspaceClient } from "../lib/workbench/workspaceClient";
import { researchClient } from "../lib/workbench/researchClient";
import { projectClient } from "../lib/projectClient";
import {
  briefNotes,
  isThinProject,
  projectSessions,
} from "../lib/projectOverview";
import {
  button,
  folderName,
  muted,
  op,
  type ProjectTab,
  type SurfaceApi,
} from "./project-surface/shared";
import { useProjectReadingState } from "./project-surface/useProjectReadingState";
import { useProjectSurfaceData } from "./project-surface/useProjectSurfaceData";
import { useProjectSelectionActions } from "./project-surface/useProjectSelectionActions";
import ProjectDestination from "./project-surface/ProjectDestination";
import ProjectSubnav from "./project-surface/ProjectSubnav";
import type { WorkspaceProjectSurfaceProps } from "./project-surface/types";

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

function WorkspaceProjectSurface({
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
  onConversationFirst,
  onProjectBrief,
}: WorkspaceProjectSurfaceProps) {
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
    return appEvents.on("workspace-destination", (d) => {
      if (d.workspaceId === workspaceId && isWorkspaceDestination(d.tab)) {
        setObject(null);
        setTab(d.tab as ProjectTab);
      }
    });
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
    const off = appEvents.on("open-file", (detail) => {
      if (detail.workspaceId === workspaceId) {
        setFileLocation(detail.location);
        setObject(null);
        setTab("files");
      }
    });
    try {
      if (sessionStorage.getItem(`pipeline.openFile.${workspaceId}`)) {
        setObject(null);
        setTab("files");
        sessionStorage.removeItem(`pipeline.openFile.${workspaceId}`);
      }
    } catch {
      /* Navigation storage is optional. */
    }
    return off;
  }, [workspaceId]);
  // A route may name a file, so a link or notification can land on it. It
  // applies once: later route changes must not pull the researcher back here.
  const followedFile = useRef("");
  useEffect(() => {
    const follow = ({ route }: RouterSnapshot) => {
      if (
        route.page !== "workspace" ||
        !route.file ||
        (route.projectId && route.projectId !== workspaceId)
      )
        return;
      const target = `${route.file}#${route.filePage ?? ""}`;
      if (target === followedFile.current) return;
      followedFile.current = target;
      setFileLocation({ path: route.file, page: route.filePage });
      setObject(null);
      setTab("files");
    };
    follow(router.getSnapshot());
    return router.subscribe(follow);
  }, [workspaceId]);
  const [comparison, setComparison] = useState<OpenResearchObject | null>(
    layout.comparison,
  );
  const deskRoot = useRef<HTMLElement>(null);
  const [taskId, setTaskId] = useState("");
  const reading = useProjectReadingState(workspaceId, layout, setTab);
  const {
    revisionId,
    setRevisionId,
    pinnedRevision,
    setInitialSelection,
    openAnnotation,
  } = reading;
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
  const dataModel = useProjectSurfaceData(workspaceId, setRevisionId);
  const {
    data,
    setData,
    workspace,
    error,
    busy,
    watch,
    setWatch,
    reportError,
    clearError,
    refresh,
    changed,
    run,
    act,
    saveSettings,
  } = dataModel;
  // A failure belongs to the view it happened in.
  useEffect(() => clearError(), [clearError, tab, object]);

  const attachFolder = () =>
    run(async () => {
      if (!workspace) return;
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected !== "string") return;
      const result = await workspaceClient.registerWorkspaceRoot({
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
      const result = await researchClient.importPaper({
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
  const paperOfRevision = async (id: string) => {
    const { revision } = await projectClient.read(workspaceId, id);
    const paper = data?.papers.find((p) => p.paper.id === revision.paperId);
    if (!paper)
      throw new Error(
        "This version's paper is no longer in the project, so it can't take a new version.",
      );
    return paper;
  };
  const addVersion = () =>
    run(async () => {
      if (!revisionId) return;
      // Each paper lists only its latest version, and an earlier one can be open.
      const paper =
        data?.papers.find((p) => p.revision?.id === revisionId) ??
        (await paperOfRevision(revisionId));
      const path = await open({
        multiple: false,
        directory: paper.revision?.inputKind === "source_tree",
      });
      if (typeof path !== "string") return;
      const imported = await researchClient.importPaper({
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
  const selectionActions = useProjectSelectionActions({
    workspaceId,
    data,
    run,
    reportError,
    reading,
    setTaskId,
    setTab,
    onConversation,
  });

  // A project with no folder or paper has nothing to derive, so it opens on
  // its latest conversation rather than on an overview with nothing to say.
  const thin = data ? isThinProject(data, workspace) : false;
  const hasConversation = projectSessions(sessions, workspaceId).length > 0;
  const entryTab = useRef(tab);
  const conversationFirst = useRef(false);
  useEffect(() => {
    if (conversationFirst.current || !thin || !hasConversation) return;
    conversationFirst.current = true;
    if (entryTab.current === "overview" && tab === "overview")
      onConversationFirst?.();
  }, [thin, hasConversation]);
  const thinBrief = data && thin ? (briefNotes(data)[0]?.body ?? null) : null;
  useEffect(() => {
    onProjectBrief?.(thinBrief);
  }, [thinBrief]);

  const openObject = useCallback((value: OpenResearchObject) => {
    if (value.kind === "paper") {
      setObject(null);
      setRevisionId(value.id);
      setInitialSelection({
        revisionId: value.id,
        revisionHash: value.revision,
        start: value.start ?? null,
        end: value.end ?? null,
        page: null,
        region: null,
        quote: "",
      });
      setTab("documents");
    } else setObject(value);
  }, []);
  useEffect(
    () =>
      appEvents.on("open-object", (detail) => {
        if (detail.workspaceId === workspaceId) openObject(detail.object);
      }),
    [workspaceId, openObject],
  );
  if (!data)
    return (
      <section className="flex-1 p-8">
        <p className={muted}>
          {error ? "This project could not be opened." : "Opening project…"}
        </p>
        {error && (
          <>
            <p role="alert" className="mt-4 text-danger">
              {error}
            </p>
            <button
              type="button"
              className={`${button} mt-3`}
              onClick={() => {
                clearError();
                void refresh().catch(reportError);
              }}
            >
              Try again
            </button>
          </>
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
      className="flex min-h-0 min-w-0 flex-1 flex-col bg-surface"
    >
      {/* Standalone chrome for direct-rendered surfaces (fixtures, tests).
          Inside the app, WorkspacePage owns navigation in its sidebar. */}
      {!navigationInSidebar && (
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
          <nav aria-label="Project sections" className="flex flex-wrap gap-2">
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
          <label className={muted}>
            Project view{" "}
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
        </header>
      )}
      {error && (
        <div
          role="alert"
          className="m-4 flex items-start gap-3 rounded-md border border-red-300 bg-red-50 p-3 text-sm text-red-800 dark:bg-red-950/30 dark:text-red-200"
        >
          <span className="min-w-0 flex-1">{error}</span>
          <button
            type="button"
            className="shrink-0 underline"
            aria-label="Dismiss this error"
            onClick={clearError}
          >
            Dismiss
          </button>
        </div>
      )}
      {/* Every destination in the section is at most two clicks away here, so
          the palette is an accelerator, not the only map. */}
      {!object &&
        workspaceDestinations[tab].section !== "overview" &&
        destinationsInSection(tab).length > 1 && (
          <ProjectSubnav tab={tab} onSelect={setTab} />
        )}
      <ProjectDestination
        api={api}
        reading={reading}
        selectionActions={selectionActions}
        busy={busy}
        object={object}
        setObject={setObject}
        comparison={comparison}
        setComparison={setComparison}
        fileLocation={fileLocation}
        openObject={openObject}
        refresh={refresh}
        onChanged={changed}
        clearError={clearError}
        watch={watch}
        setWatch={setWatch}
        attachFolder={attachFolder}
        importPaper={importPaper}
        addVersion={addVersion}
        sessionId={sessionId}
        sessions={sessions}
        onResumeSession={onResumeSession}
        onStartConversation={onStartConversation}
        onAllProjects={onAllProjects}
        onConversation={onConversation}
        onReviewHandoff={onReviewHandoff}
        navigationInSidebar={navigationInSidebar}
        snapshot={snapshot}
        onSnapshot={onSnapshot}
        tab={tab}
      />
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

export default memo(WorkspaceProjectSurface);

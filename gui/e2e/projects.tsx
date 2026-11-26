// Development fixture for the real project index and overview. All IPC is
// replaced before mounting; sample records never enter the user's research store.
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import ProjectIndexPage from "../src/components/ProjectIndexPage";
import WorkspaceProjectSurface from "../src/components/WorkspaceProjectSurface";
import NavRail from "../src/components/NavRail";
import { overviewFixture } from "./overviewFixture";
import type { ProjectHome } from "../src/lib/projectClient";
import type { ProjectIndexItem } from "../src/lib/projectIndex";
import type {
  WorkbenchSession,
  Workspace,
} from "../src/lib/workbench/sessionTypes";
import type { WorkspaceDestination } from "../src/lib/workspaceNavigation";
import "../src/App.css";
import "../src/components/WorkspaceDesk.css";
import "../src/components/WorkspaceNavigation.css";
import "../src/components/WorkspaceInspector.css";

const time = "2026-09-13T15:00:00Z";
const workspace: Workspace = {
  id: "project-preview",
  name: "Production networks",
  root: null,
  rootIdentity: null,
  settingsRevision: 0,
  revision: 1,
  archivedAt: null,
  missingRootAt: null,
  createdAt: time,
  updatedAt: time,
};
const session: WorkbenchSession = {
  id: "actual-conversation",
  workspaceId: workspace.id,
  paperId: null,
  title: "Revising the aggregation argument",
  presetId: null,
  overrides: {},
  draft: "Keep the existing assumptions explicit.",
  revision: 1,
  archivedAt: null,
  createdAt: time,
  updatedAt: time,
};
const home: ProjectHome = {
  settings: {
    id: "home",
    workspaceId: workspace.id,
    kind: "home",
    revision: 1,
    updatedAt: time,
    body: {
      manuscriptRevisionId: null,
      baselineExecutionId: null,
      briefNoteIds: ["question"],
      excludedNoteIds: [],
      ignoredPaths: [],
      layout: "",
    },
  },
  notes: [
    {
      id: "question",
      workspaceId: workspace.id,
      paperId: null,
      kind: "question",
      body: "How does input substitution change the aggregate effects of sectoral shocks?",
      state: "accepted",
      origin: "user",
      pinned: true,
      revision: 1,
      createdAt: time,
      updatedAt: time,
    },
    {
      id: "proposal",
      workspaceId: workspace.id,
      paperId: null,
      kind: "next_step",
      body: "Make the assumptions behind Proposition 2 explicit before revising the proof.",
      state: "proposed",
      origin: "assistant",
      pinned: false,
      revision: 1,
      createdAt: time,
      updatedAt: time,
    },
  ],
  tasks: [
    {
      id: "task",
      workspaceId: workspace.id,
      kind: "task",
      revision: 1,
      updatedAt: time,
      body: {
        objective: "Check the assumptions behind Proposition 2",
        status: "open",
        anchorId: null,
        expectedChecks: [],
        expectedOutputs: [],
      },
    },
  ],
  papers: [],
  executions: [],
  anchors: [],
  applications: [],
  changes: [],
  inventory: null,
  noteHistory: [],
  ledger: {
    claims: [],
    evidence: [],
    staleClaims: [],
    unsupportedAcceptedClaims: [],
  },
  contextPreview: "Fixture context",
  workingCopyStatus: "No folder attached",
  fileAcceptance: true,
};
const projects: ProjectIndexItem[] = [
  {
    id: workspace.id,
    name: workspace.name,
    root: null,
    missingRootAt: null,
    updatedAt: time,
    brief: {
      id: "question",
      title: home.notes[0].body,
      kind: "question",
      updatedAt: time,
    },
    conversation: {
      id: session.id,
      title: session.title,
      kind: "conversation",
      updatedAt: time,
    },
    activity: {
      id: session.id,
      title: session.title,
      kind: "conversation",
      updatedAt: time,
    },
    nextTask: null,
    proposedNotes: 1,
    interruptedEdits: 0,
  },
  {
    id: "fiscal-preview",
    name: "Fiscal transmission",
    root: null,
    missingRootAt: null,
    updatedAt: time,
    brief: {
      id: "fiscal-question",
      title:
        "How do household balance sheets shape the transmission of fiscal transfers?",
      kind: "question",
      updatedAt: time,
    },
    conversation: null,
    activity: null,
    nextTask: {
      id: "fiscal-task",
      title: "Check sensitivity to the borrowing constraint",
      kind: "task",
      updatedAt: time,
    },
    proposedNotes: 0,
    interruptedEdits: 0,
  },
];
const sessions = [
  session,
  {
    ...session,
    id: "standalone-chat",
    workspaceId: null,
    title: "Thinking through an identification strategy",
  },
  {
    ...session,
    id: "fiscal-chat",
    workspaceId: "fiscal-preview",
    title: "Comparing the transfer experiments",
  },
  {
    ...session,
    id: "archived-chat",
    workspaceId: null,
    title: "An earlier research question",
    archivedAt: time,
  },
];
const overviewPreview = new URLSearchParams(location.search).has("overview");
const documentPreview =
  overviewPreview || new URLSearchParams(location.search).has("documents");
const documentHistory = new Map<
  string,
  NonNullable<ProjectHome["papers"][number]["revision"]>[]
>();
if (documentPreview) {
  home.papers = [
    ["paper", "Main paper", "paper"],
    ["seminar", "Seminar presentation", "slides"],
    ["conference", "Conference presentation", "slides"],
    ["appendix", "Online appendix", "appendix"],
  ].map(([id, title, kind]) => {
    const revision = {
      id: `${id}-v2`,
      paperId: id,
      inputKind: "text",
      entrypoint: `/research/${id}.md`,
      dependencyManifest: {},
      contentHash: `fixture-${id}`,
      textReference: null,
      compiledArtifactId: null,
      extraction: { status: "complete" },
      captureComplete: true,
      capturedAt: "2026-11-16T12:00:00Z",
    };
    documentHistory.set(id, [
      revision,
      { ...revision, id: `${id}-v1`, capturedAt: "2026-11-01T12:00:00Z" },
    ]);
    return {
      paper: {
        id,
        workspaceId: workspace.id,
        title,
        role:
          kind === "paper"
            ? "manuscript"
            : kind === "appendix"
              ? "appendix"
              : "other",
        documentType: kind as "paper" | "slides" | "appendix",
        pinned: id === "seminar",
        archived: false,
        currentRevisionId: revision.id,
        createdAt: time,
        updatedAt: time,
      },
      revision,
    };
  });
}
const overviewCommand = overviewPreview
  ? overviewFixture(home, workspace, session)
  : null;
Object.assign(window, {
  __TAURI_INTERNALS__: {
    transformCallback: () => 1,
    unregisterCallback: () => {},
    invoke: async (command: string, args: any) => {
      const overviewResult = overviewCommand?.(command, args);
      if (overviewResult !== undefined) return overviewResult;
      if (command.startsWith("plugin:event|")) return 1;
      if (command === "workbench_project_index") return projects;
      if (command === "workbench_list_workspaces")
        return {
          workspaces: projects.map((project) => ({ ...workspace, ...project })),
        };
      if (command === "workbench_conversation_index")
        return {
          items: sessions
            .filter((chat) => Boolean(chat.archivedAt) === args.archived)
            .map((session) => ({
              session,
              projectName:
                projects.find((p) => p.id === session.workspaceId)?.name ??
                null,
            })),
          nextCursor: null,
        };
      if (command === "workbench_list_sessions")
        return {
          sessions: sessions.filter(
            (chat) =>
              chat.workspaceId === args.workspaceId &&
              (args.includeArchived || !chat.archivedAt),
          ),
        };
      if (command === "workbench_session_snapshot")
        return {
          session: sessions.find((chat) => chat.id === args.sessionId),
        };
      if (command === "workbench_move_session") {
        const chat = sessions.find(
          (chat) => chat.id === args.request.sessionId,
        )!;
        chat.workspaceId = args.request.workspaceId;
        chat.revision++;
        return { record: chat };
      }
      if (command === "workbench_document_versions")
        return documentHistory.get(args.paperId) ?? [];
      if (command === "workbench_document_read") {
        const revision = [...documentHistory.values()]
          .flat()
          .find((revision) => revision.id === args.revisionId)!;
        const item = home.papers.find(
          (document) => document.paper.id === revision.paperId,
        )!;
        const text = `${item.paper.title}\n\nHow does input substitution change the aggregate effects of sectoral shocks?\n\n1. Motivation and research question\n2. Model and identification\n3. Main results\n4. Implications`;
        return {
          revision,
          title: item.paper.title,
          text,
          start: 0,
          end: text.length,
          totalBytes: text.length,
          pageCount: null,
          imageUrl: null,
          page: null,
        };
      }
      if (command === "workbench_update_document") {
        const item = home.papers.find(
          (document) => document.paper.id === args.request.paperId,
        )!;
        Object.assign(item.paper, args.request, {
          updatedAt: new Date().toISOString(),
        });
        if (args.request.currentRevisionId)
          item.revision = documentHistory
            .get(item.paper.id)!
            .find((version) => version.id === args.request.currentRevisionId)!;
        return item;
      }
      if (command === "workbench_project_home") return home;
      if (command === "workbench_get_workspace") return workspace;
      if (command === "workbench_create_note") {
        const note = {
          ...args.request,
          id: crypto.randomUUID(),
          revision: 1,
          createdAt: time,
          updatedAt: time,
        };
        home.notes.push(note);
        return note;
      }
      if (command === "workbench_update_note") {
        const note = home.notes.find((n) => n.id === args.request.noteId)!;
        Object.assign(note, args.request);
        note.revision++;
        return note;
      }
      if (
        command === "workbench_project_mutate" &&
        args.request.action === "saveHome"
      ) {
        home.settings.body = args.request.settings;
        home.settings.revision++;
        return home.settings;
      }
      throw new Error(`Unavailable in the fixture: ${command}`);
    },
  },
});

function Fixture() {
  const [view, setView] = useState<"index" | "project" | "conversation">(
    documentPreview ? "project" : "index",
  );
  const [destination, setDestination] = useState<WorkspaceDestination>(
    documentPreview && !overviewPreview ? "documents" : "overview",
  );
  const [width, setWidth] = useState(1400);
  const [dark, setDark] = useState(false);
  const [resumed, setResumed] = useState("");
  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
  }, [dark]);
  const open = async (_id: string, tab: WorkspaceDestination = "overview") => {
    setDestination(tab);
    setView("project");
  };
  const resume = async (id: string) => {
    setResumed(id);
    setView("conversation");
  };
  return (
    <div
      style={
        overviewPreview
          ? { height: "100dvh", display: "flex", flexDirection: "column" }
          : undefined
      }
    >
      <div className="flex flex-wrap items-center gap-5 p-3 text-xs dark:bg-neutral-900 dark:text-neutral-100">
        <span>
          Development fixture · sample research · no native/model calls
        </span>
        <label>
          Window width{" "}
          <select
            aria-label="Test window width"
            value={width}
            onChange={(e) => setWidth(Number(e.target.value))}
          >
            {[1024, 1280, 1366, 1400, 1440, 1600, 1920].map((n) => (
              <option key={n}>{n}</option>
            ))}
          </select>
        </label>
        <label>
          <input
            type="checkbox"
            checked={dark}
            onChange={(e) => setDark(e.target.checked)}
          />{" "}
          Dark appearance
        </label>
      </div>
      <div
        data-testid="project-fixture-window"
        style={{
          width,
          maxWidth: "100%",
          height: overviewPreview ? undefined : 780,
          flex: overviewPreview ? 1 : undefined,
          minHeight: 0,
          display: "flex",
        }}
      >
        <NavRail
          activePage={view === "index" ? "project-index" : "workspace"}
          width={216}
          onResize={() => {}}
          hasCurrentRun={false}
          dependenciesReady={true}
          dependenciesLoading={false}
          onNewRun={() => {}}
          onNavigate={() => setView("index")}
          onDependencies={() => {}}
          recentProjects={projects}
          onOpenProject={(id) => void open(id)}
          onOpenConversation={(id) => void resume(id)}
          onNewConversation={() => void resume("new-unfiled")}
        />
        <main className="min-w-0 flex-1 overflow-auto bg-white dark:bg-neutral-950 dark:text-neutral-100">
          {view === "index" ? (
            <ProjectIndexPage
              onOpenProject={open}
              onResumeConversation={resume}
              onNewConversation={() => void resume("new-unfiled")}
            />
          ) : view === "project" ? (
            <div className="workspace-chat-shell flex h-full">
              <WorkspaceProjectSurface
                navigationInSidebar={overviewPreview}
                workspaceId={workspace.id}
                sessions={[session]}
                onConversation={async () => {}}
                onResumeSession={resume}
                onStartConversation={async () => {
                  setResumed("new");
                  setView("conversation");
                }}
                onAllProjects={() => setView("index")}
                destination={destination}
                onDestination={setDestination}
                onWorkspaceChanged={() => {}}
              />
            </div>
          ) : (
            <div className="space-y-5 p-8">
              <button onClick={() => setView("index")}>← All projects</button>
              <h1 className="text-xl">{session.title}</h1>
              <p>Resumed conversation: {resumed}</p>
              <textarea
                aria-label="Saved conversation draft"
                className="w-full rounded border bg-transparent p-3"
                defaultValue={session.draft}
              />
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
createRoot(document.getElementById("root")!).render(<Fixture />);

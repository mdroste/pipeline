// Development fixture for the real project index and overview. All IPC is
// replaced before mounting; sample records never enter the user's research store.
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import ProjectIndexPage from "../src/components/ProjectIndexPage";
import WorkspaceProjectSurface from "../src/components/WorkspaceProjectSurface";
import NavRail from "../src/components/NavRail";
import type { ProjectHome } from "../src/lib/projectClient";
import type { ProjectIndexItem } from "../src/lib/projectIndex";
import type { WorkbenchSession, Workspace } from "../src/lib/workbenchTypes";
import type { WorkspaceDestination } from "../src/lib/workspaceNavigation";
import "../src/App.css";
import "../src/components/WorkspaceConversation.css";

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
Object.assign(window, {
  __TAURI_INTERNALS__: {
    transformCallback: () => 1,
    unregisterCallback: () => {},
    invoke: async (command: string, args: any) => {
      if (command.startsWith("plugin:event|")) return 1;
      if (command === "workbench_project_index") return projects;
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
    "index",
  );
  const [destination, setDestination] =
    useState<WorkspaceDestination>("overview");
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
    <>
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
            {[1024, 1400, 1600].map((n) => (
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
      <div style={{ width, maxWidth: "100%", height: 780, display: "flex" }}>
        <NavRail
          activePage={view === "index" ? "project-index" : "workspace"}
          width={216}
          onResize={() => {}}
          hasCurrentRun={false}
          runInProgress={false}
          isMac={false}
          dependenciesReady={true}
          dependenciesLoading={false}
          onNewRun={() => {}}
          onNavigate={() => setView("index")}
          onDependencies={() => {}}
          recentProjects={projects}
          onOpenProject={(id) => void open(id)}
        />
        <main className="min-w-0 flex-1 overflow-auto bg-white dark:bg-neutral-950 dark:text-neutral-100">
          {view === "index" ? (
            <ProjectIndexPage
              onOpenProject={open}
              onResumeConversation={resume}
            />
          ) : view === "project" ? (
            <div className="workspace-chat-shell flex h-full">
              <WorkspaceProjectSurface
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
    </>
  );
}
createRoot(document.getElementById("root")!).render(<Fixture />);

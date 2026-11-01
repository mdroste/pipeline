import { useEffect, useId, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import useConversationIndex from "../hooks/useConversationIndex";
import useModalDialog from "../hooks/useModalDialog";
import { reviewHistoryClient } from "../lib/reviewHistoryClient";
import { router, type AppRoute } from "../lib/router";
import { taskClient } from "../lib/taskClient";
import type { TaskSummary } from "../lib/taskTypes";
import type { RunSummary } from "../lib/types";
import { workspaceClient } from "../lib/workbench/workspaceClient";
import type { Workspace } from "../lib/workbenchTypes";
import {
  workspaceDestinations,
  workspaceSections,
  type WorkspaceDestination,
} from "../lib/workspaceNavigation";
import { useNavRailSlot } from "./nav-rail/navRailSlot";
import "./WorkspaceNavigation.css";

const SHOWN_RESULTS = 40;

interface SearchEntry {
  id: string;
  label: string;
  hint: string;
  run: () => void;
}

const goTo = (route: AppRoute) => () => void router.navigate(route);

const PAGES: SearchEntry[] = (
  [
    ["Home", { page: "home" }],
    ["Conversations", { page: "conversations" }],
    ["Projects", { page: "project-index" }],
    ["New review", { page: "main" }],
    ["Review history", { page: "history" }],
    ["Review designer", { page: "pipeline" }],
    ["Automations", { page: "tasks" }],
    ["Activity", { page: "activity" }],
    ["Settings", { page: "settings" }],
    ["Help", { page: "help" }],
  ] as const
).map(([label, route]) => ({
  id: `page:${route.page}`,
  label,
  hint: "Go to",
  run: goTo(route),
}));

/** Reads what each source can offer; one failed source leaves the others. */
function useSearchSources() {
  const [projects, setProjects] = useState<Workspace[]>([]);
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  useEffect(() => {
    let live = true;
    void workspaceClient
      .listWorkspaces(false)
      .then(({ workspaces }) => {
        if (live)
          setProjects(
            [...workspaces].sort((a, b) =>
              b.updatedAt.localeCompare(a.updatedAt),
            ),
          );
      })
      .catch(() => undefined);
    void reviewHistoryClient
      .list()
      .then((listed) => {
        if (live) setRuns(listed);
      })
      .catch(() => undefined);
    void taskClient
      .list()
      .then((listed) => {
        if (live) setTasks(listed);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);
  return { projects, runs, tasks };
}

/** Find any project, conversation, review, automation, or project view. */
export default function AppSearch({
  onClose,
  onOpenConversation,
  onOpenRun,
}: {
  onClose: () => void;
  onOpenConversation: (sessionId: string) => void;
  onOpenRun: (runId: string) => void;
}) {
  const root = useModalDialog<HTMLDivElement>(onClose, true, true);
  const title = useId();
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const { projects, runs, tasks } = useSearchSources();
  const { conversations } = useConversationIndex(true, false, true);
  const workspace = useNavRailSlot()?.workspace;
  const viewProject = workspace?.projectId ?? projects[0]?.id ?? null;
  const entries = useMemo<SearchEntry[]>(() => {
    const views = viewProject
      ? (Object.keys(workspaceDestinations) as WorkspaceDestination[]).map(
          (id) => ({
            id: `view:${id}`,
            label: workspaceDestinations[id].label,
            hint: `Project view · ${
              workspaceSections.find(
                (section) => section.id === workspaceDestinations[id].section,
              )!.label
            }`,
            run:
              workspace?.projectId === viewProject
                ? () => workspace.openView(id)
                : goTo({
                    page: "workspace",
                    projectId: viewProject,
                    surface: "project",
                    destination: id,
                  }),
          }),
        )
      : [];
    return [
      ...projects.map((project) => ({
        id: `project:${project.id}`,
        label: project.name,
        hint: "Project",
        run: goTo({
          page: "workspace",
          projectId: project.id,
          surface: "project",
          destination: "overview",
        }),
      })),
      ...conversations.map(({ session, projectName }) => ({
        id: `conversation:${session.id}`,
        label: session.title,
        hint: projectName ? `Conversation · ${projectName}` : "Conversation",
        run: () => onOpenConversation(session.id),
      })),
      ...PAGES,
      ...views,
      ...runs.map((run) => ({
        id: `review:${run.run_id}`,
        label: run.input_name,
        hint: `Review · ${run.profile_name}`,
        run: () => onOpenRun(run.run_id),
      })),
      ...tasks.map((task) => ({
        id: `automation:${task.id}`,
        label: task.name,
        hint: "Automation",
        run: goTo({ page: "tasks", taskId: task.id }),
      })),
    ];
  }, [
    conversations,
    onOpenConversation,
    onOpenRun,
    projects,
    runs,
    tasks,
    viewProject,
    workspace,
  ]);
  const matches = useMemo(() => {
    const terms = query.toLocaleLowerCase().trim().split(/\s+/);
    return entries
      .filter((entry) =>
        terms.every((term) =>
          `${entry.label} ${entry.hint}`.toLocaleLowerCase().includes(term),
        ),
      )
      .slice(0, SHOWN_RESULTS);
  }, [entries, query]);
  const selected = Math.min(index, matches.length - 1);
  useEffect(() => {
    root.current
      ?.querySelector<HTMLElement>('[data-highlighted="true"]')
      ?.scrollIntoView?.({ block: "nearest" });
  }, [selected]);
  const choose = (entry: SearchEntry) => {
    onClose();
    entry.run();
  };
  return createPortal(
    <div
      className="workspace-picker-backdrop"
      onPointerDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={root}
        role="dialog"
        aria-modal="true"
        aria-labelledby={title}
        className="workspace-tool-picker"
        tabIndex={-1}
      >
        <div className="workspace-picker-heading">
          <h2 id={title}>Search</h2>
          <button type="button" aria-label="Close search" onClick={onClose}>
            ×
          </button>
        </div>
        <input
          data-autofocus
          type="search"
          role="combobox"
          aria-expanded="true"
          aria-autocomplete="list"
          aria-label="Search Pipeline"
          placeholder="Find a project, conversation, review, or view…"
          value={query}
          aria-controls={`${title}-results`}
          aria-activedescendant={
            selected >= 0 ? `${title}-${matches[selected].id}` : undefined
          }
          onChange={(event) => {
            setQuery(event.target.value);
            setIndex(0);
          }}
          onKeyDown={(event) => {
            // WebKit reports the Enter that confirms an IME composition with
            // keyCode 229 and isComposing already false.
            if (event.nativeEvent.isComposing || event.keyCode === 229) return;
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              setIndex((value) =>
                Math.max(
                  0,
                  Math.min(
                    matches.length - 1,
                    value + (event.key === "ArrowDown" ? 1 : -1),
                  ),
                ),
              );
            }
            if (event.key === "Enter" && matches[selected]) {
              event.preventDefault();
              choose(matches[selected]);
            }
          }}
        />
        <div
          id={`${title}-results`}
          role="listbox"
          aria-label="Search results"
          className="workspace-picker-results"
        >
          {matches.map((entry, row) => (
            <button
              type="button"
              role="option"
              aria-selected={row === selected}
              tabIndex={-1}
              id={`${title}-${entry.id}`}
              key={entry.id}
              data-highlighted={row === selected}
              onClick={() => choose(entry)}
            >
              <span>{entry.label}</span>
              <small>{entry.hint}</small>
            </button>
          ))}
          {!matches.length && <p>Nothing matches “{query}”.</p>}
        </div>
        <footer>↑ ↓ to browse · Enter to open · Esc to close</footer>
      </div>
    </div>,
    document.body,
  );
}

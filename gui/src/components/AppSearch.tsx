import { Fragment, useEffect, useId, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import useConversationIndex from "../hooks/useConversationIndex";
import useModalDialog from "../hooks/useModalDialog";
import useSearchSources from "../hooks/useSearchSources";
import { router, type AppRoute } from "../lib/router";
import {
  openDestinations,
  workspaceDestinations,
  workspaceSections,
} from "../lib/workspaceNavigation";
import { useNavRailSlot } from "./nav-rail/navRailSlot";
import "./WorkspaceNavigation.css";

const GROUP_LIMIT = 6;

interface SearchEntry {
  id: string;
  label: string;
  hint: string;
  group: string;
  current?: boolean;
  run: () => void;
}

const goTo = (route: AppRoute) => () => void router.navigate(route);

const PAGES: SearchEntry[] = (
  [
    ["Home", { page: "home" }],
    ["Projects", { page: "project-index" }],
    ["New review", { page: "main" }],
    ["Review history", { page: "history" }],
    ["Review designer", { page: "pipeline" }],
    ["All automations", { page: "tasks" }],
    ["All activity", { page: "activity" }],
    ["Settings", { page: "settings" }],
    ["Help", { page: "help" }],
  ] as const
).map(([label, route]) => ({
  id: `page:${route.page}`,
  group: "Pages",
  label,
  hint: route.page === "project-index" ? "Projects and chats" : "Go to",
  run: goTo(route),
}));

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
  const { projects, runs, tasks, states, retry } = useSearchSources();
  const chats = useConversationIndex(true, false, true);
  const { conversations } = chats;
  const [expanded, setExpanded] = useState<string[]>([]);
  const workspace = useNavRailSlot()?.workspace;
  const viewProject = workspace?.projectId ?? projects[0]?.id ?? null;
  const projectName =
    workspace?.projectName ?? projects.find((p) => p.id === viewProject)?.name;
  const entries = useMemo<SearchEntry[]>(() => {
    const views =
      viewProject && projectName
        ? openDestinations.map((id) => ({
            id: `view:${id}`,
            label: `${workspaceDestinations[id].label} · ${projectName}`,
            group: "Project tools",
            current: !!workspace?.projectId,
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
          }))
        : [];
    return [
      ...projects.map((project) => ({
        id: `project:${project.id}`,
        group: "Projects",
        current: project.id === workspace?.projectId,
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
        group: "Chats",
        current:
          !!workspace?.projectId && session.workspaceId === workspace.projectId,
        label: session.title,
        hint: projectName ? `Conversation · ${projectName}` : "Conversation",
        run: () => onOpenConversation(session.id),
      })),
      ...PAGES,
      ...views,
      ...runs.map((run) => ({
        id: `review:${run.run_id}`,
        group: "Reviews",
        label: run.title || run.input_name,
        hint: `Review · ${run.profile_name}`,
        run: () => onOpenRun(run.run_id),
      })),
      ...tasks.map((task) => ({
        id: `automation:${task.id}`,
        group: "Automations",
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
    projectName,
    workspace,
  ]);
  const groups = useMemo(() => {
    const needle = query.toLocaleLowerCase().trim();
    const terms = needle.split(/\s+/);
    const score = (entry: SearchEntry) =>
      (entry.label.toLocaleLowerCase() === needle
        ? 100
        : entry.label.toLocaleLowerCase().startsWith(needle)
          ? 40
          : 0) + (entry.current ? 10 : 0);
    const filtered = entries
      .filter((entry) =>
        terms.every((term) =>
          (entry.label + " " + entry.hint).toLocaleLowerCase().includes(term),
        ),
      )
      .sort((a, b) => score(b) - score(a));
    const byGroup = new Map<string, SearchEntry[]>();
    for (const entry of filtered)
      byGroup.set(entry.group, [...(byGroup.get(entry.group) ?? []), entry]);
    return [...byGroup].map(([label, items]) => ({
      label,
      items,
      visible: expanded.includes(label) ? items : items.slice(0, GROUP_LIMIT),
    }));
  }, [entries, query, expanded]);
  const matches = groups.flatMap((group) => group.visible);
  const loading = chats.loading || Object.values(states).includes("loading");
  const failures = [
    ...Object.entries(states)
      .filter(([, state]) => state === "error")
      .map(([name]) => name),
    ...(chats.error ? ["Chats"] : []),
  ];
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
          placeholder="Find a project, chat, review, or tool…"
          value={query}
          aria-controls={`${title}-results`}
          aria-activedescendant={
            selected >= 0 ? `${title}-${matches[selected].id}` : undefined
          }
          onChange={(event) => {
            setQuery(event.target.value);
            setIndex(0);
            setExpanded([]);
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
        <p className="px-4 pb-2 text-ui-meta text-ink-muted">
          Searches names and titles. To search document text, open its project.
        </p>
        {loading && (
          <p role="status" className="px-4 pb-2 text-ui-label">
            Loading saved work…
          </p>
        )}
        {failures.length > 0 && (
          <div role="status" className="px-4 pb-2 text-ui-label">
            Couldn't load {failures.join(", ")}. Results may be incomplete.{" "}
            <button
              className="underline"
              onClick={() => {
                retry();
                chats.refresh();
              }}
            >
              Retry search
            </button>
          </div>
        )}
        <div
          id={`${title}-results`}
          role="listbox"
          aria-label="Search results"
          className="workspace-picker-results"
        >
          {matches.map((entry, row) => (
            <Fragment key={entry.id}>
              {matches[row - 1]?.group !== entry.group && (
                <div
                  role="presentation"
                  className="px-3 pt-3 pb-1 text-ui-label font-semibold"
                >
                  {entry.group}
                </div>
              )}
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
            </Fragment>
          ))}
          {!matches.length && !loading && !failures.length && (
            <p>Nothing matches “{query}”.</p>
          )}
        </div>
        <div className="flex flex-wrap gap-2 px-4 py-2">
          {groups
            .filter((g) => g.items.length > g.visible.length)
            .map((g) => (
              <button
                key={g.label}
                className="text-ui-label underline"
                onClick={() => setExpanded((old) => [...old, g.label])}
              >
                Show all {g.items.length} {g.label.toLowerCase()}
              </button>
            ))}
        </div>
        <footer>↑ ↓ to browse · Enter to open · Esc to close</footer>
      </div>
    </div>,
    document.body,
  );
}

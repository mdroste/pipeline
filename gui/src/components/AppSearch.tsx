import IconButton from "../ui/IconButton";
import { Icon } from "../ui/icons";
import { Fragment, useEffect, useId, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import useConversationIndex from "../hooks/useConversationIndex";
import useModalDialog from "../hooks/useModalDialog";
import useSearchSources from "../hooks/useSearchSources";
import useResearchSearch from "../hooks/useResearchSearch";
import useFullTextSearch from "../hooks/useFullTextSearch";
import { searchExcerpt } from "../lib/researchSearch";
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
  text?: string;
  preview?: string;
  matchedQuery?: string;
  projectId?: string | null;
  run: () => void;
}

const goTo = (route: AppRoute) => () => void router.navigate(route);
const contentTarget = (route: AppRoute) =>
  route.page === "workspace"
    ? JSON.stringify([route.projectId, route.revisionId, route.file])
    : null;

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
  group: "Actions",
  label,
  hint:
    route.page === "main"
      ? "Open setup · no review starts"
      : "Navigate · all projects",
  text:
    route.page === "project-index"
      ? "chats conversations"
      : route.page === "pipeline"
        ? "workflow profile review design"
        : "",
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
  const {
    projects,
    runs,
    tasks,
    states,
    retry,
    taskNotices,
    moreTasks,
    loadMoreTasks,
  } = useSearchSources();
  const chats = useConversationIndex(true, false, query, false, true);
  const { conversations } = chats;
  const [expanded, setExpanded] = useState<string[]>([]);
  const workspace = useNavRailSlot()?.workspace;
  const [scope, setScope] = useState(workspace?.projectId ? "project" : "all");
  const [kind, setKind] = useState("All types");
  const scopeId = scope === "project" ? (workspace?.projectId ?? null) : null;
  const research = useResearchSearch(projects, runs, scopeId);
  const fullText = useFullTextSearch(
    research.scopedProjects,
    research.documents,
    query,
    kind,
    research.contentLoading,
  );
  const notices = [...research.notices, ...fullText.notices, ...taskNotices];
  const contentCount = `${fullText.documents} ${fullText.documents === 1 ? "document" : "documents"} and ${fullText.files} ${fullText.files === 1 ? "file" : "files"}`;
  const viewProject = workspace?.projectId ?? projects[0]?.id ?? null;
  const projectName =
    workspace?.projectName ?? projects.find((p) => p.id === viewProject)?.name;
  const entries = useMemo<SearchEntry[]>(() => {
    const contentTargets = new Set(
      fullText.entries.map((entry) => contentTarget(entry.route)),
    );
    const views =
      viewProject && projectName
        ? openDestinations.map((id) => ({
            id: `view:${id}`,
            label: `${workspaceDestinations[id].label} · ${projectName}`,
            group: "Project tools",
            projectId: viewProject,
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
        projectId: project.id,
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
        group: "Conversations",
        projectId: session.workspaceId,
        text: "chat chats conversation conversations",
        current:
          !!workspace?.projectId && session.workspaceId === workspace.projectId,
        label: session.title,
        hint: projectName ? `Conversation · ${projectName}` : "Conversation",
        run: () => onOpenConversation(session.id),
      })),
      ...PAGES,
      ...views,
      ...[
        ...research.entries.filter(
          (entry) => !contentTargets.has(contentTarget(entry.route)),
        ),
        ...fullText.entries,
      ].map((entry) => ({
        ...entry,
        preview: searchExcerpt(entry.text, query),
        run: goTo(entry.route),
        projectId:
          entry.route.page === "workspace" || entry.route.page === "history"
            ? entry.route.projectId
            : null,
      })),
      ...runs.map((run) => ({
        id: `review:${run.run_id}`,
        group: "Reviews",
        projectId: research.runProjects.get(run.run_id)?.id,
        label: run.title || run.input_name,
        hint: [
          "Review",
          research.runProjects.get(run.run_id)?.name,
          run.profile_name,
        ]
          .filter(Boolean)
          .join(" · "),
        run: research.runProjects.has(run.run_id)
          ? goTo({
              page: "history",
              runId: run.run_id,
              projectId: research.runProjects.get(run.run_id)!.id,
            })
          : () => onOpenRun(run.run_id),
      })),
      ...tasks.map((task) => ({
        id: `automation:${task.id}`,
        group: "Automations",
        projectId: task.projectId,
        label: task.name,
        hint: [
          "Automation",
          projects.find((p) => p.id === task.projectId)?.name,
        ]
          .filter(Boolean)
          .join(" · "),
        run: goTo({
          page: "tasks",
          taskId: task.id,
          ...(task.projectId ? { projectId: task.projectId } : {}),
        }),
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
    research.entries,
    research.runProjects,
    fullText.entries,
    query,
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
      .filter(
        (entry) =>
          (!scopeId ||
            entry.projectId === scopeId ||
            entry.group === "Actions") &&
          (kind === "All types" || entry.group === kind),
      )
      .filter(
        (entry) =>
          entry.matchedQuery === needle ||
          terms.every((term) =>
            (entry.label + " " + entry.hint + " " + (entry.text ?? ""))
              .toLocaleLowerCase()
              .includes(term),
          ),
      )
      .sort(
        (a, b) =>
          score(b) - score(a) ||
          a.label.localeCompare(b.label) ||
          a.hint.localeCompare(b.hint) ||
          a.id.localeCompare(b.id),
      );
    const byGroup = new Map<string, SearchEntry[]>();
    for (const entry of filtered)
      byGroup.set(entry.group, [...(byGroup.get(entry.group) ?? []), entry]);
    return [...byGroup].map(([label, items]) => ({
      label,
      items,
      visible: expanded.includes(label) ? items : items.slice(0, GROUP_LIMIT),
    }));
  }, [entries, query, expanded, scopeId, kind]);
  const matches = groups.flatMap((group) => group.visible);
  const loading =
    chats.loading ||
    fullText.loading ||
    research.loading ||
    Object.values(states).includes("loading");
  const failures = [
    ...Object.entries(states)
      .filter(([, state]) => state === "error")
      .map(([name]) => name),
    ...(chats.error ? ["Conversations"] : []),
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
        className="workspace-tool-picker ui-overlay-frame"
        tabIndex={-1}
      >
        <div className="workspace-picker-heading ui-overlay-header">
          <h2 id={title}>Search</h2>
          <IconButton label="Close search" onClick={onClose}>
            <Icon name="close" />
          </IconButton>
        </div>
        <input
          className="ui-input"
          data-autofocus
          type="search"
          role="combobox"
          aria-expanded="true"
          aria-autocomplete="list"
          aria-describedby={`${title}-coverage`}
          aria-label="Search Pipeline"
          placeholder="Find documents, findings, notes, files…"
          value={query}
          aria-controls={`${title}-results`}
          aria-activedescendant={
            selected >= 0 ? `${title}-result-${selected}` : undefined
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
        <div className="search-filters">
          <label>
            Scope
            <select
              className="ui-input"
              value={scope}
              onChange={(event) => {
                setScope(event.target.value);
                setIndex(0);
                setExpanded([]);
              }}
            >
              <option value="all">All projects</option>
              <option value="project" disabled={!workspace?.projectId}>
                This project
                {workspace?.projectName ? ` · ${workspace.projectName}` : ""}
              </option>
            </select>
          </label>
          <label>
            Type
            <select
              className="ui-input"
              value={kind}
              onChange={(event) => {
                setKind(event.target.value);
                setIndex(0);
                setExpanded([]);
              }}
            >
              {[
                "All types",
                "Projects",
                "Documents",
                "Findings",
                "Notes",
                "Files",
                "Conversations",
                "Reviews",
                "Automations",
                "Project tools",
                "Actions",
              ].map((type) => (
                <option key={type}>{type}</option>
              ))}
            </select>
          </label>
        </div>
        <p
          className="px-4 pb-2 text-ui-meta text-ink-muted"
          id={`${title}-coverage`}
        >
          {scopeId
            ? `Searching ${workspace?.projectName ?? "this project"}. `
            : "Searching active projects and unfiled work. "}
          Names, saved notes and findings, current captured document versions,
          and text files in project folders. Contents match the phrase you type.
          Archived records, older document versions, conversation messages,
          binary files, and ignored folders are excluded. File names also use
          the saved file list.
          {scopeId && " Actions navigate across the app."}
        </p>
        {fullText.enabled && (
          <p role="status" className="px-4 pb-2 text-ui-meta text-ink-muted">
            {!fullText.valid
              ? "Enter at least 2 characters to search contents; shorten very long queries."
              : fullText.loading
                ? `Searching contents… ${contentCount} searched.`
                : `Contents searched: ${contentCount}.`}
          </p>
        )}
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
                research.retry();
                fullText.retry();
              }}
            >
              Retry search
            </button>
          </div>
        )}
        {notices.length > 0 && (
          <details className="search-coverage">
            <summary>
              Some sources are unavailable or incomplete ({notices.length})
            </summary>
            <ul>
              {notices.map((notice, i) => (
                <li key={i}>{notice}</li>
              ))}
            </ul>
            <button
              className="ui-button ui-button--quiet ui-button--sm"
              onClick={() => {
                research.retry();
                fullText.retry();
                retry();
              }}
            >
              Retry content sources
            </button>
          </details>
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
                id={`${title}-result-${row}`}
                key={entry.id}
                data-highlighted={row === selected}
                onClick={() => choose(entry)}
              >
                <span className="search-result-label">
                  {entry.label}
                  {entry.preview && (
                    <span className="search-result-preview">
                      {entry.preview}
                    </span>
                  )}
                </span>
                <small>{entry.hint}</small>
              </button>
            </Fragment>
          ))}
          {!matches.length && !loading && (
            <p>
              {failures.length ||
              notices.length ||
              moreTasks ||
              research.more ||
              chats.more
                ? "No matches in the available results. Coverage is incomplete."
                : `Nothing matches “${query}” in this scope.`}
            </p>
          )}
        </div>
        <div className="flex flex-wrap gap-2 px-4 py-2">
          {moreTasks && (
            <button
              className="ui-button ui-button--quiet ui-button--sm"
              disabled={states.Automations === "loading"}
              onClick={loadMoreTasks}
            >
              Search older automations
            </button>
          )}
          {fullText.more && (
            <button
              className="ui-button ui-button--quiet ui-button--sm"
              disabled={fullText.loading}
              onClick={fullText.loadMore}
            >
              Load more matching passages
            </button>
          )}
          {research.more && (
            <button
              className="ui-button ui-button--quiet ui-button--sm"
              onClick={research.loadMore}
            >
              Search older findings (newest {research.limit} reviews searched)
            </button>
          )}
          {chats.more && (
            <button
              className="ui-button ui-button--quiet ui-button--sm"
              disabled={chats.loading}
              onClick={chats.loadMore}
            >
              Load more conversations
            </button>
          )}
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

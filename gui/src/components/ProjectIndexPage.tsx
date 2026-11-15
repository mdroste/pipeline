// Project selection stays compact; conversation browsing loads only on request.
import { lazy, Suspense, useState } from "react";
import useConversationIndex from "../hooks/useConversationIndex";
import ProjectChats, { type ChatFilter } from "./ProjectChats";
import useProjectIndex from "../hooks/useProjectIndex";
import {
  loadProjectPins,
  projectDate,
  saveProjectPins,
  type ProjectIndexItem,
} from "../lib/projectIndex";
import type { WorkspaceDestination } from "../lib/workspaceNavigation";
import { workbenchErrorMessage } from "../lib/workbenchError";
import PageHeader from "../ui/PageHeader";
import { Icon } from "../ui/icons";
import "./project-surface/ProjectHome.css";
import "./ProjectIndexPage.css";
import ReportIssueLink from "./ReportIssueLink";

const ProjectDialog = lazy(() => import("./WorkspaceProjectDialog"));

export default function ProjectIndexPage({
  onOpenProject,
  onResumeConversation,
  onNewConversation,
  creatingConversation = false,
}: {
  onOpenProject: (
    id: string,
    destination?: WorkspaceDestination,
  ) => Promise<void>;
  onResumeConversation: (id: string) => Promise<unknown>;
  onNewConversation: () => void;
  creatingConversation?: boolean;
}) {
  const { projects, loading, error, refresh } = useProjectIndex();
  const [search, setSearch] = useState("");
  const [showChats, setShowChats] = useState(false);
  const [chatSearch, setChatSearch] = useState("");
  const [chatFilter, setChatFilter] = useState<ChatFilter>("all");
  const chats = useConversationIndex(
    showChats,
    chatFilter === "archived",
    search,
    chatFilter === "unfiled",
  );
  const [pins, setPins] = useState(loadProjectPins);
  const [byDeadline, setByDeadline] = useState(false);
  const [creating, setCreating] = useState(false);
  const [pending, setPending] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const openProject = async (id: string) => {
    if (pending) return;
    setPending(true);
    setActionError(null);
    try {
      await onOpenProject(id);
    } catch (cause) {
      setActionError(workbenchErrorMessage(cause));
    } finally {
      setPending(false);
    }
  };
  const togglePin = (id: string) => {
    const next = pins.includes(id)
      ? pins.filter((pin) => pin !== id)
      : [...pins, id];
    try {
      saveProjectPins(next);
      setPins(next);
    } catch {
      setActionError("The project pin could not be saved. Please try again.");
    }
  };
  const visible = projects.filter((p) =>
    `${p.name} ${p.brief?.title ?? ""} ${p.root ?? ""}`
      .toLocaleLowerCase()
      .includes(search.trim().toLocaleLowerCase()),
  );
  const sorted = byDeadline
    ? [...visible].sort(
        (a, b) =>
          (a.targetDate ?? "9999").localeCompare(b.targetDate ?? "9999") ||
          b.updatedAt.localeCompare(a.updatedAt),
      )
    : visible;
  const groups: Array<[string, ProjectIndexItem[]]> = [
    ["Pinned", sorted.filter((p) => pins.includes(p.id))],
    [
      byDeadline ? "By deadline" : "Recently active",
      sorted.filter((p) => !pins.includes(p.id)),
    ],
  ];
  return (
    <div className="project-home project-index-page">
      <div className="ui-page">
        <PageHeader
          title="Projects"
          action={
            <button
              type="button"
              className="ui-button ui-button--primary"
              onClick={() => setCreating(true)}
            >
              <Icon name="plus" />
              New project
            </button>
          }
        />
        <div className="project-picker-toolbar">
          <div className="project-picker-search">
            <Icon name="search" />
            <input
              aria-label="Search projects"
              className="ui-input"
              type="search"
              placeholder="Search projects…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
          {projects.some((p) => p.targetDate) && (
            <select
              aria-label="Sort projects"
              className="ui-input project-picker-sort"
              value={byDeadline ? "deadline" : "recent"}
              onChange={(e) => setByDeadline(e.target.value === "deadline")}
            >
              <option value="recent">Recent activity</option>
              <option value="deadline">Deadline</option>
            </select>
          )}
        </div>
        {(error || actionError) && (
          <div role="alert" className="project-home-error">
            {actionError ?? error}
            {error && (
              <button
                type="button"
                className="ui-button ui-button--sm"
                disabled={loading}
                onClick={() => void refresh()}
              >
                Try again
              </button>
            )}
            <ReportIssueLink />
          </div>
        )}
        <section aria-label="Your projects" aria-busy={loading}>
          {loading && !projects.length && (
            <p role="status" className="project-home-empty">
              Loading projects…
            </p>
          )}
          {!loading && !error && !projects.length && (
            <div className="project-home-empty">
              <Icon name="folder" className="project-picker-empty-icon" />
              <h2>Your first project starts here</h2>
              <p>Create a project for your research.</p>
              <button
                type="button"
                className="ui-button"
                onClick={() => setCreating(true)}
              >
                Create your first project
              </button>
            </div>
          )}
          {!!projects.length && !visible.length && (
            <div role="status" className="project-home-empty">
              <p>No projects match “{search}”.</p>
              <button
                type="button"
                className="ui-button"
                onClick={() => setSearch("")}
              >
                Clear search
              </button>
            </div>
          )}
          {groups.map(
            ([label, items]) =>
              items.length > 0 && (
                <section
                  className="project-picker-group"
                  key={label}
                  aria-label={label}
                >
                  <h2 className="ui-page-label">{label}</h2>
                  {items.map((project) => {
                    const subtitle = project.brief?.title || project.root;
                    const updatedAt =
                      project.activity?.updatedAt ?? project.updatedAt;
                    const date = new Date(updatedAt);
                    return (
                      <article className="project-picker-row" key={project.id}>
                        <button
                          type="button"
                          className="project-picker-open"
                          aria-label={project.name}
                          disabled={pending}
                          onClick={() => void openProject(project.id)}
                        >
                          <span className="project-picker-folder">
                            <Icon name="folder" />
                          </span>
                          <span className="project-picker-info">
                            <span className="project-picker-name">
                              {project.name}
                            </span>
                            {subtitle && (
                              <span
                                className="project-picker-subtitle"
                                title={subtitle}
                              >
                                {subtitle}
                              </span>
                            )}
                          </span>
                          <time
                            className="project-picker-date"
                            dateTime={updatedAt}
                            title={`Last active ${projectDate(updatedAt)}`}
                          >
                            {Number.isNaN(date.getTime())
                              ? ""
                              : date.toLocaleDateString(undefined, {
                                  month: "short",
                                  day: "numeric",
                                  ...(date.getFullYear() !==
                                  new Date().getFullYear()
                                    ? { year: "numeric" as const }
                                    : {}),
                                })}
                          </time>
                          <Icon
                            name="chevron-right"
                            className="project-picker-chevron"
                          />
                        </button>
                        <button
                          type="button"
                          className="project-picker-pin"
                          aria-label={`${pins.includes(project.id) ? "Unpin" : "Pin"} ${project.name}`}
                          aria-pressed={pins.includes(project.id)}
                          onClick={() => togglePin(project.id)}
                        >
                          {pins.includes(project.id) ? "Unpin" : "Pin"}
                        </button>
                      </article>
                    );
                  })}
                </section>
              ),
          )}
        </section>
        <div className="project-picker-chats">
          <button
            type="button"
            className="project-picker-chat-toggle"
            aria-expanded={showChats}
            aria-controls="project-picker-chats"
            onClick={() => setShowChats(!showChats)}
          >
            <Icon name={showChats ? "chevron-down" : "chevron-right"} />
            Browse chats
          </button>
          <div id="project-picker-chats" hidden={!showChats}>
            {showChats && (
              <>
                <div className="project-picker-toolbar">
                  <input
                    aria-label="Search chats"
                    className="ui-input"
                    type="search"
                    placeholder="Search chats…"
                    value={chatSearch}
                    onChange={(e) => setChatSearch(e.target.value)}
                  />
                  <button
                    type="button"
                    className="ui-button"
                    onClick={onNewConversation}
                    disabled={creatingConversation}
                  >
                    + New chat
                  </button>
                  {chats.error && (
                    <button
                      type="button"
                      className="ui-button"
                      onClick={chats.refresh}
                    >
                      Retry chats
                    </button>
                  )}
                </div>
                <ProjectChats
                  loadMore={chats.more ? chats.loadMore : undefined}
                  conversations={chats.conversations}
                  loading={chats.loading}
                  error={chats.error}
                  projects={projects}
                  search={chatSearch}
                  filter={chatFilter}
                  onFilter={setChatFilter}
                  onOpen={onResumeConversation}
                  onOpenProject={onOpenProject}
                />
              </>
            )}
          </div>
        </div>
      </div>
      {creating && (
        <Suspense fallback={<p role="status">Opening new project…</p>}>
          <ProjectDialog
            onClose={() => setCreating(false)}
            onCreated={async (project) => {
              await onOpenProject(project.id);
              setCreating(false);
            }}
          />
        </Suspense>
      )}
    </div>
  );
}

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
import { useProjectEnvironment } from "../lib/projectEnvironment";
import { recentWork } from "../lib/project-overview/continuation";
import { targetSummary } from "../lib/project-overview/target";

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
  const { choose } = useProjectEnvironment();
  const [initialFolder, setInitialFolder] = useState<string | null>(null);
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
  const openProject = async (
    id: string,
    destination?: WorkspaceDestination,
  ) => {
    if (pending) return;
    setPending(true);
    setActionError(null);
    try {
      if (destination) await onOpenProject(id, destination);
      else await onOpenProject(id);
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
      <div className="ui-page ui-page--index">
        <PageHeader
          title="Projects"
          action={
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                className="ui-button"
                onClick={() => {
                  void choose({ directory: true, multiple: false })
                    .then((folder) => {
                      if (typeof folder === "string") {
                        setInitialFolder(folder);
                        setCreating(true);
                      }
                    })
                    .catch((cause) =>
                      setActionError(workbenchErrorMessage(cause)),
                    );
                }}
                title="Open a research folder or local Git repository"
              >
                Open folder
              </button>
              <button
                type="button"
                className="ui-button ui-button--primary"
                onClick={() => {
                  setInitialFolder(null);
                  setCreating(true);
                }}
              >
                <Icon name="plus" />
                New project
              </button>
            </div>
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
                    const deadline = targetSummary(project, new Date());
                    const showDeadline =
                      deadline && (byDeadline || deadline.days <= 30);
                    const resume = recentWork(project.id)[0];
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
                            {resume && (
                              <span className="project-picker-subtitle">
                                Recent: {resume.title}
                              </span>
                            )}
                            {project.interruptedEdits > 0 && (
                              <span className="project-picker-attention">
                                Interrupted edit needs recovery
                              </span>
                            )}
                            {!project.interruptedEdits &&
                              project.proposedNotes > 0 && (
                                <span className="project-picker-subtitle">
                                  {project.proposedNotes} proposed{" "}
                                  {project.proposedNotes === 1
                                    ? "note"
                                    : "notes"}
                                </span>
                              )}
                          </span>
                          <time
                            className="project-picker-date"
                            data-deadline={showDeadline || undefined}
                            dateTime={
                              showDeadline ? project.targetDate! : updatedAt
                            }
                            title={
                              showDeadline
                                ? `${deadline.label}: ${deadline.date}`
                                : `Last active ${projectDate(updatedAt)}`
                            }
                          >
                            {showDeadline
                              ? `${deadline.label} · ${deadline.date}${deadline.days < 0 ? " · overdue" : deadline.days === 0 ? " · today" : ""}`
                              : `Active ${new Date(updatedAt).toLocaleDateString(undefined, { month: "short", day: "numeric" })}`}
                          </time>
                          <Icon
                            name="chevron-right"
                            className="project-picker-chevron"
                          />
                        </button>
                        <button
                          type="button"
                          className="project-picker-pin"
                          aria-label={`Overview of ${project.name}`}
                          disabled={pending}
                          onClick={() =>
                            void openProject(project.id, "overview")
                          }
                        >
                          Overview
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
            Browse conversations
          </button>
          <div id="project-picker-chats" hidden={!showChats}>
            {showChats && (
              <>
                <div className="project-picker-toolbar">
                  <input
                    aria-label="Search conversations"
                    className="ui-input"
                    type="search"
                    placeholder="Search conversations…"
                    value={chatSearch}
                    onChange={(e) => setChatSearch(e.target.value)}
                  />
                  <button
                    type="button"
                    className="ui-button"
                    onClick={onNewConversation}
                    disabled={creatingConversation}
                  >
                    + New conversation
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
            initialFolder={initialFolder}
            onClose={() => setCreating(false)}
            onCreated={async (project) => {
              await onOpenProject(
                project.id,
                initialFolder ? "files" : undefined,
              );
              setCreating(false);
            }}
          />
        </Suspense>
      )}
    </div>
  );
}

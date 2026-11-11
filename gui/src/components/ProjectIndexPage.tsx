import { lazy, Suspense, useState } from "react";
import useConversationIndex from "../hooks/useConversationIndex";
import ProjectChats, { type ChatFilter } from "./ProjectChats";
import useProjectIndex from "../hooks/useProjectIndex";
import {
  activityKind,
  loadProjectPins,
  projectDate,
  saveProjectPins,
  type ProjectIndexItem,
} from "../lib/projectIndex";
import {
  stageLabel,
  type WorkspaceDestination,
} from "../lib/workspaceNavigation";
import { targetSummary } from "../lib/projectOverview";
import { workbenchErrorMessage } from "../lib/workbenchError";
import PageHeader from "../ui/PageHeader";
import "./project-surface/ProjectHome.css";
import ReportIssueLink from "./ReportIssueLink";

const ProjectDialog = lazy(() => import("./WorkspaceProjectDialog"));

/** Stage, venue, and deadline on one line; empty when none is set. */
function paperLine(project: ProjectIndexItem) {
  const target = targetSummary(project, new Date());
  return [
    stageLabel(project.stage),
    project.venue?.trim(),
    target &&
      `${target.label} ${target.date}${
        target.days >= 0
          ? ` · ${target.days === 0 ? "today" : target.days === 1 ? "tomorrow" : `in ${target.days} days`}`
          : ` · ${-target.days === 1 ? "yesterday" : `${-target.days} days ago`}`
      }`,
  ]
    .filter(Boolean)
    .join(" · ");
}

export default function ProjectIndexPage({
  onOpenProject,
  onResumeConversation,
  onNewConversation,
}: {
  onOpenProject: (
    id: string,
    destination?: WorkspaceDestination,
  ) => Promise<void>;
  onResumeConversation: (id: string) => Promise<unknown>;
  onNewConversation: () => void;
}) {
  const { projects, loading, error, refresh } = useProjectIndex();
  const [search, setSearch] = useState("");
  const [chatFilter, setChatFilter] = useState<ChatFilter>("all");
  const chats = useConversationIndex(true, chatFilter === "archived", true);
  const [pins, setPins] = useState(loadProjectPins);
  const [byDeadline, setByDeadline] = useState(false);
  const [creating, setCreating] = useState(false);
  const [pending, setPending] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const run = async (action: () => Promise<unknown>) => {
    if (pending) return;
    setPending(true);
    setActionError(null);
    try {
      await action();
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
  const hasDeadlines = projects.some((p) => p.targetDate);
  return (
    <div className="project-home project-index-page">
      <div className="ui-page">
        <PageHeader
          title="Projects"
          description="Your research and chats, together. Start a chat without a project, or pick up where you left off."
          action={
            <div className="project-index-create">
              <button
                type="button"
                className="ui-page-button"
                onClick={() => setCreating(true)}
              >
                + New project
              </button>
              <button
                type="button"
                className="ui-page-button primary"
                onClick={onNewConversation}
              >
                + New chat
              </button>
            </div>
          }
        />
        <div className="ui-page-toolbar project-index-search">
          <label className="sr-only" htmlFor="project-index-search">
            Search projects and chats
          </label>
          <input
            id="project-index-search"
            type="search"
            placeholder="Search projects and chats…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <button
            type="button"
            className="ui-page-button"
            disabled={loading || chats.loading}
            onClick={() => {
              void refresh();
              chats.refresh();
            }}
          >
            Refresh
          </button>
          {hasDeadlines && (
            <button
              type="button"
              className="ui-page-button"
              aria-pressed={byDeadline}
              onClick={() => setByDeadline(!byDeadline)}
            >
              {byDeadline ? "Sort by recent activity" : "Sort by deadline"}
            </button>
          )}
        </div>
        <div className="project-index-layout">
          <section
            className="project-index-projects"
            aria-label="Your projects"
          >
            <div className="project-index-section-heading">
              <h2>Your projects</h2>
              <p className="project-home-meta">
                Files, notes, and ongoing research.
              </p>
            </div>
            {(error || actionError) && (
              <p role="alert" className="project-home-error">
                {actionError ?? error}
                <ReportIssueLink />
              </p>
            )}
            {loading && !projects.length && (
              <p role="status" className="project-home-empty">
                Loading projects…
              </p>
            )}
            {!loading && !error && !projects.length && (
              <section className="project-home-empty">
                <h2>Give your research a home</h2>
                <p>
                  Keep related conversations, papers, and notes in a project.
                  Add files whenever you’re ready, or start with a chat.
                </p>
                <button
                  type="button"
                  className="ui-page-button primary"
                  onClick={() => setCreating(true)}
                >
                  Create your first project
                </button>
              </section>
            )}
            {!!projects.length && !visible.length && (
              <p role="status" className="project-home-empty">
                No projects match “{search}”.
              </p>
            )}
            {groups.map(
              ([label, items]) =>
                items.length > 0 && (
                  <section
                    className="project-index-group"
                    key={label}
                    aria-label={label}
                  >
                    <h2 className="ui-page-label">{label}</h2>
                    {items.map((project) => (
                      <article
                        className="ui-page-row project-index-row"
                        key={project.id}
                      >
                        <div className="project-index-info">
                          <button
                            type="button"
                            className="project-index-name"
                            disabled={pending}
                            onClick={() =>
                              void run(() => onOpenProject(project.id))
                            }
                          >
                            {project.name}
                          </button>
                          {project.brief ? (
                            <p className="project-index-brief">
                              {project.brief.title}
                            </p>
                          ) : (
                            <p className="project-index-brief muted">
                              {project.root
                                ? project.root
                                    .split(/[\\/]/)
                                    .filter(Boolean)
                                    .pop()
                                : "Conversations and research notes"}
                            </p>
                          )}
                          {paperLine(project) && (
                            <p className="project-home-meta">
                              {paperLine(project)}
                            </p>
                          )}
                          <p className="project-home-meta">
                            {project.activity
                              ? `${activityKind(project.activity.kind)}: ${project.activity.title}`
                              : "Project created"}{" "}
                            ·{" "}
                            {projectDate(
                              project.activity?.updatedAt ?? project.updatedAt,
                            )}
                          </p>
                          {project.missingRootAt && (
                            <p className="project-home-attention">
                              The attached folder is unavailable.
                            </p>
                          )}
                          {project.interruptedEdits > 0 ? (
                            <button
                              className="project-home-link project-home-attention"
                              disabled={pending}
                              onClick={() =>
                                void run(() =>
                                  onOpenProject(project.id, "edits"),
                                )
                              }
                            >
                              An interrupted edit needs attention →
                            </button>
                          ) : project.proposedNotes > 0 ? (
                            <button
                              className="project-home-link project-home-attention"
                              disabled={pending}
                              onClick={() =>
                                void run(() =>
                                  onOpenProject(project.id, "overview"),
                                )
                              }
                            >
                              {project.proposedNotes} suggested{" "}
                              {project.proposedNotes === 1
                                ? "note awaits"
                                : "notes await"}{" "}
                              your decision →
                            </button>
                          ) : (
                            project.nextTask && (
                              <button
                                className="project-home-link"
                                disabled={pending}
                                onClick={() =>
                                  void run(() =>
                                    onOpenProject(project.id, "tasks"),
                                  )
                                }
                              >
                                Next: {project.nextTask.title} →
                              </button>
                            )
                          )}
                        </div>
                        <div className="project-index-actions">
                          <button
                            type="button"
                            className="ui-page-button"
                            disabled={pending}
                            onClick={() =>
                              void run(() =>
                                project.conversation
                                  ? onResumeConversation(
                                      project.conversation.id,
                                    )
                                  : onOpenProject(project.id),
                              )
                            }
                          >
                            {project.conversation
                              ? "Resume conversation"
                              : "Open project"}{" "}
                            →
                          </button>
                          <button
                            type="button"
                            className="project-home-link muted"
                            aria-label={`${pins.includes(project.id) ? "Unpin" : "Pin"} ${project.name}`}
                            aria-pressed={pins.includes(project.id)}
                            onClick={() => togglePin(project.id)}
                          >
                            {pins.includes(project.id)
                              ? "Unpin"
                              : "Pin project"}
                          </button>
                        </div>
                      </article>
                    ))}
                  </section>
                ),
            )}
          </section>
          <ProjectChats
            conversations={chats.conversations}
            loading={chats.loading}
            error={chats.error}
            projects={projects}
            search={search}
            filter={chatFilter}
            onFilter={setChatFilter}
            onOpen={onResumeConversation}
            onOpenProject={onOpenProject}
          />
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

import { lazy, Suspense, useState } from "react";
import useProjectIndex from "../hooks/useProjectIndex";
import {
  activityKind,
  loadProjectPins,
  projectDate,
  saveProjectPins,
  type ProjectIndexItem,
} from "../lib/projectIndex";
import type { WorkspaceDestination } from "../lib/workspaceNavigation";
import { workbenchErrorMessage } from "../lib/workbenchError";
import "./project-surface/ProjectHome.css";

const ProjectDialog = lazy(() => import("./WorkspaceProjectDialog"));

export default function ProjectIndexPage({
  onOpenProject,
  onResumeConversation,
}: {
  onOpenProject: (
    id: string,
    destination?: WorkspaceDestination,
  ) => Promise<void>;
  onResumeConversation: (id: string) => Promise<unknown>;
}) {
  const { projects, loading, error, refresh } = useProjectIndex();
  const [search, setSearch] = useState("");
  const [pins, setPins] = useState(loadProjectPins);
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
  const groups: Array<[string, ProjectIndexItem[]]> = [
    ["Pinned", visible.filter((p) => pins.includes(p.id))],
    ["Recently active", visible.filter((p) => !pins.includes(p.id))],
  ];
  return (
    <div className="project-home project-index-page">
      <div className="project-home-content">
        <header className="project-home-heading">
          <div>
            <h1>Projects</h1>
            <p>Pick up your research where you left it.</p>
          </div>
          <button
            type="button"
            className="project-home-button primary"
            onClick={() => setCreating(true)}
          >
            New project
          </button>
        </header>
        <div className="project-index-search">
          <label className="sr-only" htmlFor="project-index-search">
            Search projects
          </label>
          <input
            id="project-index-search"
            type="search"
            placeholder="Search projects…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <button
            type="button"
            className="project-home-button"
            disabled={loading}
            onClick={() => void refresh()}
          >
            Refresh
          </button>
        </div>
        {(error || actionError) && (
          <p role="alert" className="project-home-error">
            {actionError ?? error}
          </p>
        )}
        {loading && !projects.length && (
          <p role="status" className="project-home-empty">
            Loading projects…
          </p>
        )}
        {!loading && !error && !projects.length && (
          <section className="project-home-empty">
            <h2>Your research starts here</h2>
            <p>
              Create a project to keep conversations, papers, and notes
              together. A folder and paper are optional.
            </p>
            <button
              type="button"
              className="project-home-button primary"
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
                <h2>{label}</h2>
                {items.map((project) => (
                  <article className="project-index-row" key={project.id}>
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
                            ? project.root.split(/[\\/]/).filter(Boolean).pop()
                            : "Conversations and research notes"}
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
                            void run(() => onOpenProject(project.id, "edits"))
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
                              void run(() => onOpenProject(project.id, "tasks"))
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
                        className="project-home-button"
                        disabled={pending}
                        onClick={() =>
                          void run(() =>
                            project.conversation
                              ? onResumeConversation(project.conversation.id)
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
                        {pins.includes(project.id) ? "Unpin" : "Pin project"}
                      </button>
                    </div>
                  </article>
                ))}
              </section>
            ),
        )}
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

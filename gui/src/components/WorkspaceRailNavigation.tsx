// Project selection and project contents are separate controls. Their callbacks
// stay with WorkspacePage so switching still passes through its save gates.
import { useState } from "react";
import { createPortal } from "react-dom";
import useModalDialog from "../hooks/useModalDialog";
import { loadProjectPins } from "../lib/projectIndex";
import type { WorkspaceDestination } from "../lib/workspaceNavigation";
import type { Workspace } from "../lib/workbenchTypes";
import Badge from "../ui/Badge";
import { Icon } from "../ui/icons";
import { NavRow } from "./nav-rail/NavRows";
import WorkspaceProjectNavigation from "./WorkspaceProjectNavigation";

interface Props {
  project: Workspace | null;
  projects: Workspace[];
  destination: WorkspaceDestination;
  chatOpen: boolean;
  disabled: boolean;
  onNavigate: (view: WorkspaceDestination) => void;
  onSelectProject: (id: string) => void;
  onNewProject: () => void;
  onAllProjects?: () => void;
  onBrowseUnfiled: () => void;
}
function ProjectSwitcher({
  project,
  projects,
  onSelectProject,
  onNewProject,
  onAllProjects,
  onClose,
}: Pick<
  Props,
  "projects" | "onSelectProject" | "onNewProject" | "onAllProjects"
> & { project: Workspace; onClose: () => void }) {
  const ref = useModalDialog<HTMLDivElement>(onClose, true, true);
  const [query, setQuery] = useState("");
  const [pins] = useState(loadProjectPins);
  const matches = projects
    .filter(
      (p) =>
        !p.archivedAt &&
        p.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()),
    )
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  const choose = (action?: () => void) => {
    onClose();
    action?.();
  };
  return createPortal(
    <div
      className="workspace-picker-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label="Switch project"
        tabIndex={-1}
        className="workspace-tool-picker"
      >
        <div className="workspace-picker-heading">
          <h2>Switch project</h2>
          <button
            type="button"
            aria-label="Close project switcher"
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <input
          data-autofocus
          type="search"
          aria-label="Find a project"
          placeholder="Find a project…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div className="focused-switch-results">
          {["Pinned", "Recent projects"].map((group, i) => {
            const items = matches.filter((p) =>
              i ? !pins.includes(p.id) : pins.includes(p.id),
            );
            return (
              items.length > 0 && (
                <section key={group}>
                  <h3>{group}</h3>
                  {items.map((p) => (
                    <button
                      key={p.id}
                      className="nav-row"
                      type="button"
                      onClick={() =>
                        choose(() => {
                          if (p.id !== project.id) onSelectProject(p.id);
                        })
                      }
                    >
                      <Icon name="folder" />
                      <span className="nav-row-label" title={p.name}>
                        {p.name}
                      </span>
                      {p.id === project.id && (
                        <>
                          <Icon name="check" />
                          <span className="sr-only">Current project</span>
                        </>
                      )}
                    </button>
                  ))}
                </section>
              )
            );
          })}
          {!matches.length && <p role="status">No matching projects.</p>}
        </div>
        <div className="focused-switch-actions">
          <button type="button" onClick={() => choose(onAllProjects)}>
            All projects
          </button>
          <button type="button" onClick={() => choose(onNewProject)}>
            New project
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
export default function WorkspaceRailNavigation({
  project,
  projects,
  destination,
  chatOpen,
  disabled,
  onNavigate,
  onSelectProject,
  onNewProject,
  onAllProjects,
  onBrowseUnfiled,
}: Props) {
  const [switching, setSwitching] = useState(false);
  if (!project)
    return (
      <>
        <div className="focused-scope-label">No project</div>
        <NavRow
          icon="assistant"
          label="Chats"
          disabled={disabled}
          onClick={onBrowseUnfiled}
        />
      </>
    );
  return (
    <>
      <div className="focused-project-header">
        <div className="focused-scope-label">Current project</div>
        <button
          type="button"
          className="focused-project-switch"
          disabled={disabled}
          aria-label={"Switch project: " + project.name}
          aria-haspopup="dialog"
          aria-expanded={switching}
          title={[project.name, project.root].filter(Boolean).join(" · ")}
          onClick={() => setSwitching(true)}
        >
          <span>{project.name}</span>
          <Icon name="chevron-down" />
        </button>
        {project.missingRootAt && (
          <Badge kind="attention" label="Folder not found" />
        )}
      </div>
      {switching && !disabled && (
        <ProjectSwitcher
          project={project}
          projects={projects}
          onSelectProject={onSelectProject}
          onAllProjects={onAllProjects}
          onNewProject={onNewProject}
          onClose={() => setSwitching(false)}
        />
      )}
      <WorkspaceProjectNavigation
        destination={chatOpen ? "chats" : destination}
        disabled={disabled}
        onNavigate={onNavigate}
        hasFolder={Boolean(project.root)}
      />
    </>
  );
}

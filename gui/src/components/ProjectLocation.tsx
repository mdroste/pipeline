import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { router } from "../lib/router";
import { workspaceClient } from "../lib/workbench/workspaceClient";
import type { Workspace } from "../lib/workbench/sessionTypes";
import { Icon } from "../ui/icons";
import { Menu, MenuItem, MenuSeparator } from "../ui/Menu";

/** The desk keeps its project switcher even when the global rail is collapsed. */
export default function ProjectLocation({
  projectId,
  name,
  disabled,
}: {
  projectId: string;
  name: string;
  disabled: boolean;
}) {
  const anchor = useRef<HTMLButtonElement>(null);
  const menuId = useId();
  const [open, setOpen] = useState(false);
  const [projects, setProjects] = useState<Workspace[]>([]);
  const [status, setStatus] = useState("Loading projects…");
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!open) return;
    let live = true;
    setStatus("Loading projects…");
    void workspaceClient
      .listWorkspaces(false)
      .then(({ workspaces }) => {
        if (!live) return;
        setProjects(workspaces);
        setStatus("");
      })
      .catch(() => {
        if (live) setStatus("Couldn’t load projects.");
      });
    return () => {
      live = false;
    };
  }, [open, attempt]);
  useLayoutEffect(() => {
    // A successful retry removes its focused menu item. Keep Escape and arrow
    // navigation inside the menu when the asynchronous list replaces it.
    if (open && document.activeElement === document.body)
      document
        .getElementById(menuId)
        ?.querySelector<HTMLElement>('[role^="menuitem"]:not(:disabled)')
        ?.focus();
  }, [open, status, menuId]);
  return (
    <>
      <button
        ref={anchor}
        type="button"
        className="workspace-desk-button workspace-desk-title"
        aria-label={`Switch project: ${name}`}
        title={name}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen(!open)}
      >
        <Icon name="project" className="h-4 w-4 shrink-0" />
        <span className="truncate">{name}</span>
        <Icon name="chevron-down" className="h-3.5 w-3.5 shrink-0" />
      </button>
      <Menu
        id={menuId}
        open={open && !disabled}
        onClose={() => setOpen(false)}
        anchorRef={anchor}
        label="Switch project"
        align="start"
      >
        {status && (
          <div role="status" className="px-3 py-2 text-ui-meta text-ink-muted">
            {status}
          </div>
        )}
        {status.startsWith("Couldn") && (
          <MenuItem keepOpen onSelect={() => setAttempt((n) => n + 1)}>
            Retry projects
          </MenuItem>
        )}
        {projects.map((project) => (
          <MenuItem
            key={project.id}
            checked={project.id === projectId}
            onSelect={() => {
              if (project.id !== projectId)
                void router.navigate({
                  page: "workspace",
                  projectId: project.id,
                  surface: "project",
                  destination: "overview",
                });
            }}
          >
            {project.name}
          </MenuItem>
        ))}
        <MenuSeparator />
        <MenuItem
          onSelect={() =>
            void router.navigate({
              page: "workspace",
              projectId,
              surface: "project",
              destination: "overview",
            })
          }
        >
          Project overview
        </MenuItem>
        <MenuItem
          onSelect={() => void router.navigate({ page: "project-index" })}
        >
          All projects
        </MenuItem>
        <MenuSeparator />
        <MenuItem
          onSelect={() =>
            void router.navigate({
              page: "workspace",
              projectId,
              surface: "project",
              destination: "project-settings",
            })
          }
        >
          Project settings
        </MenuItem>
      </Menu>
    </>
  );
}

import { useEffect, useState } from "react";
import { router } from "../lib/router";
import { workspaceClient } from "../lib/workbench/workspaceClient";
import { Icon } from "../ui/icons";
import "../ui/page.css";

/** Keeps project origin available when a result opens outside the project desk. */
export default function ProjectBreadcrumb({
  projectId,
  location,
}: {
  projectId: string;
  location: string;
}) {
  const [name, setName] = useState("Project");
  useEffect(() => {
    let live = true;
    setName("Project");
    void workspaceClient
      .getWorkspace(projectId)
      .then((project) => {
        if (live) setName(project.name);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [projectId]);
  return (
    <nav className="ui-location" aria-label="Location">
      <button
        className="ui-button ui-button--link ui-button--sm"
        onClick={() =>
          void router.navigate({
            page: "workspace",
            projectId,
            surface: "project",
            destination: "overview",
          })
        }
      >
        {name}
      </button>
      <Icon name="chevron-right" className="h-3.5 w-3.5 shrink-0" />
      <span aria-current="location">{location}</span>
    </nav>
  );
}

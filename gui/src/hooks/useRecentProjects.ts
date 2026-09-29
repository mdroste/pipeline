import { useEffect, useState } from "react";
import type { AppPage, RecentProject } from "../components/NavRail";

export default function useRecentProjects(page: AppPage) {
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);
  const [projectsLoading, setProjectsLoading] = useState(true);
  useEffect(() => {
    if (!["home", "workspace", "project-index"].includes(page)) return;
    let live = true;
    setProjectsLoading(true);
    void import("../lib/workbenchClient")
      .then(({ workbenchClient }) => workbenchClient.listWorkspaces(false))
      .then(({ workspaces }) => {
        if (live)
          setRecentProjects(
            [...workspaces]
              .filter((p) => !p.archivedAt)
              .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
              .slice(0, 6),
          );
      })
      .catch(() => {
        /* The project index reports loading errors and offers retry. */
      })
      .finally(() => {
        if (live) setProjectsLoading(false);
      });
    return () => {
      live = false;
    };
  }, [page]);
  return { recentProjects, projectsLoading };
}

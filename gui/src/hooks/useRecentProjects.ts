import { useEffect, useRef, useState } from "react";
import type { AppPage, RecentProject } from "../components/NavRail";

const REFRESH_PAGES: AppPage[] = ["home", "workspace", "project-index"];

export default function useRecentProjects(page: AppPage) {
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);
  const [projectsLoading, setProjectsLoading] = useState(true);
  const loaded = useRef(false);
  useEffect(() => {
    // The sidebar lists these on every page, so the first page always loads.
    if (loaded.current && !REFRESH_PAGES.includes(page)) return;
    loaded.current = true;
    let live = true;
    setProjectsLoading(true);
    void import("../lib/workbench/workspaceClient")
      .then(({ workspaceClient }) => workspaceClient.listWorkspaces(false))
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

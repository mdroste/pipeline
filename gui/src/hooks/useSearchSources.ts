import { useEffect, useState } from "react";
import { reviewHistoryClient } from "../lib/reviewHistoryClient";
import { taskClient } from "../lib/taskClient";
import { workspaceClient } from "../lib/workbench/workspaceClient";
import type { Workspace } from "../lib/workbenchTypes";
import type { RunSummary } from "../lib/types";
import type { TaskSummary } from "../lib/taskTypes";
type Source = "Projects" | "Reviews" | "Automations";
export default function useSearchSources() {
  const [projects, setProjects] = useState<Workspace[]>([]);
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [attempt, setAttempt] = useState(0);
  const [states, setStates] = useState<
    Record<Source, "loading" | "ready" | "error">
  >({ Projects: "loading", Reviews: "loading", Automations: "loading" });
  useEffect(() => {
    let live = true;
    setStates({
      Projects: "loading",
      Reviews: "loading",
      Automations: "loading",
    });
    const read = async (source: Source, action: () => Promise<void>) => {
      try {
        await action();
        if (live) setStates((s) => ({ ...s, [source]: "ready" }));
      } catch {
        if (live) setStates((s) => ({ ...s, [source]: "error" }));
      }
    };
    void read("Projects", async () => {
      const { workspaces } = await workspaceClient.listWorkspaces(false);
      if (live)
        setProjects(
          [...workspaces].sort((a, b) =>
            b.updatedAt.localeCompare(a.updatedAt),
          ),
        );
    });
    void read("Reviews", async () => {
      const value = await reviewHistoryClient.list();
      if (live) setRuns(value);
    });
    void read("Automations", async () => {
      const value = await taskClient.list();
      if (live) setTasks(value);
    });
    return () => {
      live = false;
    };
  }, [attempt]);
  return {
    projects,
    runs,
    tasks,
    states,
    retry: () => setAttempt((n) => n + 1),
  };
}

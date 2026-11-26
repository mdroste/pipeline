import { useEffect, useState } from "react";
import { reviewHistoryClient } from "../lib/reviewHistoryClient";
import { taskClient } from "../lib/taskClient";
import { workspaceClient } from "../lib/workbench/workspaceClient";
import type { Workspace } from "../lib/workbench/sessionTypes";
import type { RunSummary } from "../lib/reviewTypes";
import type { TaskSummary } from "../lib/taskTypes";
type Source = "Projects" | "Reviews" | "Automations";
export default function useSearchSources() {
  const [projects, setProjects] = useState<Workspace[]>([]);
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [tasks, setTasks] = useState<
    (TaskSummary & { projectId?: string | null })[]
  >([]);
  const [taskNotices, setTaskNotices] = useState<string[]>([]);
  const [taskLimit, setTaskLimit] = useState(50);
  const [moreTasks, setMoreTasks] = useState(false);
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
      const value: (TaskSummary & { projectId?: string | null })[] = [];
      let more = false;
      for (let offset = 0; live && offset < taskLimit; offset += 50) {
        // The session view without a session filter includes active and finished runs.
        const page = await taskClient.list("session", null, offset, true);
        value.push(...page.slice(0, 50));
        more = page.length > 50;
        if (!more) break;
      }
      if (!live) return;
      setTasks(value);
      setMoreTasks(more);
      setTaskNotices([]);
      const pending = [...value];
      const notices: string[] = [];
      const scopes = new Map<string, string | null>();
      const worker = async () => {
        while (live && pending.length) {
          const task = pending.shift()!;
          try {
            const detail = await taskClient.get(task.id);
            if (!live) return;
            scopes.set(task.id, detail.scope.workspaceId);
          } catch {
            if (live)
              notices.push(
                `${task.name}: project association unavailable. This automation appears only under All projects until retried.`,
              );
          }
        }
      };
      await Promise.all([worker(), worker(), worker()]);
      if (live) {
        setTasks(
          value.map((task) => ({ ...task, projectId: scopes.get(task.id) })),
        );
        setTaskNotices(notices);
      }
    });
    return () => {
      live = false;
    };
  }, [attempt, taskLimit]);
  return {
    projects,
    runs,
    tasks,
    taskNotices,
    moreTasks,
    loadMoreTasks: () => setTaskLimit((n) => n + 50),
    states,
    retry: () => setAttempt((n) => n + 1),
  };
}

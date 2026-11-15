import { useCallback, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { confirmDialog, notify } from "../DialogService";
import { reviewCollectionClient } from "../../lib/reviewCollectionClient";
import { reviewHistoryClient } from "../../lib/reviewHistoryClient";
import type { Project } from "../../lib/reviewCollectionTypes";
import type {
  RunSummary,
  RunsDiskUsage,
  TrashedRun,
} from "../../lib/reviewTypes";

export function useHistoryData(
  setCompareIds: Dispatch<SetStateAction<string[]>>,
) {
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [usage, setUsage] = useState<RunsDiskUsage | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [trashedRuns, setTrashedRuns] = useState<TrashedRun[]>([]);
  const [trashLoading, setTrashLoading] = useState(false);
  const refreshRequestRef = useRef(0);

  const refresh = useCallback(async () => {
    const request = ++refreshRequestRef.current;
    setLoading(true);
    try {
      const [nextRuns, nextUsage, nextProjects] = await Promise.all([
        reviewHistoryClient.list(),
        reviewHistoryClient.usage().catch(() => null),
        reviewCollectionClient.list().catch(() => null),
      ]);
      if (request !== refreshRequestRef.current) return;
      setRuns(nextRuns);
      // A report that left the list cannot be compared, and its row is gone.
      setCompareIds((prev) =>
        prev.filter((id) => nextRuns.some((run) => run.run_id === id)),
      );
      setUsage(nextUsage);
      if (nextProjects) setProjects(nextProjects.projects);
      setError(null);
    } catch (e) {
      if (request === refreshRequestRef.current) {
        setError(e instanceof Error ? e.message : String(e));
      }
    } finally {
      if (request === refreshRequestRef.current) setLoading(false);
    }
  }, [setCompareIds]);

  const refreshTrash = useCallback(async () => {
    setTrashLoading(true);
    try {
      setTrashedRuns(await reviewHistoryClient.trashed());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setTrashLoading(false);
    }
  }, []);

  const restoreRun = async (run: TrashedRun) => {
    try {
      await reviewHistoryClient.restore({ runId: run.run_id });
      notify("Report restored to History.", "success");
      await Promise.all([refresh(), refreshTrash()]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const permanentlyDeleteRun = async (run: TrashedRun) => {
    const label = run.title || run.input_name || run.run_id;
    if (
      !(await confirmDialog(
        `Permanently delete “${label}” and all its artifacts? This cannot be undone.`,
        {
          title: "Delete report forever",
          confirmLabel: "Delete forever",
          destructive: true,
        },
      ))
    )
      return;
    try {
      await reviewHistoryClient.remove({ runId: run.run_id });
      notify("Report permanently deleted.", "success");
      await refreshTrash();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return {
    runs,
    projects,
    usage,
    loading,
    error,
    setError,
    refresh,
    refreshRequestRef,
    trashedRuns,
    trashLoading,
    refreshTrash,
    restoreRun,
    permanentlyDeleteRun,
  };
}

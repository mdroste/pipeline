import { useProjectEnvironment } from "../lib/projectEnvironment";
import { useEffect, useRef, useState } from "react";
import { reviewCollectionClient } from "../lib/reviewCollectionClient";
import { reviewHistoryClient } from "../lib/reviewHistoryClient";
import { studioClient } from "../lib/studioClient";
import { taskClient } from "../lib/taskClient";
import { listProjectReviewLinks } from "../lib/workbench/releaseClient";
import { repositoryClient } from "../lib/repositoryClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import {
  NO_SIGNALS,
  type ProjectSignals,
  type SignalSource,
  type SignalState,
} from "../lib/project-overview/types";
import { reviewsForProject } from "../lib/project-overview/projectPaths";

/** Independently refreshable sources; failed refreshes preserve the last known value. */
export default function useProjectSignals({
  workspaceId,
  root,
  sessionIds,
  enabled,
  stamp,
}: {
  workspaceId: string;
  root: string | null;
  sessionIds: string[];
  enabled: boolean;
  stamp: unknown;
}) {
  const { signals: suppliedSignals } = useProjectEnvironment();
  const [signals, setSignals] = useState<ProjectSignals>(NO_SIGNALS);
  const [request, setRequest] = useState<{ id: number; source?: SignalSource }>(
    { id: 0 },
  );
  const loaded = useRef({ at: 0, request: -1, key: "" });
  const sessions = useRef(sessionIds);
  sessions.current = sessionIds;
  const generations = useRef<Record<string, number>>({});
  const mounted = useRef(true);
  const currentKey = `${workspaceId}\n${root ?? ""}`;
  const keyRef = useRef(currentKey);
  keyRef.current = currentKey;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const previousStamp = useRef(stamp);
  useEffect(() => {
    if (previousStamp.current === stamp) return;
    previousStamp.current = stamp;
    if (!enabled || suppliedSignals) return;
    const timer = setTimeout(
      () => setRequest((old) => ({ id: old.id + 1 })),
      250,
    );
    return () => clearTimeout(timer);
  }, [stamp, enabled, suppliedSignals]);
  useEffect(() => {
    if (!enabled || suppliedSignals) return;
    const changedProject = loaded.current.key !== currentKey;
    const forced = changedProject || loaded.current.request !== request.id;
    if (!forced && Date.now() - loaded.current.at < 10_000) return;
    if (changedProject) setSignals(NO_SIGNALS);
    loaded.current = { at: Date.now(), request: request.id, key: currentKey };
    const only = !changedProject && forced ? request.source : undefined;
    const load = <T>(
      name: SignalSource,
      scope: string,
      source: () => Promise<T>,
      apply: (value: T) => Partial<ProjectSignals>,
      completeness: (
        value: T,
      ) => Pick<SignalState, "complete" | "error"> = () => ({ complete: true }),
    ) => {
      if (only && only !== name) return;
      const generation = (generations.current[name] ?? 0) + 1;
      generations.current[name] = generation;
      const live = () =>
        mounted.current &&
        keyRef.current === currentKey &&
        generations.current[name] === generation;
      setSignals((old) => ({
        ...old,
        sources: {
          ...old.sources,
          [name]: {
            ...old.sources?.[name],
            state: "loading",
            fetchedAt: old.sources?.[name]?.fetchedAt ?? null,
            complete: old.sources?.[name]?.complete ?? false,
            scope,
          },
        },
      }));
      void Promise.resolve()
        .then(source)
        .then((value) => {
          if (live())
            setSignals((old) => ({
              ...old,
              ...apply(value),
              sources: {
                ...old.sources,
                [name]: {
                  state:
                    name === "repository" && value === null
                      ? "not-applicable"
                      : "ready",
                  fetchedAt: new Date().toISOString(),
                  scope,
                  ...completeness(value),
                },
              },
            }));
        })
        .catch((error) => {
          if (live())
            setSignals((old) => ({
              ...old,
              sources: {
                ...old.sources,
                [name]: {
                  state: "error",
                  fetchedAt: old.sources?.[name]?.fetchedAt ?? null,
                  complete: false,
                  scope,
                  error: workbenchErrorMessage(error),
                },
              },
            }));
        });
    };
    load(
      "builds",
      "Build receipts in this project; document identity shown where known",
      () => studioClient.records(workspaceId, "build"),
      (builds) => ({ builds }),
    );
    load(
      "responses",
      "Response records in this project",
      () => studioClient.records(workspaceId, "response"),
      (responses) => ({ responses }),
    );
    load(
      "automations",
      "Automation activity in project conversations",
      async () => {
        const tasks = await taskClient.projectActivity(sessions.current),
          own = new Set(sessions.current);
        return tasks.filter(
          (task) => task.sessionId && own.has(task.sessionId),
        );
      },
      (automations) => ({ automations }),
    );
    if (root)
      load(
        "repository",
        "Local Git status; remote freshness is reported separately",
        () => repositoryClient.status(workspaceId),
        (repository) => ({ repository }),
      );
    else
      setSignals((old) => ({
        ...old,
        repository: null,
        sources: {
          ...old.sources,
          repository: {
            state: "not-applicable",
            fetchedAt: null,
            complete: true,
            scope: "No folder attached",
          },
        },
      }));
    load(
      "reviews",
      "Linked review collections across this project",
      async () => {
        const [allRuns, collections, reviewLinks] = await Promise.all([
          reviewHistoryClient.list(),
          reviewCollectionClient.list(),
          listProjectReviewLinks(workspaceId),
        ]);
        const runs = reviewsForProject(allRuns, root, reviewLinks),
          ids = new Set(runs.map((run) => run.run_id));
        const linked = collections.projects.filter((collection) =>
          collection.run_ids.some((id) => ids.has(id)),
        );
        const ledgers: NonNullable<ProjectSignals["ledgers"]> = [],
          failures: string[] = [];
        // Read every linked collection, with bounded concurrency rather than a silent cap.
        for (let start = 0; start < linked.length; start += 3) {
          const batch = linked.slice(start, start + 3);
          const results = await Promise.allSettled(
            batch.map((collection) =>
              reviewCollectionClient.syncLedger({ projectId: collection.id }),
            ),
          );
          results.forEach((result, index) => {
            if (result.status === "fulfilled") ledgers.push(result.value);
            else
              failures.push(
                `${batch[index].name}: ${workbenchErrorMessage(result.reason)}`,
              );
          });
        }
        return {
          runs,
          ledgers,
          reviewLinks,
          failed: failures.length,
          total: linked.length,
          warnings: collections.warnings ?? [],
          failures,
        };
      },
      ({ runs, ledgers, reviewLinks }) => ({ runs, ledgers, reviewLinks }),
      (value) => ({
        complete: !value.failed && !value.warnings.length,
        error:
          value.failed || value.warnings.length
            ? `${value.total - value.failed} of ${value.total} collections loaded. ${value.failures.join("; ")}${value.warnings.length ? " Collection listing has warnings." : ""}`
            : undefined,
      }),
    );
  }, [enabled, workspaceId, root, request, suppliedSignals]);
  const visible = suppliedSignals ?? signals;
  return {
    signals: visible,
    loading: Object.values(visible.sources ?? {}).some(
      (source) => source.state === "loading",
    ),
    reload: (source?: SignalSource) =>
      setRequest((old) => ({ id: old.id + 1, source })),
    checkRemote: async () => {
      if (suppliedSignals) return;
      const key = currentKey;
      try {
        const repository = await repositoryClient.fetch(workspaceId);
        if (mounted.current && keyRef.current === key)
          setSignals((old) => ({ ...old, repository }));
      } catch (cause) {
        throw new Error(workbenchErrorMessage(cause));
      }
    },
  };
}

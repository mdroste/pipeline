// The project's Reviews section: the state of this project's documents under
// review — issue ledgers first, then the runs — so "what's outstanding on the
// draft" lives with the project instead of a separate "collections" page.

import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Project, ProjectsResponse, RunSummary } from "../../lib/types";
import { router } from "../../lib/router";
import { underRoot } from "../../lib/projectOverview";
import ProjectIssueLedgerPanel from "../ProjectIssueLedgerPanel";
import EmptyState from "../../ui/EmptyState";
import Spinner from "../../ui/Spinner";
import Button from "../../ui/Button";
import { button, muted } from "../../ui/classes";

export default function ProjectReviews({
  workspaceRoot,
}: {
  workspaceRoot: string | null;
}) {
  const [runs, setRuns] = useState<RunSummary[] | null>(null);
  const [collections, setCollections] = useState<Project[]>([]);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let stale = false;
    void Promise.all([
      invoke<RunSummary[]>("list_runs"),
      invoke<ProjectsResponse>("list_projects"),
    ])
      .then(([loadedRuns, loadedCollections]) => {
        if (stale) return;
        setRuns(loadedRuns);
        setCollections(loadedCollections.projects);
        setError(null);
      })
      .catch((caught) => {
        if (!stale) setError(String(caught));
      });
    return () => {
      stale = true;
    };
  }, []);
  const projectRuns = useMemo(
    () =>
      workspaceRoot && runs
        ? runs.filter((run) => underRoot(run.input_path, workspaceRoot))
        : [],
    [runs, workspaceRoot],
  );
  const projectRunIds = useMemo(
    () => new Set(projectRuns.map((run) => run.run_id)),
    [projectRuns],
  );
  const linked = collections.filter((collection) =>
    collection.run_ids.some((id) => projectRunIds.has(id)),
  );
  const unfiled = collections.filter(
    (collection) => !collection.run_ids.some((id) => projectRunIds.has(id)),
  );
  const openRun = (runId: string) =>
    void router.navigate({ page: "history", runId });

  if (error)
    return (
      <p role="alert" className="text-sm text-red-600 dark:text-red-400">
        Reviews could not be loaded: {error}
      </p>
    );
  if (!runs) return <Spinner label="Loading reviews…" />;
  return (
    <div className="mx-auto w-full max-w-4xl space-y-8">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h2 className="text-lg font-semibold">Reviews</h2>
          <p className={`${muted} mt-1`}>
            {workspaceRoot
              ? "Reviews of documents in this project's folder, and the findings ledger that tracks them across drafts."
              : "Attach a folder to this project to connect its documents' reviews here."}
          </p>
        </div>
        <Button
          variant="primary"
          onClick={() => void router.navigate({ page: "main" })}
        >
          Start a review
        </Button>
      </header>

      {linked.map((collection) => (
        <section key={collection.id} aria-label={`${collection.name} findings`}>
          <ProjectIssueLedgerPanel
            project={collection}
            runs={runs}
            onOpenRun={openRun}
          />
        </section>
      ))}

      <section aria-label="Review runs for this project">
        <h3 className="text-sm font-semibold">Runs</h3>
        {projectRuns.length === 0 ? (
          <EmptyState
            title="No reviews of this project's documents yet."
            description={
              workspaceRoot
                ? "Run a review of the manuscript to see its findings tracked here."
                : undefined
            }
            action={
              <Button onClick={() => void router.navigate({ page: "main" })}>
                Start a review
              </Button>
            }
          />
        ) : (
          <div className="mt-2 overflow-hidden rounded-xl border border-gray-200 dark:border-gray-800">
            {projectRuns.map((run, index) => (
              <button
                key={run.run_id}
                type="button"
                onClick={() => openRun(run.run_id)}
                className={`flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-gray-50 dark:hover:bg-gray-900 ${index ? "border-t border-gray-200 dark:border-gray-800" : ""}`}
              >
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">
                    {run.title || run.input_name}
                  </span>
                  <span className={`${muted} mt-0.5 block`}>
                    {run.profile_name} ·{" "}
                    {new Date(run.created).toLocaleDateString([], {
                      month: "short",
                      day: "numeric",
                    })}
                  </span>
                </span>
                <span
                  className={`shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium ${
                    run.status === "complete"
                      ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300"
                      : "bg-amber-50 text-amber-700 dark:bg-amber-950/40 dark:text-amber-300"
                  }`}
                >
                  {run.status}
                </span>
              </button>
            ))}
          </div>
        )}
      </section>

      {unfiled.length > 0 && (
        <section aria-label="Unfiled review collections">
          <h3 className="text-sm font-semibold">Other review collections</h3>
          <p className={`${muted} mt-1`}>
            Collections whose runs are not part of this project.
          </p>
          <div className="mt-2 flex flex-wrap gap-2">
            {unfiled.map((collection) => (
              <button
                key={collection.id}
                type="button"
                className={button}
                onClick={() => void router.navigate({ page: "projects" })}
              >
                {collection.name}
              </button>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

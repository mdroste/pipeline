import type { PaperWithRevision } from "../../lib/workbench/researchTypes";
// The project's Reviews section: the state of this project's documents under
// review — issue ledgers first, then the runs — so "what's outstanding on the
// draft" lives with the project instead of a separate "collections" page.

import { useEffect, useMemo, useState } from "react";
import { reviewCollectionClient } from "../../lib/reviewCollectionClient";
import { reviewHistoryClient } from "../../lib/reviewHistoryClient";
import type { Project } from "../../lib/reviewCollectionTypes";
import type { RunSummary } from "../../lib/reviewTypes";
import type { ArtifactSelectionTarget } from "../../lib/artifactTypes";
import { router } from "../../lib/router";
import { useOpenHistoryRun } from "../../hooks/app/historyRunOpener";
import {
  reviewsForProject,
  reviewDocumentId,
  type ProjectReviewLink,
} from "../../lib/project-overview/projectPaths";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { listProjectReviewLinks } from "../../lib/workbench/releaseClient";
import ProjectIssueLedgerPanel from "../ProjectIssueLedgerPanel";
import EmptyState from "../../ui/EmptyState";
import Spinner from "../../ui/Spinner";
import Button from "../../ui/Button";
import { button, muted } from "../../ui/classes";
import { listedRunStatus, runFinished } from "../../lib/reportPresentation";
import ReportIssueLink from "../ReportIssueLink";

export default function ProjectReviews({
  workspaceId,
  workspaceRoot,
  workspaceName = "This project",
  onChanged,
  documents = [],
}: {
  documents?: PaperWithRevision[];
  workspaceId?: string;
  workspaceRoot: string | null;
  workspaceName?: string;
  /** Findings are counted on the Overview too. */
  onChanged?: () => void;
}) {
  const [runs, setRuns] = useState<RunSummary[] | null>(null);
  const [links, setLinks] = useState<ProjectReviewLink[]>([]);
  const [collections, setCollections] = useState<Project[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [documentFilter, setDocumentFilter] = useState("");
  const [tracking, setTracking] = useState(false);
  const [trackError, setTrackError] = useState<string | null>(null);
  const openHistoryRun = useOpenHistoryRun();
  useEffect(() => {
    let stale = false;
    void Promise.all([
      reviewHistoryClient.list(),
      reviewCollectionClient.list(),
      workspaceId ? listProjectReviewLinks(workspaceId) : Promise.resolve([]),
    ])
      .then(([loadedRuns, loadedCollections, handoffs]) => {
        if (stale) return;
        setRuns(loadedRuns);
        setLinks(handoffs);
        setCollections(loadedCollections.projects);
        setError(null);
      })
      .catch((caught) => {
        if (!stale) setError(workbenchErrorMessage(caught));
      });
    return () => {
      stale = true;
    };
  }, [workspaceId]);
  const allProjectRuns = useMemo(
    () => reviewsForProject(runs ?? [], workspaceRoot, links),
    [runs, workspaceRoot, links],
  );
  const projectRuns = allProjectRuns.filter(
    (run) => !documentFilter || reviewDocumentId(run, links) === documentFilter,
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
  // Findings are followed through a review collection. Keep that detail out of
  // the researcher's way: one action files this project's finished reviews.
  const untracked = projectRuns.filter(
    (run) =>
      runFinished(run.status) &&
      !linked.some((collection) => collection.run_ids.includes(run.run_id)),
  );
  const trackFindings = async () => {
    setTracking(true);
    setTrackError(null);
    try {
      const groups = new Map<string | null, RunSummary[]>();
      for (const run of untracked) {
        const documentId = reviewDocumentId(run, links);
        groups.set(documentId, [...(groups.get(documentId) ?? []), run]);
      }
      for (const [documentId, group] of groups) {
        // Never append deck findings to a collection that also contains paper reviews.
        let collection =
          linked.find(
            (item) =>
              item.run_ids.length > 0 &&
              item.run_ids.every((id) => {
                const run = allProjectRuns.find((run) => run.run_id === id);
                return run && reviewDocumentId(run, links) === documentId;
              }),
          ) ??
          (await reviewCollectionClient.create({
            name: `${documents.find((item) => item.paper.id === documentId)?.paper.title ?? workspaceName} findings`,
            description: null,
          }));
        for (const run of group)
          collection = await reviewCollectionClient.setRun({
            projectId: collection.id,
            runId: run.run_id,
            included: true,
          });
        const saved = collection;
        setCollections((current) => [
          saved,
          ...current.filter((item) => item.id !== saved.id),
        ]);
      }
      onChanged?.();
    } catch (caught) {
      setTrackError(workbenchErrorMessage(caught));
    } finally {
      setTracking(false);
    }
  };
  // Evidence chips carry the cited source; the app shell opens the run there.
  const openRun = (runId: string, source?: ArtifactSelectionTarget) =>
    openHistoryRun(runId, source ?? null);

  if (error)
    return (
      <p role="alert" className="text-sm text-danger">
        Reviews could not be loaded: {error}
        <ReportIssueLink />
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
              ? "See reviews of this project’s documents and track findings across drafts."
              : "Reviews started from papers in this project appear here."}
          </p>
        </div>
        <Button
          variant="primary"
          onClick={() => void router.navigate({ page: "main" })}
        >
          Start a review
        </Button>
      </header>

      {documents.length > 1 && (
        <label className="block text-sm">
          Document
          <select
            aria-label="Filter reviews by document"
            className="ui-input mt-1"
            value={documentFilter}
            onChange={(e) => setDocumentFilter(e.target.value)}
          >
            <option value="">All documents</option>
            {documents.map(({ paper }) => (
              <option key={paper.id} value={paper.id}>
                {paper.title}
              </option>
            ))}
          </select>
        </label>
      )}
      {untracked.length > 0 && (
        <section aria-label="Track findings" className="space-y-2">
          <p className={muted}>
            {linked.length > 0
              ? `${untracked.length} ${untracked.length === 1 ? "review isn’t" : "reviews aren’t"} in Findings across reviews yet.`
              : "Follow each finding from one report to the next, and see open findings on the Overview."}
          </p>
          <Button disabled={tracking} onClick={() => void trackFindings()}>
            Track findings
          </Button>
          {trackError && (
            <p role="alert" className="text-sm text-danger">
              {trackError}
              <ReportIssueLink />
            </p>
          )}
        </section>
      )}

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
                ? "Run a review of the manuscript, then choose Track findings to follow its findings here."
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
                  className={`shrink-0 rounded-full px-2 py-0.5 text-[0.6875rem] font-medium ${
                    runFinished(run.status)
                      ? "bg-success-soft text-success"
                      : "bg-warning-soft text-warning"
                  }`}
                >
                  {listedRunStatus(run.status)}
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
                onClick={() =>
                  void router.navigate({
                    page: "projects",
                    collectionId: collection.id,
                  })
                }
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

import type { ReactNode } from "react";
import PipelineProgress from "./PipelineProgress";
import { Button, EmptyState, Spinner } from "../ui";
import type {
  PassTiming,
  PipelineState,
  ReviewRoutingSummary,
  RuntimeStage,
} from "../lib/reviewRuntimeTypes";
import type { ExecutionPlanStage } from "../lib/pipelineHelpers";
import type { QueuedRun } from "../lib/types";
import { queuedExplanation } from "../lib/reviewRuns";

interface Props {
  /** The queue entry; null when the review is no longer listed. */
  run: QueuedRun | null;
  runs: QueuedRun[];
  limit: number;
  state: PipelineState;
  plan: ExecutionPlanStage[] | null;
  stageHistory: RuntimeStage[];
  runStartedAt: number | null;
  passTimes: Record<string, PassTiming>;
  reviewRouting: ReviewRoutingSummary | null;
  onCancel: () => void;
  onNewReview: () => void;
  onOpenHistory: (runId?: string) => void;
}

/** Centers short content; content taller than the view starts at its top so
 *  the surrounding scroller can reach all of it. */
function Centered({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-full justify-center px-8 py-12">
      <div data-testid="review-run-content" className="my-auto w-full max-w-sm">
        {children}
      </div>
    </div>
  );
}

/** One review from the queue that has no report on screen yet: waiting,
 *  running, failed, cancelled, or finished with its report loading. */
export default function ReviewRunView({
  run,
  runs,
  limit,
  state,
  plan,
  stageHistory,
  runStartedAt,
  passTimes,
  reviewRouting,
  onCancel,
  onNewReview,
  onOpenHistory,
}: Props) {
  if (!run) {
    return (
      <EmptyState
        title="This review is no longer listed."
        description="Reviews leave the list when they are cleared or after Pipeline restarts. Finished reports stay in Review history."
        action={
          <div className="flex gap-2">
            <Button onClick={() => onOpenHistory()}>Open review history</Button>
            <Button variant="primary" onClick={onNewReview}>
              New review
            </Button>
          </div>
        }
      />
    );
  }

  if (run.status === "queued") {
    return (
      <Centered>
        <div className="flex flex-col items-center text-center">
          <span
            aria-hidden="true"
            className="h-9 w-9 rounded-full border-2 border-dashed border-gray-300 dark:border-neutral-600"
          />
          <h2 className="mt-4 text-base font-semibold text-gray-900 dark:text-neutral-100">
            {run.label} is queued
          </h2>
          <p className="mt-2 text-sm leading-6 text-gray-500 dark:text-neutral-400">
            {queuedExplanation(run, runs, limit)} You can keep working or set up
            another review; this one stays in Activity until it finishes.
          </p>
          <p className="mt-1 text-xs text-gray-400 dark:text-neutral-500">
            Workflow: {run.profileName || "Active workflow"}
          </p>
          <div className="mt-5 flex gap-2">
            <Button onClick={onCancel}>Remove from queue</Button>
            <Button variant="primary" onClick={onNewReview}>
              Set up another review
            </Button>
          </div>
        </div>
      </Centered>
    );
  }

  if (run.status === "done") {
    if (run.resultAvailable)
      return <Spinner label={`Loading the report for ${run.label}…`} />;
    return (
      <EmptyState
        title={`${run.label} finished.`}
        description="Its report is saved in Review history."
        action={
          <Button
            variant="primary"
            onClick={() => onOpenHistory(run.runId ?? undefined)}
          >
            Open report
          </Button>
        }
      />
    );
  }

  const failed = state.kind === "error";
  // A run known only from its queue entry (after a reload) has no stage
  // history; show its outcome without inventing which stages finished.
  const showProgress = !(
    failed &&
    stageHistory.length === 0 &&
    state.failedAt === undefined
  );
  return (
    <Centered>
      <p className="mb-4 truncate text-center text-xs font-medium text-gray-500 dark:text-neutral-400">
        {run.label}
      </p>
      {showProgress && (
        <PipelineProgress
          runKey={run.key}
          state={state}
          plan={plan ?? undefined}
          stageHistory={stageHistory}
          runStartedAt={runStartedAt}
          runEndedAt={run.finishedAt}
          passTimes={passTimes}
          reviewRouting={reviewRouting}
        />
      )}
      {failed ? (
        <div className={showProgress ? "mt-6" : ""}>
          <div
            role={run.status === "cancelled" ? "status" : "alert"}
            className={
              run.status === "cancelled"
                ? "rounded-lg border border-gray-200 bg-gray-50 px-3 py-2.5 text-sm leading-5 text-gray-600 dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-300"
                : "rounded-lg border border-red-200 bg-red-50 px-3 py-2.5 text-sm leading-5 text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-300"
            }
          >
            {state.message}
          </div>
          <button
            type="button"
            onClick={onNewReview}
            className="mt-3 w-full rounded-lg border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700
                     transition-colors hover:bg-white focus-visible:outline-none focus-visible:ring-2
                     focus-visible:ring-gray-400 dark:border-neutral-700 dark:text-neutral-300 dark:hover:bg-neutral-900"
          >
            Start a new review
          </button>
        </div>
      ) : (
        <>
          <p className="mt-6 text-center text-xs leading-relaxed text-gray-500 dark:text-neutral-400">
            This may take 15–60 minutes depending on
            <br />
            paper length, number of reviewers, and provider response times.
            <br />
            It keeps running if you leave this page.
          </p>
          <button
            type="button"
            onClick={onCancel}
            className="mx-auto mt-4 block rounded-lg px-3 py-1.5 text-xs font-medium text-gray-500 transition-colors
                     hover:bg-red-50 hover:text-red-600 focus-visible:outline-none focus-visible:ring-2
                     focus-visible:ring-red-300 dark:text-neutral-400 dark:hover:bg-red-950/40 dark:hover:text-red-400"
          >
            Cancel review
          </button>
        </>
      )}
    </Centered>
  );
}

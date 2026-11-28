// Run controls beside a project script's tabs. Play runs the script with its
// saved run profile; a profile whose test has not passed runs its test first.
// Host execution stays behind its review: whenever the command or the script
// has changed since it was last authorized, Play shows the exact command and
// runs it only once the researcher authorizes it. Run… opens the full setup.

import { useEffect, useState } from "react";
import { executionClient } from "../../lib/workbench/executionClient";
import { projectClient } from "../../lib/projectClient";
import { scriptAdapter } from "../../lib/analysisSetup";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import type {
  ExecutionProfile,
  ResearchExecution,
} from "../../lib/workbench/executionTypes";
import type { HostExecutionPreview } from "../../lib/projectTypes";
import { notify } from "../DialogService";
import { Icon } from "../../ui/icons";
import OverviewDialog from "./OverviewDialog";

const ACTIVE: ReadonlySet<ResearchExecution["outcome"]> = new Set([
  "queued",
  "running",
]);
const LABELS: Record<ResearchExecution["outcome"], string> = {
  queued: "Queued…",
  running: "Running…",
  completed: "Completed",
  failed: "Failed",
  interrupted: "Stopped",
  timed_out: "Timed out",
  outcome_unknown: "Outcome unknown",
};
const POLL_MS = 1000;

/** The saved profile that runs `path`, preferring one whose test passed. */
export function profileFor(profiles: ExecutionProfile[], path: string) {
  const score = (profile: ExecutionProfile) =>
    (profile.testStatus === "passed" ? 4 : 0) +
    (profile.name === path ? 2 : 0) +
    (profile.inputs[0] === path ? 1 : 0);
  return (
    profiles
      .filter((p) => p.name === path || p.inputs.includes(path))
      .sort((a, b) => score(b) - score(a))[0] ?? null
  );
}

/** The newest run of a profile. */
function latestRun(runs: ResearchExecution[], profileId: string) {
  return (
    runs
      .filter((run) => run.profileId === profileId)
      .sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0] ?? null
  );
}

export default function ScriptRunControls({
  workspaceId,
  path,
  dirty,
  onSetup,
  onShowRun,
}: {
  workspaceId: string;
  path: string;
  /** The script has unsaved changes, so neither Play nor Run… is offered. */
  dirty: boolean;
  /** Opens the run setup for this script. */
  onSetup: (path: string) => void;
  /** Opens a run's output. */
  onShowRun: (path: string, runId: string) => void;
}) {
  const runnable = Boolean(scriptAdapter(path));
  const name = path.split("/").pop() ?? path;
  const [run, setRun] = useState<ResearchExecution | null>(null);
  const [test, setTest] = useState(false);
  const [busy, setBusy] = useState(false);
  const [review, setReview] = useState<{
    preview: HostExecutionPreview;
    profile: ExecutionProfile;
  } | null>(null);

  // The script's last run, so its outcome is still shown on coming back.
  useEffect(() => {
    if (!runnable) return;
    let live = true;
    void Promise.all([
      executionClient.listExecutionProfiles(workspaceId),
      executionClient.listExecutions(workspaceId),
    ])
      .then(([profiles, runs]) => {
        const profile = profileFor(profiles, path);
        if (live) setRun(profile ? latestRun(runs, profile.id) : null);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [runnable, workspaceId, path]);

  // Follow a run until it ends.
  const running = run !== null && ACTIVE.has(run.outcome);
  const runId = run?.id;
  useEffect(() => {
    if (!running || !runId) return;
    let live = true;
    let pending = false;
    const timer = setInterval(() => {
      if (pending) return;
      pending = true;
      void executionClient
        .listExecutions(workspaceId)
        .then((runs) => {
          const next = runs.find((r) => r.id === runId);
          if (live && next) setRun(next);
        })
        .catch(() => undefined)
        .finally(() => {
          pending = false;
        });
    }, POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [running, runId, workspaceId]);

  if (!runnable) return null;

  const attempt = async (step: () => Promise<void>) => {
    setBusy(true);
    try {
      await step();
    } catch (cause) {
      notify(workbenchErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  const launch = async (profile: ExecutionProfile) => {
    const testOnly = profile.testStatus !== "passed";
    setTest(testOnly);
    setRun(
      await executionClient.runExecution({
        profileId: profile.id,
        sessionId: null,
        testOnly,
        operationId: `play-${crypto.randomUUID()}`,
      }),
    );
  };
  const play = () =>
    attempt(async () => {
      const profile = profileFor(
        await executionClient.listExecutionProfiles(workspaceId),
        path,
      );
      if (!profile) {
        notify(`Set up how to run ${name}, then press Play.`, "info");
        onSetup(path);
        return;
      }
      const preview = await projectClient.previewExecution(profile.id);
      if (preview.authorized) await launch(profile);
      else setReview({ preview, profile });
    });
  const authorize = () => {
    const pending = review;
    if (!pending) return;
    setReview(null);
    void attempt(async () => {
      await projectClient.authorizeExecution(
        pending.profile.id,
        pending.preview.fingerprint,
      );
      await launch(pending.profile);
    });
  };
  const stop = () =>
    attempt(async () => {
      if (run) await executionClient.cancelExecution(run.id);
    });

  return (
    <>
      {running ? (
        <button
          type="button"
          className="file-play"
          data-running="true"
          aria-label={`Stop ${name}`}
          title={`Stop ${name}`}
          disabled={busy}
          onClick={() => void stop()}
        >
          <Icon name="stop" className="h-4 w-4" />
        </button>
      ) : (
        <button
          type="button"
          className="file-play"
          aria-label={`Run ${name}`}
          title={dirty ? "Save the script before running it" : `Run ${name}`}
          disabled={busy || dirty}
          onClick={() => void play()}
        >
          <Icon name="play" className="h-4 w-4" />
        </button>
      )}
      {run && (
        <button
          type="button"
          className="file-run-status"
          data-outcome={run.outcome}
          title="Show this run's output"
          onClick={() => onShowRun(path, run.id)}
        >
          {test ? `Test: ${LABELS[run.outcome]}` : LABELS[run.outcome]}
        </button>
      )}
      <button
        className="file-action"
        disabled={dirty}
        title={
          dirty
            ? "Save the script before preparing a run"
            : "Set up how this script runs"
        }
        onClick={() => onSetup(path)}
      >
        Run…
      </button>
      <OverviewDialog
        title="Review command before running"
        open={review !== null}
        onClose={() => setReview(null)}
      >
        {review && (
          <div className="space-y-2 text-xs">
            <p>{review.preview.boundary}</p>
            <p>
              Pipeline stops this command after {review.profile.timeoutSeconds}{" "}
              seconds.
            </p>
            <pre className="overflow-auto whitespace-pre-wrap text-[0.6875rem]">
              {JSON.stringify(
                {
                  command: review.preview.command,
                  cwd: review.preview.cwd,
                  inputs: review.preview.inputs,
                  launch: review.preview.launch,
                  outputs: review.preview.outputs,
                },
                null,
                2,
              )}
            </pre>
            <div className="flex gap-2">
              <button
                type="button"
                className="ui-button ui-button--primary"
                onClick={authorize}
              >
                Authorize this command and{" "}
                {review.profile.testStatus === "passed" ? "run" : "test"}
              </button>
              <button
                type="button"
                className="ui-button"
                onClick={() => setReview(null)}
              >
                Cancel
              </button>
            </div>
          </div>
        )}
      </OverviewDialog>
    </>
  );
}

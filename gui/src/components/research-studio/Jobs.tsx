import { useEffect, useRef, useState } from "react";
import {
  projectClient,
  type HostExecutionPreview,
} from "../../lib/projectClient";
import { studioClient, type JobStatus } from "../../lib/studioClient";
import { executionClient } from "../../lib/workbench/executionClient";
import {
  STATA_CLEANUP_NOTICE,
  stataUsesLauncher,
} from "../../lib/workbench/toolchains";
import type {
  ExecutionProfile,
  ResearchExecution,
} from "../../lib/workbenchTypes";
import { button, panel, ErrorNotice, Inspect, useAction } from "./shared";
import JobLog, { ACTIVE_JOB_STATES } from "./JobLog";

const ACTIVE_POLL_MS = 1500;
const RUNNING_POLL_MS = 5000;
const IDLE_POLL_MS = 10_000;
export function JobLauncher({
  profile,
  onQueued,
}: {
  profile: ExecutionProfile;
  onQueued?: (job: ResearchExecution) => void;
}) {
  const [preview, setPreview] = useState<HostExecutionPreview | null>(null);
  const { error, busy, run } = useAction();
  useEffect(() => setPreview(null), [profile.id, profile.revision]);
  return (
    <div className="space-y-2">
      <button
        className={button}
        disabled={busy}
        onClick={() =>
          void run(async () =>
            setPreview(await projectClient.previewExecution(profile.id)),
          )
        }
      >
        Review {profile.testStatus === "passed" ? "run" : "test run"}
      </button>
      <ErrorNotice error={error} />
      {preview && (
        <div className={panel}>
          <p className="text-sm font-medium">
            Run locally while Pipeline stays open
          </p>
          <pre className="whitespace-pre-wrap break-words text-xs">
            {preview.command.join(" ")}
          </pre>
          <p className="break-all text-xs">Working directory: {preview.cwd}</p>
          <p className="text-xs">{preview.boundary}</p>
          <Inspect
            value={{
              inputs: preview.inputs,
              launch: preview.launch,
              outputs: preview.outputs,
            }}
            label="Declared inputs, launcher and expected outputs"
          />
          <p className="text-xs">
            This job keeps running if you stop the assistant. Use Stop job to
            cancel it. Quitting Pipeline stops all local jobs.
          </p>
          <button
            className={button}
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await projectClient.authorizeExecution(
                  profile.id,
                  preview.fingerprint,
                );
                const e = await executionClient.runExecution({
                  profileId: profile.id,
                  sessionId: null,
                  testOnly: profile.testStatus !== "passed",
                  operationId: `job-${crypto.randomUUID()}`,
                });
                setPreview(null);
                onQueued?.(e);
              })
            }
          >
            Authorize and start{" "}
            {profile.testStatus === "passed" ? "job" : "test"}
          </button>
        </div>
      )}
    </div>
  );
}
export default function Jobs({
  workspaceId,
  onCompleted,
  visible = true,
}: {
  workspaceId: string;
  onCompleted: () => void;
  visible?: boolean;
}) {
  const [jobs, setJobs] = useState<JobStatus[]>([]);
  const [selected, setSelected] = useState("");
  const [open, setOpen] = useState(false);
  const { error, run, setError } = useAction();
  // Survives hiding, so a job that ended meanwhile still triggers a refresh.
  const completed = useRef<{ workspaceId: string; key: string } | null>(null);
  const callback = useRef(onCompleted);
  callback.current = onCompleted;
  useEffect(() => {
    if (!visible) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    let running = false;
    const refresh = async () => {
      try {
        const next = await studioClient.jobs(workspaceId);
        if (disposed) return;
        setJobs(next);
        running = next.some((j) => ACTIVE_JOB_STATES.includes(j.state));
        const key = next.map((j) => `${j.execution.id}:${j.state}`).join("|");
        const previous = completed.current;
        if (previous?.workspaceId === workspaceId && key !== previous.key) {
          callback.current();
        }
        completed.current = { workspaceId, key };
      } catch (e) {
        if (!disposed) setError(String(e));
      }
      // Collapsed, check less often: enough to notice a job ending or a
      // newly queued one.
      const delay = open
        ? ACTIVE_POLL_MS
        : running
          ? RUNNING_POLL_MS
          : IDLE_POLL_MS;
      if (!disposed) timer = setTimeout(() => void refresh(), delay);
    };
    void refresh();
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [visible, open, workspaceId, setError]);
  const active = jobs.filter((j) => ACTIVE_JOB_STATES.includes(j.state));
  return (
    <details
      className={`${panel} bg-gray-50/60 dark:bg-neutral-900/30`}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary className="cursor-pointer text-sm font-semibold">
        Local jobs · {active.length} active · {jobs.length} retained
      </summary>
      <ErrorNotice error={error} />
      <p className="text-xs text-gray-500">
        Up to two jobs can run at once. Jobs that would write to the same
        project cannot run together. Keep Pipeline open; interrupted jobs do not
        restart automatically.
      </p>
      <div className="max-h-64 space-y-2 overflow-auto">
        {jobs.map((j) => (
          <div
            key={j.execution.id}
            className="flex flex-wrap items-center gap-2 border-b py-2 text-xs"
          >
            <button
              className="min-w-0 flex-1 truncate text-left"
              onClick={() => setSelected(j.execution.id)}
            >
              {j.execution.command.join(" ")}
            </button>
            <span>
              {j.state} · {j.owner} · {j.durationSeconds ?? 0}s
            </span>
            {["queued", "running"].includes(j.state) && (
              <button
                className={button}
                onClick={() =>
                  void run(() =>
                    executionClient.cancelExecution(j.execution.id),
                  )
                }
              >
                Stop job
              </button>
            )}
            {j.state === "cancelling" && (
              <span>
                {j.execution.adapter === "stata" &&
                stataUsesLauncher(j.execution.command)
                  ? "Allowing the Stata launcher to exit (up to 30s)"
                  : "Stopping descendants…"}
              </span>
            )}
            {j.canReconcile && (
              <button
                className={button}
                onClick={() =>
                  void run(() =>
                    studioClient.reconcile(workspaceId, j.execution.id),
                  )
                }
              >
                Reconcile adopted outputs
              </button>
            )}
            {j.execution.inputManifest.snapshotConsistency ===
              "captured_inputs_verified" && (
              <div className="w-full space-y-1 text-gray-600 dark:text-gray-400">
                <p>
                  Saved inputs checked · listed dependencies · runs with your
                  computer’s permissions
                </p>
                <Inspect
                  label="Run details and saved inputs"
                  value={{
                    executionId: j.execution.id,
                    planId: j.execution.inputManifest.planId,
                    workingDirectory: j.execution.cwd,
                    inputs: j.execution.inputManifest,
                    outputs: j.execution.outputManifest,
                    validation: j.execution.validation,
                  }}
                />
              </div>
            )}
            {j.execution.validation.stataCleanupRequired === true && (
              <p role="alert" className="w-full text-red-700">
                {STATA_CLEANUP_NOTICE}
              </p>
            )}
          </div>
        ))}
      </div>
      {selected && (
        <>
          <JobLog
            workspaceId={workspaceId}
            executionId={selected}
            job={jobs.find((j) => j.execution.id === selected)}
            live={visible && open}
            onError={setError}
          />
          <Inspect
            value={jobs.find((j) => j.execution.id === selected)}
            label="Run details"
          />
        </>
      )}
    </details>
  );
}

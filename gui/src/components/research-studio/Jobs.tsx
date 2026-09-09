import { useEffect, useRef, useState } from "react";
import {
  projectClient,
  type HostExecutionPreview,
} from "../../lib/projectClient";
import { studioClient, type JobStatus } from "../../lib/studioClient";
import { workbenchClient } from "../../lib/workbenchClient";
import type {
  ExecutionProfile,
  ResearchExecution,
} from "../../lib/workbenchTypes";
import { button, panel, ErrorNotice, Inspect, useAction } from "./shared";
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
            This detached job continues if a research turn stops. Use Stop job
            below to cancel it. Quitting Pipeline stops all local jobs.
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
                const e = await workbenchClient.runExecution({
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
  const [stream, setStream] = useState("stdout");
  const [log, setLog] = useState("");
  const [truncated, setTruncated] = useState(false);
  const { error, run, setError } = useAction();
  const completed = useRef<string | null>(null);
  const callback = useRef(onCompleted);
  callback.current = onCompleted;
  useEffect(() => {
    if (!visible) return;
    completed.current = null;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      try {
        const next = await studioClient.jobs(workspaceId);
        if (disposed) return;
        setJobs(next);
        const key = next.map((j) => `${j.execution.id}:${j.state}`).join("|");
        if (completed.current !== null && key !== completed.current) {
          callback.current();
        }
        completed.current = key;
      } catch (e) {
        if (!disposed) setError(String(e));
      }
      if (!disposed) timer = setTimeout(() => void refresh(), 1500);
    };
    void refresh();
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [visible, workspaceId, setError]);
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    let offset = 0;
    setLog("");
    setTruncated(false);
    if (!selected || !visible) return;
    const read = async () => {
      try {
        const page = await studioClient.log(
          workspaceId,
          selected,
          stream,
          offset,
        );
        if (disposed) return;
        offset = page.nextOffset;
        setLog((old) => (old + page.text).slice(-200000));
        setTruncated((old) => old || page.truncated || offset > 200000);
      } catch (e) {
        if (!disposed) setError(String(e));
      }
      if (!disposed) timer = setTimeout(() => void read(), 750);
    };
    void read();
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [visible, workspaceId, selected, stream, setError]);
  const active = jobs.filter((j) =>
    ["queued", "running", "cancelling"].includes(j.state),
  );
  return (
    <details className={`${panel} bg-gray-50/60 dark:bg-neutral-900/30`}>
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
                    workbenchClient.cancelExecution(j.execution.id),
                  )
                }
              >
                Stop job
              </button>
            )}
            {j.state === "cancelling" && (
              <span>
                {j.execution.adapter === "stata"
                  ? "Allowing oldstata cleanup (up to 30s)"
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
                  Verified captured inputs · declared dependencies · host access
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
                Verify that Legacy Time Off has completed before continuing.
              </p>
            )}
          </div>
        ))}
      </div>
      {selected && (
        <>
          <div className="flex gap-2">
            {["stdout", "stderr"].map((s) => (
              <button
                className={button}
                key={s}
                aria-pressed={stream === s}
                onClick={() => setStream(s)}
              >
                {s}
              </button>
            ))}
          </div>
          <pre
            aria-label="Job log"
            className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded bg-neutral-950 p-3 text-xs text-neutral-100"
          >
            {log || "No output yet."}
          </pre>
          {truncated && (
            <p className="text-xs">
              Log limit reached. Up to 4 MiB is saved for each output stream.
            </p>
          )}
          <Inspect
            value={jobs.find((j) => j.execution.id === selected)}
            label="Run details"
          />
        </>
      )}
    </details>
  );
}

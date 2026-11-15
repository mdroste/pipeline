// Only an eligible conversation file mounts a run target. The native runner
// keeps the latest output while the researcher switches files or closes the pane.
import { useEffect, useMemo, useRef, useState } from "react";
import type { FilePreview } from "../../lib/fileWorkspaceClient";
import {
  conversationClient,
  type ArtifactRun,
} from "../../lib/workbench/conversationClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { confirmDialog } from "../DialogService";

const labels: Record<ArtifactRun["state"], string> = {
  running: "Running…",
  stopping: "Stopping…",
  completed: "Completed",
  failed: "Run failed",
  interrupted: "Stopped",
  timed_out: "Timed out",
  outcome_unknown: "Run outcome unavailable",
};
const isActive = (run: ArtifactRun | null) =>
  run?.state === "running" || run?.state === "stopping";

export function useArtifactRun(
  sessionId: string | undefined,
  file: FilePreview,
) {
  const kind = /\.py$/i.test(file.path)
    ? "Python script"
    : /\.do$/i.test(file.path)
      ? "Stata do-file"
      : null;
  const target = useMemo(
    () =>
      sessionId && kind && file.scopeRoot && file.editable
        ? { sessionId, scopeRoot: file.scopeRoot, path: file.path }
        : null,
    [sessionId, kind, file.scopeRoot, file.path, file.editable],
  );
  const [run, setRun] = useState<ArtifactRun | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const runId = run?.id;
  useEffect(() => {
    if (!target) return;
    let current = true;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      try {
        const next = await conversationClient.artifactRunStatus(target);
        if (!current) return;
        setRun(next);
        if (isActive(next)) timer = setTimeout(() => void refresh(), 750);
      } catch (cause) {
        if (!current) return;
        if (runId) {
          setError(workbenchErrorMessage(cause));
          timer = setTimeout(() => void refresh(), 1500);
        }
      }
    };
    void refresh();
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [target, runId]);
  const start = async () => {
    if (!target || busy) return;
    setBusy(true);
    setError("");
    try {
      const approved = await confirmDialog(
        `Run ${target.path} in ${target.scopeRoot}? The script runs with your local account's file access.`,
        { title: `Run ${kind}`, confirmLabel: "Run" },
      );
      if (approved && mounted.current)
        setRun(await conversationClient.runArtifact(target, file.hash));
    } catch (cause) {
      setError(workbenchErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  const stop = async () => {
    if (!target || !run) return;
    try {
      await conversationClient.stopArtifactRun(target, run.id);
      setRun((current) =>
        current?.id === run.id && isActive(current)
          ? { ...current, state: "stopping" }
          : current,
      );
    } catch (cause) {
      setError(workbenchErrorMessage(cause));
    }
  };
  return {
    enabled: Boolean(target),
    run,
    active: isActive(run),
    error,
    busy,
    start,
    stop,
  };
}

export function ArtifactRunOutput({
  run,
  error,
}: {
  run: ArtifactRun | null;
  error: string;
}) {
  if (!run && !error) return null;
  return (
    <section className="file-run-output" aria-label="Run output">
      <div className="file-run-heading">
        <strong>Run output</strong>
        {run && (
          <span role="status">
            {labels[run.state]}
            {run.exitStatus !== null && run.exitStatus !== 0
              ? ` · Exit code ${run.exitStatus}`
              : ""}
          </span>
        )}
      </div>
      {error && <p role="alert">{error}</p>}
      {run?.truncated && <p>Output shortened.</p>}
      {run?.stdout && <pre aria-label="Output">{run.stdout}</pre>}
      {run?.stderr && <pre aria-label="Errors">{run.stderr}</pre>}
      {run?.state === "completed" && !run.stdout && !run.stderr && (
        <p>No output.</p>
      )}
    </section>
  );
}

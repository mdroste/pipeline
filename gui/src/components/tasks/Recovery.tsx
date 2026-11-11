import { useState } from "react";
import type { TaskRun } from "../../lib/taskTypes";
import { connections } from "../../lib/connections";

export default function Recovery({
  run,
  busy,
  onControl,
}: {
  run: TaskRun;
  busy: boolean;
  onControl: (action: string) => Promise<void>;
}) {
  const [preview, setPreview] = useState<"retry" | "context" | null>(null);
  const unfinished = Object.values(run.progress.receipts).filter((r) =>
    ["unknown", "failed"].includes(r.state),
  );
  const expired = run.deadlineAt <= Date.now() / 1000;
  return (
    <section className="task-attention" aria-label="Recover automation">
      <h3>Check the saved results first</h3>
      <p>
        Pipeline will look for results from the interrupted work and continue if
        every action is resolved.
      </p>
      <button
        type="button"
        className="task-primary"
        disabled={busy}
        onClick={() => void onControl("reconcile")}
      >
        Check saved results and update status
      </button>
      {!!unfinished.length && (
        <>
          <h4>Actions to resolve</h4>
          <ul>
            {unfinished.map((r) => (
              <li key={r.operation}>
                <strong>{r.label}</strong>:{" "}
                {r.state === "unknown"
                  ? "The result was not recorded. This action may have changed files or sent a request."
                  : r.error || "This action failed."}
              </li>
            ))}
          </ul>
        </>
      )}
      <details>
        <summary>More recovery options</summary>
        <div className="task-actions">
          <button
            type="button"
            disabled={busy || (!unfinished.length && !expired)}
            onClick={() => setPreview("retry")}
          >
            Review retry
          </button>
          {run.scope.sessionId && (
            <button
              type="button"
              disabled={busy}
              onClick={() => setPreview("context")}
            >
              Review conversation update
            </button>
          )}
        </div>
        {preview === "retry" && (
          <div aria-label="Retry preview">
            <h4>Retry unfinished actions</h4>
            {unfinished.length ? (
              <>
                <p>These actions will run again:</p>
                <ul>
                  {unfinished.map((r) => (
                    <li key={r.operation}>{r.label}</li>
                  ))}
                </ul>
                <p>
                  An action with an unrecorded result may repeat a file change
                  or request. Check its output and the conversation before
                  retrying.
                </p>
              </>
            ) : (
              <p>The automation will continue from where it stopped.</p>
            )}
            <p>
              Completed results are kept. Remaining steps run after these
              actions finish.
            </p>
            {expired && (
              <p>
                This starts a new {run.chain.limits.deadlineHours}-hour
                deadline.
              </p>
            )}
            <button
              type="button"
              disabled={busy}
              onClick={() => void onControl("retry")}
            >
              Retry unfinished actions
            </button>
            <button type="button" onClick={() => setPreview(null)}>
              Cancel
            </button>
          </div>
        )}
        {preview === "context" && (
          <div aria-label="Conversation update preview">
            <h4>Use the conversation as it is now</h4>
            <p>
              This replaces the saved conversation position, connection, folder,
              tools, permissions, and check definitions with their current
              settings. Review designs and completed results stay the same.
            </p>
            <dl>
              <dt>Saved conversation</dt>
              <dd>
                {run.scope.conversationTitle ||
                  "This automation’s conversation"}
              </dd>
              <dt>Saved folder</dt>
              <dd>{run.scope.runtimeRoot || "No folder"}</dd>
              <dt>Saved connection</dt>
              <dd>
                {connections.find((c) => c.id === run.scope.connectionId)
                  ?.label || "Not recorded"}
              </dd>
            </dl>
            <p>
              The automation stays stopped. Check saved results or retry after
              updating.
            </p>
            <button
              type="button"
              disabled={busy}
              onClick={() => void onControl("refreshContext")}
            >
              Use current conversation context
            </button>
            <button type="button" onClick={() => setPreview(null)}>
              Cancel
            </button>
          </div>
        )}
      </details>
    </section>
  );
}

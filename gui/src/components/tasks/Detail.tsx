import { useEffect, useRef, useState } from "react";
import {
  stateLabel,
  taskClient,
  taskTime,
  type Json,
  type Receipt,
  type TaskEvent,
  type TaskRun,
} from "../../lib/taskClient";
import { Outline, Status } from "./shared";

function Output({
  receipt,
  runId,
  onError,
}: {
  receipt: Receipt;
  runId: string;
  onError: (error: string) => void;
}) {
  const [full, setFull] = useState<Json>(null);
  const v = full ?? receipt.output;
  const object = v && typeof v === "object" && !Array.isArray(v) ? v : null;
  const result =
    object?.result &&
    typeof object.result === "object" &&
    !Array.isArray(object.result)
      ? object.result
      : object;
  async function exportPaper() {
    try {
      await taskClient.artifact(runId, receipt.address, receipt.operation);
    } catch (e) {
      onError(String(e));
    }
  }
  return (
    <>
      {result &&
        (result.kind === "artifact" ||
          result.kind === "artifactTree" ||
          result.reviewedArtifact) && (
          <button onClick={() => void exportPaper()}>Save paper…</button>
        )}
      {result?.runId && (
        <p className="task-muted">Review run: {String(result.runId)}</p>
      )}
      {result?.highPriorityCount !== undefined && (
        <p>
          {String(result.highPriorityCount)} high-priority findings ·{" "}
          {result.complete === true
            ? "Complete review"
            : "Review has limitations"}
        </p>
      )}
      <details
        onToggle={(e) => {
          if (e.currentTarget.open && full === null)
            void taskClient
              .stepOutput(runId, receipt.address, receipt.operation)
              .then(setFull)
              .catch((e) => onError(String(e)));
        }}
      >
        <summary>Output</summary>
        <pre className="task-result">
          {typeof v === "string" ? v : JSON.stringify(v, null, 2)}
        </pre>
      </details>
    </>
  );
}
function InputForm({
  run,
  receipt,
  onUpdated,
  onError,
}: {
  run: TaskRun;
  receipt: Receipt;
  onUpdated: (run: TaskRun) => void;
  onError: (error: string) => void;
}) {
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const op = useRef(crypto.randomUUID());
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        setBusy(true);
        void taskClient
          .input(run.id, receipt.address, value, op.current)
          .then(onUpdated)
          .catch((e) => onError(String(e)))
          .finally(() => setBusy(false));
      }}
    >
      <label>
        {receipt.label}
        <textarea
          value={value}
          onChange={(e) => setValue(e.target.value)}
          rows={3}
        />
      </label>
      <button className="task-primary" disabled={busy || !value.trim()}>
        Provide input
      </button>
      {receipt.wakeAt && (
        <p className="task-muted">Timeout: {taskTime(receipt.wakeAt)}</p>
      )}
    </form>
  );
}
export default function Detail({
  run,
  onUpdated,
  onClose,
  onConversation,
}: {
  run: TaskRun;
  onUpdated: (run: TaskRun) => void;
  onClose: () => void;
  onConversation?: (id: string) => void | Promise<void>;
}) {
  const [events, setEvents] = useState<TaskEvent[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  useEffect(() => {
    let alive = true;
    void taskClient
      .events(run.id)
      .then((v) => {
        if (alive) setEvents(v);
      })
      .catch((e) => {
        if (alive) setError(String(e));
      });
    return () => {
      alive = false;
    };
  }, [run.id, run.revision]);
  async function control(action: string) {
    setBusy(true);
    setError("");
    try {
      onUpdated(await taskClient.control(run, action));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const terminal = ["finished", "cancelled", "failed"].includes(run.state);
  const receipts = Object.values(run.progress.receipts).sort(
    (a, b) =>
      (a.sequence ?? 0) - (b.sequence ?? 0) ||
      a.startedAt - b.startedAt ||
      a.address.localeCompare(b.address, undefined, { numeric: true }),
  );
  return (
    <section className="task-detail" aria-label="Automation details">
      <div className="task-section-heading">
        <div>
          <Status state={run.state} />
          <h2>{run.name}</h2>
        </div>
        <button
          type="button"
          aria-label="Close automation details"
          onClick={onClose}
        >
          ×
        </button>
      </div>
      <p>{run.reason || run.chain.description}</p>
      {run.state === "draft" && (
        <div className="task-scope">
          <h3>Ready to start</h3>
          <p>{run.chain.description}</p>
          <dl>
            <dt>Conversation</dt>
            <dd>{run.scope.sessionId ?? "No conversation steps"}</dd>
            <dt>Folder</dt>
            <dd>{run.scope.runtimeRoot ?? "Automation artifacts"}</dd>
            <dt>Review profiles</dt>
            <dd>
              {Object.values(run.scope.profiles)
                .map((p) => p.profileName)
                .join(", ") || "None"}{" "}
              · saved versions
            </dd>
            <dt>Limits</dt>
            <dd>
              {run.chain.limits.maxActions} actions ·{" "}
              {run.chain.limits.actionTimeoutSecs / 60} minutes per action ·{" "}
              {run.chain.limits.deadlineHours} hour deadline
            </dd>
            <dt>Starts</dt>
            <dd>{taskTime(run.dueAt)}</dd>
          </dl>
          <p className="task-muted">
            Conversation steps use this project’s tools and folder permissions.
            Scheduled occurrences get their own conversation. Reviews use the
            saved review profile and their own provider credentials.
          </p>
        </div>
      )}
      <div className="task-actions">
        {!run.missionId && (
          <>
            {run.state === "draft" && (
              <button
                className="task-primary"
                disabled={busy}
                onClick={() => void control("start")}
              >
                Start automation
              </button>
            )}
            {["running", "queued", "waiting"].includes(run.state) && (
              <button disabled={busy} onClick={() => void control("pause")}>
                Pause
              </button>
            )}
            {run.state === "paused" && (
              <button
                className="task-primary"
                disabled={busy}
                onClick={() => void control("resume")}
              >
                Resume
              </button>
            )}
            {!terminal && run.state !== "cancelling" && (
              <button disabled={busy} onClick={() => void control("stop")}>
                Stop
              </button>
            )}
          </>
        )}
        {run.scope.sessionId && onConversation && (
          <button
            onClick={() => {
              void Promise.resolve(onConversation(run.scope.sessionId!)).catch(
                (e) => setError(String(e)),
              );
            }}
          >
            Open conversation ↗
          </button>
        )}
      </div>
      {run.missionId && (
        <p className="task-muted">
          Managed by a research automation. Use its controls to pause, stop or
          recover this action.
        </p>
      )}
      {!run.missionId && run.state === "attention" && (
        <div className="task-attention">
          <p>
            Inspect any completed work before retrying an interrupted action. A
            retry can repeat effects that were not recorded.
          </p>
          <div className="task-actions">
            <button disabled={busy} onClick={() => void control("reconcile")}>
              Reconcile recorded results
            </button>
            <button
              disabled={busy}
              onClick={() => void control("refreshContext")}
            >
              Use current conversation context
            </button>
            <button disabled={busy} onClick={() => void control("retry")}>
              Retry unresolved actions
            </button>
          </div>
        </div>
      )}
      {error && (
        <p className="task-error" role="alert">
          {error}
        </p>
      )}
      {run.progress.limitReached && (
        <p className="task-attention">
          The iteration limit was reached. The final artifact is the last
          reviewed version; unresolved findings remain available below.
        </p>
      )}
      {receipts.length ? (
        <ol className="task-activity" aria-label="Step activity">
          {receipts.map((r) => (
            <li key={r.operation}>
              <div className="task-section-heading">
                <div>
                  <h3>{r.label}</h3>
                  <small className="task-muted">
                    {r.address} · {taskTime(r.startedAt)}
                  </small>
                </div>
                <Status state={r.state === "timer" ? "waiting" : r.state} />
              </div>
              {r.error && <p className="task-error">{r.error}</p>}
              {r.state === "input" ? (
                <InputForm
                  run={run}
                  receipt={r}
                  onUpdated={onUpdated}
                  onError={setError}
                />
              ) : r.state === "timer" ? (
                <p className="task-muted">Resumes {taskTime(r.wakeAt)}</p>
              ) : (
                r.output !== null && (
                  <Output receipt={r} runId={run.id} onError={setError} />
                )
              )}
            </li>
          ))}
        </ol>
      ) : (
        <Outline steps={run.chain.steps} />
      )}
      <details>
        <summary>Chain and limits</summary>
        <Outline steps={run.chain.steps} />
        <pre className="task-result">{JSON.stringify(run.chain, null, 2)}</pre>
        <div className="task-actions">
          <button
            onClick={() => {
              void taskClient
                .saveChain(run.chain)
                .then(() => setSaved(true))
                .catch((e) => setError(String(e)));
            }}
          >
            {saved ? "Saved to your chains" : "Save reusable chain"}
          </button>
          <button
            onClick={() => {
              void taskClient
                .exportChain(run.chain)
                .catch((e) => setError(String(e)));
            }}
          >
            Export definition…
          </button>
        </div>
      </details>
      <details>
        <summary>Activity log</summary>
        <ol className="task-event-log">
          {events.map((e) => (
            <li key={e.sequence}>
              <time>{taskTime(e.at)}</time>
              <span>{e.detail}</span>
              {e.attempts?.map((receipt) => (
                <details key={receipt.operation}>
                  <summary>
                    Prior attempt {receipt.sequence}: {receipt.label} ·{" "}
                    {stateLabel(receipt.state)}
                  </summary>
                  {receipt.error && (
                    <p className="task-error">{receipt.error}</p>
                  )}
                  <pre className="task-result">
                    {JSON.stringify(receipt, null, 2)}
                  </pre>
                  <Output receipt={receipt} runId={run.id} onError={setError} />
                </details>
              ))}
            </li>
          ))}
        </ol>
        {events.length > 0 && events.length % 100 === 0 && (
          <button
            onClick={() => {
              void taskClient
                .events(run.id, events[events.length - 1].sequence)
                .then((v) => setEvents((old) => [...old, ...v]))
                .catch((e) => setError(String(e)));
            }}
          >
            Load more events
          </button>
        )}
      </details>
    </section>
  );
}

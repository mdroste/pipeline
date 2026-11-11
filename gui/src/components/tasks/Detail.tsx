import { useEffect, useRef, useState } from "react";
import { stateLabel, taskTime } from "../../lib/taskPresentation";
import { taskClient } from "../../lib/taskClient";
import {
  type Receipt,
  type TaskEvent,
  type TaskRun,
} from "../../lib/taskTypes";
import { Outline, Status } from "./shared";
import ReportIssueLink from "../ReportIssueLink";
import Output from "./Output";
import ScopePreview from "./ScopePreview";
import Recovery from "./Recovery";

const UNRESOLVED = ["unknown", "failed"];
/** Past its deadline a wait still takes input, and that starts a new deadline. */
function lateInput(run: TaskRun) {
  return (
    run.state === "attention" &&
    run.deadlineAt <= Date.now() / 1000 &&
    !Object.values(run.progress.receipts).some((r) =>
      UNRESOLVED.includes(r.state),
    )
  );
}
function acceptsInput(run: TaskRun) {
  return (
    ["waiting", "paused", "running", "queued"].includes(run.state) ||
    lateInput(run)
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
      {lateInput(run) && (
        <p className="task-muted">
          The deadline has passed. Providing input continues this automation
          with a new {run.chain.limits.deadlineHours}-hour deadline.
        </p>
      )}
    </form>
  );
}
export default function Detail({
  run,
  onUpdated,
  onClose,
  onConversation,
  background = null,
  onBackgroundSetup,
}: {
  run: TaskRun;
  onUpdated: (run: TaskRun) => void;
  onClose: () => void;
  onConversation?: (id: string) => void | Promise<void>;
  background?: boolean | null;
  onBackgroundSetup?: () => void;
}) {
  const [events, setEvents] = useState<TaskEvent[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  const terminal = ["finished", "cancelled", "failed"].includes(run.state);
  useEffect(() => {
    // History cleanup never removes a result nobody has opened.
    if (terminal) void taskClient.markSeen(run.id).catch(() => undefined);
  }, [run.id, terminal]);
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
        <ScopePreview
          run={run}
          background={background}
          onBackgroundSetup={onBackgroundSetup}
        />
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
        <Recovery
          key={run.revision}
          run={run}
          busy={busy}
          onControl={control}
        />
      )}
      {error && (
        <p className="task-error" role="alert">
          {error}
          <ReportIssueLink />
        </p>
      )}
      {run.progress.limitReached && (
        <p className="task-attention">
          The review limit was reached. The final file is the last version
          reviewed. Unresolved findings appear below.
        </p>
      )}
      {receipts.length ? (
        <ol className="task-activity" aria-label="Step activity">
          {receipts.map((r) => (
            <li key={r.operation}>
              <div className="task-section-heading">
                <div>
                  <h3>{r.label}</h3>
                  <small className="task-muted">{taskTime(r.startedAt)}</small>
                </div>
                <Status state={r.state === "timer" ? "waiting" : r.state} />
              </div>
              {r.error && (
                <p className="task-error">
                  {r.error}
                  <ReportIssueLink />
                </p>
              )}
              {r.state === "input" ? (
                acceptsInput(run) ? (
                  <InputForm
                    run={run}
                    receipt={r}
                    onUpdated={onUpdated}
                    onError={setError}
                  />
                ) : (
                  <p className="task-muted">
                    This automation can&rsquo;t take input right now.
                  </p>
                )
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
        <summary>Steps and limits</summary>
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
            {saved
              ? "Saved to your automation templates"
              : "Save as an automation template"}
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

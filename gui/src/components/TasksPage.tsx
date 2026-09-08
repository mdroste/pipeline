import useTabList from "../hooks/useTabList";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  taskClient,
  taskTime,
  stateLabel,
  template,
  type Chain,
  type Json,
  type Receipt,
  type Schedule,
  type SessionChoice,
  type Step,
  type TaskEvent,
  type TaskRun,
  type TaskSummary,
  type Trigger,
} from "../lib/taskClient";
import "./TasksPage.css";
const ResearchMissions = lazy(() => import("./ResearchMissions"));

function Status({ state }: { state: string }) {
  return (
    <span className={`task-status task-status--${state}`}>
      <i />
      {stateLabel(state)}
    </span>
  );
}
function Outline({ steps, level = 0 }: { steps: Step[]; level?: number }) {
  return (
    <ol
      className="task-outline"
      aria-label={level ? "Nested steps" : "Task outline"}
    >
      {steps.map((step, i) => (
        <li key={step.id}>
          <div className="task-outline-row">
            <span className="task-step-number">{i + 1}</span>
            <span>{step.label}</span>
            <small>
              {step.kind === "repeat"
                ? `up to ${step.maxIterations} rounds`
                : step.kind === "delay"
                  ? `${step.seconds / 60} min`
                  : step.kind === "parallel"
                    ? "in parallel"
                    : step.kind}
            </small>
          </div>
          {step.kind === "repeat" ||
          step.kind === "while" ||
          step.kind === "forEach" ? (
            <Outline steps={step.steps} level={level + 1} />
          ) : step.kind === "if" ? (
            <>
              <Outline steps={step.thenSteps} level={level + 1} />
              {!!step.elseSteps?.length && (
                <Outline steps={step.elseSteps} level={level + 1} />
              )}
            </>
          ) : step.kind === "parallel" ? (
            step.branches.map((branch, n) => (
              <Outline key={n} steps={branch} level={level + 1} />
            ))
          ) : step.kind === "chain" ? (
            <Outline steps={step.chain.steps} level={level + 1} />
          ) : null}
        </li>
      ))}
    </ol>
  );
}
const zone = () => Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
const localDate = (date: Date) =>
  new Date(date.getTime() - date.getTimezoneOffset() * 60000)
    .toISOString()
    .slice(0, 16);
export function Timing({
  value,
  onChange,
}: {
  value: Trigger;
  onChange: (trigger: Trigger) => void;
}) {
  const [times, setTimes] = useState<number[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    let current = true;
    setError("");
    void taskClient
      .times(value)
      .then((v) => {
        if (current) setTimes(v);
      })
      .catch((e) => {
        if (current) setError(String(e));
      });
    return () => {
      current = false;
    };
  }, [value]);
  return (
    <fieldset className="task-timing">
      <legend>When</legend>
      <div className="task-timing-fields">
        <label className="sr-only" htmlFor="task-timing-kind">
          Start rule
        </label>
        <select
          id="task-timing-kind"
          value={value.kind}
          onChange={(e) => {
            const now = Math.floor(Date.now() / 1000);
            onChange(
              e.target.value === "once"
                ? { kind: "once", at: now + 3600 }
                : e.target.value === "interval"
                  ? { kind: "interval", seconds: 86400, anchor: now + 86400 }
                  : e.target.value === "calendar"
                    ? {
                        kind: "calendar",
                        timezone: zone(),
                        hour: 9,
                        minute: 0,
                        weekdays: [0, 1, 2, 3, 4],
                      }
                    : { kind: "now" },
            );
          }}
        >
          <option value="now">Now</option>
          <option value="once">Later</option>
          <option value="interval">At an interval</option>
          <option value="calendar">On a schedule</option>
        </select>
        {value.kind === "once" && (
          <>
            <input
              aria-label="Start date and time"
              type="datetime-local"
              value={localDate(new Date(value.at * 1000))}
              onChange={(e) => {
                if (e.target.value)
                  onChange({
                    ...value,
                    at: Math.floor(new Date(e.target.value).getTime() / 1000),
                  });
              }}
            />
            <button
              type="button"
              onClick={() =>
                onChange({
                  kind: "once",
                  at: Math.floor(Date.now() / 1000) + 3600,
                })
              }
            >
              In one hour
            </button>
          </>
        )}
        {value.kind === "interval" && (
          <>
            <label>
              Every{" "}
              <input
                aria-label="Interval in minutes"
                type="number"
                min={1}
                max={525600}
                value={value.seconds / 60}
                onChange={(e) =>
                  onChange({
                    ...value,
                    seconds: Math.max(1, Number(e.target.value)) * 60,
                  })
                }
              />{" "}
              minutes
            </label>
            <label>
              First run{" "}
              <input
                aria-label="First run"
                type="datetime-local"
                value={localDate(new Date(value.anchor * 1000))}
                onChange={(e) => {
                  if (e.target.value)
                    onChange({
                      ...value,
                      anchor: Math.floor(
                        new Date(e.target.value).getTime() / 1000,
                      ),
                    });
                }}
              />
            </label>
          </>
        )}
        {value.kind === "calendar" && (
          <>
            <input
              aria-label="Time of day"
              type="time"
              value={`${String(value.hour).padStart(2, "0")}:${String(value.minute).padStart(2, "0")}`}
              onChange={(e) => {
                const [hour, minute] = e.target.value.split(":").map(Number);
                if (Number.isFinite(hour + minute))
                  onChange({ ...value, hour, minute });
              }}
            />
            <input
              aria-label="Time zone"
              value={value.timezone}
              onChange={(e) => onChange({ ...value, timezone: e.target.value })}
            />
            <div className="task-weekdays" aria-label="Days of the week">
              {["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].map(
                (day, i) => (
                  <button
                    key={day}
                    type="button"
                    aria-pressed={value.weekdays.includes(i)}
                    onClick={() =>
                      onChange({
                        ...value,
                        weekdays: value.weekdays.includes(i)
                          ? value.weekdays.filter((v) => v !== i)
                          : [...value.weekdays, i],
                      })
                    }
                  >
                    {day}
                  </button>
                ),
              )}
            </div>
          </>
        )}
      </div>
      {error ? (
        <p role="alert">{error}</p>
      ) : (
        times.length > 0 && (
          <p className="task-muted">Next: {times.map(taskTime).join(" · ")}</p>
        )
      )}
      {value.kind !== "now" && (
        <p className="task-muted">
          Runs while Pipeline is open and this computer is awake. Missed runs
          are combined into one. Each run finishes before the next one starts.
        </p>
      )}
    </fieldset>
  );
}

function Builder({
  initialSessionId,
  onPrepared,
  onClose,
}: {
  initialSessionId?: string | null;
  onPrepared: (run: TaskRun) => void;
  onClose: () => void;
}) {
  const [kind, setKind] = useState<"review" | "prompt" | "input">("review");
  const [prompt, setPrompt] = useState("");
  const [profile, setProfile] = useState("");
  const [rounds, setRounds] = useState(3);
  const [path, setPath] = useState("");
  const [profiles, setProfiles] = useState<{ id: string; name: string }[]>([]);
  const [sessions, setSessions] = useState<SessionChoice[]>([]);
  const [session, setSession] = useState(initialSessionId ?? "");
  const [trigger, setTrigger] = useState<Trigger>({ kind: "now" });
  const [saved, setSaved] = useState<{ id: string; chain: Chain }[]>([]);
  const [advanced, setAdvanced] = useState<string | null>(null);
  const [inputs, setInputs] = useState("{}");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const operation = useRef(crypto.randomUUID());
  const generated = template(kind, prompt, profile, rounds, path);
  useEffect(() => {
    let alive = true;
    void Promise.all([
      invoke<{ id: string; name: string }[]>("list_profiles"),
      taskClient.sessions(),
      taskClient.chains(),
    ])
      .then(([p, s, c]) => {
        if (!alive) return;
        setProfiles(p);
        setProfile(
          p.find((v) => /automatic.*paper|auto.*review/i.test(v.name))?.id ??
            p[0]?.id ??
            "",
        );
        setSessions(s);
        setSaved(c);
      })
      .catch((e) => {
        if (alive) setError(String(e));
      });
    return () => {
      alive = false;
    };
  }, []);
  async function prepare() {
    setBusy(true);
    setError("");
    try {
      const chain = await taskClient.validate(
        advanced ?? JSON.stringify(generated),
      );
      const parsed: unknown = JSON.parse(inputs);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
        throw new Error("Inputs must be a JSON object");
      const run = await taskClient.prepare(
        chain,
        session || null,
        parsed as Record<string, Json>,
        trigger,
        operation.current,
      );
      onPrepared(run);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="task-builder" aria-label="New task chain">
      <div className="task-section-heading">
        <div>
          <h2>New task chain</h2>
          <p className="task-muted">
            Review and revise a paper, schedule a follow-up, or wait for input.
          </p>
        </div>
        <button type="button" onClick={onClose} aria-label="Close new task">
          ×
        </button>
      </div>
      <div className="task-template-picker" aria-label="Task template">
        {(
          [
            [
              "review",
              "Review and revise",
              "Review a paper and revise it up to a set limit.",
            ],
            ["prompt", "Follow up", "Continue a conversation now or later."],
            [
              "input",
              "Wait for input",
              "Pick up when the missing input arrives.",
            ],
          ] as const
        ).map(([id, title, subtitle]) => (
          <button
            key={id}
            type="button"
            aria-label={title}
            aria-pressed={kind === id && advanced === null}
            onClick={() => {
              setKind(id);
              setAdvanced(null);
            }}
          >
            <strong>{title}</strong>
            <span>{subtitle}</span>
          </button>
        ))}
      </div>
      <label>
        Conversation
        <select value={session} onChange={(e) => setSession(e.target.value)}>
          <option value="">Select a conversation</option>
          {sessions.map((s) => (
            <option key={s.id} value={s.id}>
              {s.title}
              {s.workspaceName ? ` · ${s.workspaceName}` : ""}
            </option>
          ))}
        </select>
      </label>
      {advanced === null ? (
        <>
          <label>
            {kind === "review"
              ? "Idea or instructions"
              : "What should happen next?"}
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={4}
              placeholder={
                kind === "review"
                  ? "Develop a paper about…"
                  : "Review the new results and explain what changed…"
              }
            />
          </label>
          {kind === "review" && (
            <>
              <div className="task-form-row">
                <label>
                  Workflow profile
                  <select
                    value={profile}
                    onChange={(e) => setProfile(e.target.value)}
                  >
                    {profiles.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Maximum reviews
                  <input
                    type="number"
                    min={1}
                    max={32}
                    value={rounds}
                    onChange={(e) => setRounds(Number(e.target.value))}
                  />
                </label>
              </div>
              <details>
                <summary>Start from an existing paper</summary>
                <label>
                  File within the conversation’s folder
                  <input
                    value={path}
                    onChange={(e) => setPath(e.target.value)}
                    placeholder="/path/to/paper.pdf"
                  />
                </label>
                <p className="task-muted">
                  Leave blank to draft from your idea. Revisions from this
                  template are saved as Markdown.
                </p>
              </details>
            </>
          )}
          <Outline steps={generated.steps} />
        </>
      ) : (
        <>
          <label>
            Chain definition
            <textarea
              className="task-code"
              spellCheck={false}
              rows={19}
              value={advanced}
              onChange={(e) => setAdvanced(e.target.value)}
            />
          </label>
          <label>
            Named inputs
            <textarea
              className="task-code"
              spellCheck={false}
              rows={3}
              value={inputs}
              onChange={(e) => setInputs(e.target.value)}
            />
          </label>
          <p className="task-muted">
            Supports Workspace, snapshot, Workflow, check, delivery, delay, input,
            if, repeat, parallel, forEach, and embedded chain steps. Bind
            outputs with a step ID and JSON pointer.
          </p>
        </>
      )}
      <div className="task-actions">
        <button
          type="button"
          onClick={() =>
            setAdvanced(
              advanced === null ? JSON.stringify(generated, null, 2) : null,
            )
          }
        >
          {advanced === null ? "Edit definition" : "Use template"}
        </button>
        <label className="task-import">
          Import chain
          <input
            type="file"
            accept=".json,application/json"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (!file) return;
              if (file.size > 262144) {
                setError("Chain exceeds 256 KiB");
                return;
              }
              void file
                .text()
                .then(taskClient.validate)
                .then((c) => setAdvanced(JSON.stringify(c, null, 2)))
                .catch((e) => setError(String(e)));
              e.target.value = "";
            }}
          />
        </label>
        {saved.length > 0 && (
          <select
            aria-label="Use a saved chain"
            value=""
            onChange={(e) => {
              const c = saved.find((v) => v.id === e.target.value);
              if (c) setAdvanced(JSON.stringify(c.chain, null, 2));
            }}
          >
            <option value="">Saved chains…</option>
            {saved.map((c) => (
              <option value={c.id} key={c.id}>
                {c.chain.name}
              </option>
            ))}
          </select>
        )}
      </div>
      <Timing value={trigger} onChange={setTrigger} />
      {error && (
        <p className="task-error" role="alert">
          {error}
        </p>
      )}
      <div className="task-footer">
        <p className="task-muted">
          You’ll review the exact scope before starting.
        </p>
        <button
          className="task-primary"
          disabled={
            busy ||
            (advanced === null &&
              (!session ||
                (kind === "review" && !profile) ||
                (!prompt.trim() && !path)))
          }
          onClick={() => void prepare()}
        >
          {busy ? "Preparing…" : "Prepare task chain →"}
        </button>
      </div>
    </section>
  );
}

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
        <p className="task-muted">Workflow run: {String(result.runId)}</p>
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
            void invoke<Json>("task_step_output", {
              id: runId,
              address: receipt.address,
              operation: receipt.operation,
            })
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
function Detail({
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
    <section className="task-detail" aria-label="Task details">
      <div className="task-section-heading">
        <div>
          <Status state={run.state} />
          <h2>{run.name}</h2>
        </div>
        <button type="button" aria-label="Close task details" onClick={onClose}>
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
            <dd>{run.scope.sessionId ?? "No Workspace actions"}</dd>
            <dt>Folder</dt>
            <dd>{run.scope.runtimeRoot ?? "Task artifacts"}</dd>
            <dt>Workflow profiles</dt>
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
            Workspace turns use this conversation’s tools and folder
            permissions. Scheduled occurrences get their own conversation.
            Reviews use the saved workflow profile and their own provider credentials.
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
                Start task chain
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
          Managed by a research mission. Use its mission controls to pause, stop
          or recover this action.
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

function ScheduleCard({
  schedule,
  onUpdated,
  onError,
}: {
  schedule: Schedule;
  onUpdated: () => void;
  onError: (error: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [trigger, setTrigger] = useState(schedule.trigger);
  const [busy, setBusy] = useState(false);
  async function update(enabled: boolean, value: Trigger | null = null) {
    setBusy(true);
    try {
      await taskClient.updateSchedule(schedule, enabled, value);
      setEditing(false);
      onUpdated();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <article className="task-schedule">
      <div className="task-section-heading">
        <div>
          <h3>{schedule.name}</h3>
          <p className="task-muted">
            {schedule.enabled
              ? `Next run ${taskTime(schedule.nextDueAt)}`
              : "Paused"}
          </p>
        </div>
        <div className="task-actions">
          <button
            disabled={busy}
            onClick={() => void update(!schedule.enabled)}
          >
            {schedule.enabled ? "Pause" : "Enable"}
          </button>
          <button onClick={() => setEditing((v) => !v)}>Edit timing</button>
        </div>
      </div>
      {editing && (
        <>
          <Timing value={trigger} onChange={setTrigger} />
          <button
            className="task-primary"
            disabled={busy || trigger.kind === "now" || trigger.kind === "once"}
            onClick={() => void update(schedule.enabled, trigger)}
          >
            Save schedule
          </button>
        </>
      )}
    </article>
  );
}
export default function TasksPage({
  initialSessionId,
  onConversation,
  initialTaskId,
}: {
  initialSessionId?: string | null;
  onConversation?: (id: string) => void | Promise<void>;
  initialTaskId?: string | null;
}) {
  const [view, setView] = useState("active");
  const views = ["active", "scheduled", "history", "missions"];
  const tabList = useTabList(
    views,
    view,
    (id) => {
      setView(id);
      setOffset(0);
    },
    "manual",
  );
  const [runs, setRuns] = useState<TaskSummary[]>([]);
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [selected, setSelected] = useState<TaskRun | null>(null);
  const selectedRef = useRef<string | null>(initialTaskId ?? null);
  const [creating, setCreating] = useState(
    Boolean(initialSessionId && !initialTaskId),
  );
  const [error, setError] = useState("");
  const [background, setBackground] = useState(false);
  const [loading, setLoading] = useState(true);
  const [offset, setOffset] = useState(0);
  const refreshVersion = useRef(0);
  const refresh = useCallback(async () => {
    const version = ++refreshVersion.current;
    const selectedId = selectedRef.current;
    try {
      const [rows, planned, detail] = await Promise.all([
        view === "missions"
          ? Promise.resolve([])
          : taskClient.list(view, null, offset),
        view === "scheduled" ? taskClient.schedules() : Promise.resolve([]),
        selectedId ? taskClient.get(selectedId) : Promise.resolve(null),
      ]);
      if (version !== refreshVersion.current) return;
      setRuns(rows);
      setSchedules(planned);
      if (selectedId === selectedRef.current) setSelected(detail);
      setError("");
    } catch (e) {
      if (version === refreshVersion.current) setError(String(e));
    } finally {
      if (version === refreshVersion.current) setLoading(false);
    }
  }, [view, offset]);
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const queue = () => {
      if (!disposed && !timer)
        timer = setTimeout(() => {
          timer = null;
          if (!disposed) void refresh();
        }, 180);
    };
    void refresh();
    const unlisten = listen("tasks:changed", queue);
    const errors = listen<string>("tasks:error", (e) => setError(e.payload));
    return () => {
      disposed = true;
      refreshVersion.current++;
      if (timer) clearTimeout(timer);
      void unlisten.then((f) => f());
      void errors.then((f) => f());
    };
  }, [refresh]);
  useEffect(() => {
    localStorage.setItem("pipeline.tasks.enabled", "true");
    void taskClient
      .background()
      .then(setBackground)
      .catch((e) => setError(String(e)));
  }, []);
  const select = (run: TaskRun) => {
    selectedRef.current = run.id;
    setSelected(run);
    setCreating(false);
    void refresh();
  };
  return (
    <main className="tasks-page">
      <header className="tasks-header">
        <div>
          <p className="task-eyebrow">PIPELINE</p>
          <h1>Tasks</h1>
          <p className="task-muted">
            Task chains connect Workspace and Reviews. Missions pursue research goals.
          </p>
        </div>
        {view !== "missions" && (
          <button
            className="task-primary"
            onClick={() => {
              setCreating(true);
              selectedRef.current = null;
              setSelected(null);
            }}
          >
            + New task chain
          </button>
        )}
      </header>
      <div className="tasks-toolbar">
        <div className="task-tabs" role="tablist" aria-label="Tasks view">
          {views.map((v) => (
            <button {...tabList.tabProps(v)} key={v}>
              {v.charAt(0).toUpperCase() + v.slice(1)}
            </button>
          ))}
        </div>
        <label className="task-background">
          <input
            type="checkbox"
            checked={background}
            onChange={(e) => {
              void taskClient
                .background(e.target.checked)
                .then(setBackground)
                .catch((e) => setError(String(e)));
            }}
          />
          Keep running when the window closes
        </label>
      </div>
      {error && (
        <p className="task-error" role="alert">
          {error}
        </p>
      )}
      {views
        .filter((id) => id !== view)
        .map((id) => (
          <div key={id} {...tabList.panelProps(id)} hidden />
        ))}
      <div {...tabList.panelProps(view)}>
        {view === "missions" ? (
          <Suspense
            fallback={<p className="task-empty">Loading research missions…</p>}
          >
            <ResearchMissions
              initialSessionId={initialSessionId}
              onConversation={onConversation}
              onTask={async (id) => {
                const run = await taskClient.get(id);
                setView("active");
                select(run);
              }}
            />
          </Suspense>
        ) : (
          <div
            className={`tasks-content ${creating || selected ? "tasks-content--split" : ""}`}
          >
            <section className="task-list">
              {loading ? (
                <p className="task-empty">Loading tasks…</p>
              ) : view === "scheduled" ? (
                schedules.length ? (
                  schedules.map((s) => (
                    <ScheduleCard
                      key={`${s.id}-${s.revision}`}
                      schedule={s}
                      onUpdated={() => void refresh()}
                      onError={setError}
                    />
                  ))
                ) : (
                  <div className="task-empty">
                    <h2>Make time for recurring work.</h2>
                    <p>Choose a schedule when creating a task chain.</p>
                  </div>
                )
              ) : runs.length ? (
                <>
                  {runs.map((run) => (
                    <button
                      className={`task-list-row ${selected?.id === run.id ? "is-selected" : ""}`}
                      key={run.id}
                      onClick={() => {
                        selectedRef.current = run.id;
                        setCreating(false);
                        void taskClient
                          .get(run.id)
                          .then((detail) => {
                            if (selectedRef.current === run.id)
                              setSelected(detail);
                          })
                          .catch((e) => {
                            if (selectedRef.current === run.id)
                              setError(String(e));
                          });
                      }}
                    >
                      <div>
                        <strong>{run.name}</strong>
                        <span>
                          {run.reason ||
                            (run.dueAt && run.dueAt > Date.now() / 1000
                              ? `Starts ${taskTime(run.dueAt)}`
                              : "Ready for the next step")}
                        </span>
                      </div>
                      <Status state={run.state} />
                    </button>
                  ))}
                  <div className="task-actions">
                    {offset > 0 && (
                      <button
                        onClick={() => setOffset(Math.max(0, offset - 50))}
                      >
                        Previous
                      </button>
                    )}
                    {runs.length === 50 && (
                      <button onClick={() => setOffset(offset + 50)}>
                        Next
                      </button>
                    )}
                  </div>
                </>
              ) : (
                <div className="task-empty">
                  <div className="task-empty-symbol" aria-hidden="true">
                    ○—○—○
                  </div>
                  <h2>
                    {view === "history"
                      ? "Completed work will appear here."
                      : selected
                        ? "No active tasks."
                        : "Give your work a next step."}
                  </h2>
                  <p>
                    Connect Workspace and Reviews, pause for input, or set a time
                    to continue.
                  </p>
                  {view !== "history" && (
                    <button onClick={() => setCreating(true)}>
                      Create a task chain →
                    </button>
                  )}
                </div>
              )}
            </section>
            {creating ? (
              <Builder
                initialSessionId={initialSessionId}
                onPrepared={select}
                onClose={() => setCreating(false)}
              />
            ) : (
              selected && (
                <Detail
                  key={selected.id}
                  run={selected}
                  onUpdated={select}
                  onClose={() => {
                    selectedRef.current = null;
                    setSelected(null);
                  }}
                  onConversation={onConversation}
                />
              )
            )}
          </div>
        )}
      </div>
    </main>
  );
}

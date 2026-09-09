import { lazy, Suspense, useEffect, useRef, useState } from "react";
import {
  missionClient,
  missionState,
  type Mission,
  type MissionQuestion,
} from "../../lib/missionClient";
import { taskTime, type Json, type TaskEvent } from "../../lib/taskClient";
const ReportViewer = lazy(() => import("../ReportViewer"));

function Answer({
  missionId,
  question,
  terminal,
  onUpdated,
  onError,
}: {
  terminal: boolean;
  missionId: string;
  question: MissionQuestion;
  onUpdated: (m: Mission) => void;
  onError: (error: string) => void;
}) {
  const [answer, setAnswer] = useState("");
  const [busy, setBusy] = useState(false);
  const op = useRef(crypto.randomUUID());
  return (
    <form
      className="mission-question"
      onSubmit={(e) => {
        e.preventDefault();
        setBusy(true);
        void missionClient
          .answer(missionId, question.id, answer, op.current)
          .then(onUpdated)
          .catch((e) => onError(String(e)))
          .finally(() => setBusy(false));
      }}
    >
      <label htmlFor={`mission-answer-${question.id}`}>
        {question.question}
      </label>
      <p className="task-muted">{question.whyNeeded}</p>
      {question.answer !== null ? (
        <p>
          <strong>Your answer:</strong> {question.answer}
        </p>
      ) : terminal ? (
        <p className="task-muted">Unanswered when the automation ended.</p>
      ) : (
        <>
          <textarea
            id={`mission-answer-${question.id}`}
            rows={3}
            disabled={busy}
            value={answer}
            onChange={(e) => {
              setAnswer(e.target.value);
              op.current = crypto.randomUUID();
            }}
          />
          <button disabled={busy || !answer.trim()}>
            {busy ? "Saving…" : "Send research input"}
          </button>
        </>
      )}
    </form>
  );
}
export default function MissionDetail({
  mission: m,
  onUpdated,
  onClose,
  onConversation,
  onTask,
}: {
  mission: Mission;
  onUpdated: (m: Mission) => void;
  onClose: () => void;
  onConversation?: (id: string) => void | Promise<void>;
  onTask?: (id: string) => void | Promise<void>;
}) {
  const [tab, setTab] = useState("agenda");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [events, setEvents] = useState<TaskEvent[]>([]);
  const [evidence, setEvidence] = useState<{ id: string; value: Json } | null>(
    null,
  );
  const evidenceVersion = useRef(0);
  useEffect(
    () => () => {
      evidenceVersion.current++;
    },
    [],
  );
  useEffect(() => {
    let live = true;
    if (tab === "activity")
      void missionClient
        .events(m.id)
        .then((v) => {
          if (live) setEvents(v);
        })
        .catch((e) => {
          if (live) setError(String(e));
        });
    return () => {
      live = false;
    };
  }, [m.id, m.revision, tab]);
  async function control(action: string) {
    setBusy(true);
    setError("");
    try {
      onUpdated(await missionClient.control(m, action));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function openEvidence(id: string) {
    const version = ++evidenceVersion.current;
    setEvidence(null);
    try {
      const value = await missionClient.evidence(m.id, id);
      if (version === evidenceVersion.current) setEvidence({ id, value });
    } catch (e) {
      if (version === evidenceVersion.current) setError(String(e));
    }
  }
  async function exportBrief(format: "markdown" | "json") {
    try {
      await missionClient.export(m.id, format);
    } catch (e) {
      setError(String(e));
    }
  }
  const terminal = ["completed", "exhausted", "stopped"].includes(m.state);
  const latest = [...m.rounds].reverse().find((r) => r.challenge)?.challenge;
  const navigate = (
    callback: ((id: string) => void | Promise<void>) | undefined,
    id: string,
  ) => {
    if (callback)
      void Promise.resolve(callback(id)).catch((e) => setError(String(e)));
  };
  return (
    <section
      className="task-detail mission-detail"
      aria-label="Research automation details"
    >
      <div className="task-section-heading">
        <div>
          <span className={`task-status task-status--${m.state}`}>
            {missionState(m.state)}
          </span>
          <h2>{m.definition.name}</h2>
        </div>
        <button
          aria-label="Close research automation details"
          onClick={onClose}
        >
          ×
        </button>
      </div>
      <p>{m.definition.objective}</p>
      <p className="mission-reason" role="status">
        {m.reason}
      </p>
      <div
        className="mission-metrics"
        aria-label="Research automation resource use"
      >
        <span>
          <strong>
            {m.rounds.length} / {m.definition.budget.maxRounds}
          </strong>{" "}
          rounds
        </span>
        <span>
          <strong>
            {m.actionsReserved} / {m.definition.budget.maxActions}
          </strong>{" "}
          actions
        </span>
        <span>
          <strong>
            {(m.activeSeconds / 3600).toFixed(1)} /{" "}
            {(m.definition.budget.activeSeconds / 3600).toFixed(1)}
          </strong>{" "}
          active hours
        </span>
      </div>
      {m.deadlineAt && (
        <p className="task-muted">
          Deadline: {taskTime(m.deadlineAt)} · {m.attentionCount} interruptions
          requiring attention
        </p>
      )}
      {m.state === "draft" && (
        <div className="task-scope">
          <h3>Prepared scope</h3>
          <dl>
            <dt>Folder</dt>
            <dd>{m.scope.runtimeRoot}</dd>
            <dt>Role conversations</dt>
            <dd>
              Separate planner, investigator and challenger, with scoped project
              search and exact-source retrieval.
            </dd>
            <dt>File access</dt>
            <dd>
              {m.definition.policy.allowEdits
                ? "Investigator may edit the existing isolated task copy; planner and challenger inspect only."
                : "Inspect only."}
            </dd>
            <dt>Command network</dt>
            <dd>
              {m.definition.policy.commandNetwork
                ? "Uses the source conversation’s existing network permission."
                : "Disabled."}
            </dd>
            <dt>Computation</dt>
            <dd>
              {m.capabilities.filter((c) => c.kind === "check").length}{" "}
              configured checks ·{" "}
              {m.capabilities.filter((c) => c.kind === "experiment").length}{" "}
              exact captured variants
            </dd>
            <dt>Additional review</dt>
            <dd>
              {Object.values(m.scope.profiles)
                .map((p) => p.profileName)
                .join(", ") || "Separate project challenge"}
            </dd>
            <dt>Stopping rule</dt>
            <dd>
              Every original criterion assessed as met with evidence, a resource
              limit, or {m.definition.budget.maxStagnantRounds} stagnant rounds.
            </dd>
          </dl>
          {!!m.capabilities.length && (
            <details>
              <summary>Inspect authorized computations</summary>
              {m.capabilities.map((c) => (
                <div key={`${c.kind}-${c.id}`}>
                  <strong>{c.name}</strong>
                  <p className="task-muted">
                    {c.kind} · timeout {c.timeoutSeconds} seconds
                  </p>
                  {c.kind === "experiment" && (
                    <pre className="task-result">
                      {JSON.stringify(c.parameters, null, 2)}
                    </pre>
                  )}
                  <pre className="task-result">
                    {JSON.stringify(c.execution, null, 2)}
                  </pre>
                  <small>Capture: {c.fingerprint}</small>
                </div>
              ))}
              <p className="task-muted">
                Captured execution uses fresh copies of declared inputs and
                retains host access. Starting authorizes these exact variants;
                changed plans require a new review.
              </p>
            </details>
          )}
        </div>
      )}
      <div className="task-actions">
        {m.state === "draft" && (
          <button
            className="task-primary"
            disabled={busy}
            onClick={() => void control("start")}
          >
            Start research automation
          </button>
        )}
        {["queued", "running", "waiting"].includes(m.state) && (
          <button disabled={busy} onClick={() => void control("pause")}>
            Pause research automation
          </button>
        )}
        {m.state === "paused" && (
          <button
            className="task-primary"
            disabled={busy}
            onClick={() => void control("resume")}
          >
            Resume research automation
          </button>
        )}
        {!terminal && m.state !== "stopping" && (
          <button disabled={busy} onClick={() => void control("stop")}>
            Stop research automation
          </button>
        )}
        <button onClick={() => void exportBrief("markdown")}>
          Export brief…
        </button>
      </div>
      {m.state === "attention" && (
        <div className="task-attention">
          <p>
            Inspect the recorded work before retrying. A retry can repeat
            effects whose outcome was not recorded, and uses the remaining
            automation budget.
          </p>
          <div className="task-actions">
            {m.activeChild && (
              <button disabled={busy} onClick={() => void control("reconcile")}>
                Reconcile recorded results
              </button>
            )}
            <button disabled={busy} onClick={() => void control("retry")}>
              {m.activeChild
                ? "Retry interrupted action"
                : "Reconsider the agenda"}
            </button>
            {m.activeChild && onTask && (
              <button onClick={() => navigate(onTask, m.activeChild!)}>
                Inspect action
              </button>
            )}
          </div>
        </div>
      )}
      {error && (
        <p className="task-error" role="alert">
          {error}
        </p>
      )}
      <div
        className="task-tabs mission-tabs"
        role="tablist"
        aria-label="Research automation detail view"
      >
        {[
          "agenda",
          "findings",
          "decisions",
          "methods",
          "activity",
          "brief",
        ].map((v) => (
          <button
            role="tab"
            aria-selected={tab === v}
            key={v}
            onClick={() => setTab(v)}
          >
            {v.charAt(0).toUpperCase() + v.slice(1)}
            {v === "decisions" && m.questions.some((q) => q.answer === null)
              ? ` (${m.questions.filter((q) => q.answer === null).length})`
              : ""}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        className="mission-tabpanel"
        aria-label={`${tab} for research automation`}
      >
        {tab === "agenda" && (
          <>
            <h3>Original completion criteria</h3>
            <ol className="mission-criteria">
              {m.definition.criteria.map((criterion, i) => {
                const assessment = latest?.criteria.find(
                  (a) => a.criterion === i,
                );
                return (
                  <li key={i}>
                    <strong>{criterion}</strong>
                    <span className="task-muted">
                      {assessment
                        ? `${assessment.status === "met" ? "Assessed as met" : assessment.status}: ${assessment.explanation}`
                        : "Awaiting assessment"}
                    </span>
                    {assessment?.evidenceIds.map((id) => (
                      <button
                        className="mission-evidence-link"
                        key={id}
                        onClick={() => void openEvidence(id)}
                      >
                        {id}
                      </button>
                    ))}
                  </li>
                );
              })}
            </ol>
            <h3>Research agenda</h3>
            {m.goals.length ? (
              m.goals.map((goal) => (
                <article className="mission-goal" key={goal.id}>
                  <div className="task-section-heading">
                    <strong>{goal.question}</strong>
                    <span className="task-status">{goal.state}</span>
                  </div>
                  <p>{goal.rationale}</p>
                  <p>
                    <strong>Resolving evidence:</strong>{" "}
                    {goal.resolvingEvidence}
                  </p>
                  {goal.dependsOn.length > 0 && (
                    <p className="task-muted">
                      Depends on {goal.dependsOn.join(", ")}
                    </p>
                  )}
                  {goal.assessment && <p>{goal.assessment}</p>}
                </article>
              ))
            ) : (
              <p className="task-muted">
                The planner will propose goals and compare the first useful
                investigations after you start.
              </p>
            )}
            {m.rounds[m.rounds.length - 1]?.selected && (
              <div className="mission-next">
                <h3>Selected investigation</h3>
                <p>{m.rounds[m.rounds.length - 1]!.selected!.question}</p>
                <p className="task-muted">
                  {m.rounds[m.rounds.length - 1]!.plan.selectionReason}
                </p>
              </div>
            )}
          </>
        )}
        {tab === "findings" && (
          <>
            {m.rounds.length ? (
              [...m.rounds].reverse().map((r) => (
                <article className="mission-round" key={r.number}>
                  <h3>
                    Round {r.number}:{" "}
                    {r.selected?.question ?? "Agenda assessment"}
                  </h3>
                  <p>{r.plan.summary}</p>
                  <p className="task-muted">{r.plan.selectionReason}</p>
                  {r.finding && (
                    <>
                      <h4>Investigation</h4>
                      <p>{r.finding.summary}</p>
                      <p>
                        <strong>Outcome:</strong> {r.finding.outcome}
                      </p>
                      <p>
                        <strong>Method and domain:</strong> {r.finding.method} ·{" "}
                        {r.finding.testedDomain}
                      </p>
                      {r.finding.limitations.length > 0 && (
                        <ul>
                          {r.finding.limitations.map((v, i) => (
                            <li key={i}>{v}</li>
                          ))}
                        </ul>
                      )}
                    </>
                  )}
                  {r.challenge && (
                    <>
                      <h4>Independent challenge · {r.challenge.outcome}</h4>
                      <p>{r.challenge.summary}</p>
                      <p className="task-muted">{r.challenge.testedDomain}</p>
                      {r.challenge.unresolved.length > 0 && (
                        <ul>
                          {r.challenge.unresolved.map((v, i) => (
                            <li key={i}>{v}</li>
                          ))}
                        </ul>
                      )}
                    </>
                  )}
                  <div className="task-actions">
                    {Object.keys(r.evidence).map((id) => (
                      <button key={id} onClick={() => void openEvidence(id)}>
                        Evidence: {id}
                      </button>
                    ))}
                  </div>
                  <details>
                    <summary>Alternatives considered</summary>
                    {r.plan.candidates.map((c) => (
                      <div key={c.id}>
                        <h4>{c.question}</h4>
                        <p>{c.rationale}</p>
                        <p>Uncertainty: {c.uncertainty}</p>
                        <ul>
                          {c.possibleOutcomes.map((outcome, i) => (
                            <li key={i}>{outcome}</li>
                          ))}
                        </ul>
                        <p className="task-muted">{c.expectedCost}</p>
                      </div>
                    ))}
                  </details>
                </article>
              ))
            ) : (
              <p className="task-muted">
                Investigations and their evidence will appear here, including
                unsuccessful attempts.
              </p>
            )}
            <p className="task-muted">
              Conclusions are model assessments. Researcher acceptance and file
              acceptance remain separate.
            </p>
          </>
        )}
        {tab === "decisions" && (
          <>
            {m.questions.length ? (
              m.questions.map((q) => (
                <Answer
                  key={q.id}
                  missionId={m.id}
                  question={q}
                  terminal={terminal}
                  onUpdated={onUpdated}
                  onError={setError}
                />
              ))
            ) : (
              <p className="task-muted">
                No researcher decisions are pending. Pipeline can continue
                independent work while another branch awaits input.
              </p>
            )}
          </>
        )}
        {tab === "methods" && (
          <>
            {m.methods.length ? (
              m.methods.map((method) => (
                <article className="mission-goal" key={method.id}>
                  <h3>{method.name}</h3>
                  <p>
                    <strong>Use when:</strong> {method.whenToUse}
                  </p>
                  <p>{method.procedure}</p>
                  <p>
                    <strong>Limitations:</strong> {method.limitations}
                  </p>
                  <div className="task-actions">
                    {method.evidenceIds.map((id) => (
                      <button key={id} onClick={() => void openEvidence(id)}>
                        {id}
                      </button>
                    ))}
                    <button
                      disabled={busy}
                      onClick={() => {
                        setBusy(true);
                        void missionClient
                          .retainMethod(m, method.id, !method.retained)
                          .then(onUpdated)
                          .catch((e) => setError(String(e)))
                          .finally(() => setBusy(false));
                      }}
                    >
                      {method.retained
                        ? "Remove from retained methods"
                        : "Save for reuse"}
                    </button>
                  </div>
                </article>
              ))
            ) : (
              <p className="task-muted">
                Useful methods may be proposed after an investigation. Retaining
                one makes it selectable for future research automations in this
                project.
              </p>
            )}
            <p className="task-muted">
              Retained methods keep their original evidence and limitations.
              They are not automatically evaluated or installed as defaults.
            </p>
          </>
        )}
        {tab === "activity" && (
          <>
            <h3>Role conversations</h3>
            <div className="task-actions">
              {(
                [
                  ["Planner", m.plannerScope],
                  ["Investigator", m.scope],
                  ["Challenger", m.challengerScope],
                ] as const
              ).map(
                ([label, scope]) =>
                  scope.sessionId && (
                    <button
                      key={label}
                      disabled={!onConversation}
                      onClick={() => navigate(onConversation, scope.sessionId!)}
                    >
                      {label} ↗
                    </button>
                  ),
              )}
            </div>
            <h3>Action history</h3>
            {m.childIds.map((id, i) => (
              <button
                className="mission-child"
                key={id}
                disabled={!onTask}
                onClick={() => navigate(onTask, id)}
              >
                Action {i + 1} · {id === m.activeChild ? "current" : "recorded"}{" "}
                ↗
              </button>
            ))}
            <h3>Automation history</h3>
            <ol className="task-event-log">
              {events.map((e) => (
                <li key={e.sequence}>
                  <time>{taskTime(e.at)}</time>
                  <span>{e.detail}</span>
                </li>
              ))}
            </ol>
            {events.length > 0 && events.length % 100 === 0 && (
              <button
                onClick={() =>
                  void missionClient
                    .events(m.id, events[events.length - 1]!.sequence)
                    .then((rows) => setEvents((old) => [...old, ...rows]))
                    .catch((e) => setError(String(e)))
                }
              >
                Load more activity
              </button>
            )}
            <button onClick={() => void exportBrief("json")}>
              Export complete automation record…
            </button>
          </>
        )}
        {tab === "brief" && (
          <div className="mission-brief">
            <Suspense fallback={<p>Loading the research brief…</p>}>
              <ReportViewer markdown={m.brief} initialContentsOpen={false} />
            </Suspense>
          </div>
        )}
      </div>
      {evidence && (
        <section
          className="mission-evidence"
          aria-label="Retained research automation evidence"
        >
          <div className="task-section-heading">
            <h3>{evidence.id}</h3>
            <button
              aria-label="Close evidence"
              onClick={() => {
                evidenceVersion.current++;
                setEvidence(null);
              }}
            >
              ×
            </button>
          </div>
          <pre className="task-result">
            {typeof evidence.value === "string"
              ? evidence.value
              : JSON.stringify(evidence.value, null, 2)}
          </pre>
        </section>
      )}
    </section>
  );
}

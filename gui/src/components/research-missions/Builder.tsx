import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { taskClient, type SessionChoice } from "../../lib/taskClient";
import {
  missionClient,
  newMission,
  type Mission,
  type MissionChoices,
  type MissionDefinition,
  type MissionMethod,
  type MissionMode,
} from "../../lib/missionClient";

const draftKey = "pipeline.researchMission.draft.v1";
function recover(sessionId?: string | null) {
  try {
    const saved = JSON.parse(localStorage.getItem(draftKey) ?? "null");
    if (
      saved?.definition?.schemaVersion === 1 &&
      typeof saved.sessionId === "string" &&
      typeof saved.operationId === "string"
    )
      return saved as {
        definition: MissionDefinition;
        sessionId: string;
        operationId: string;
      };
  } catch {
    /* An unavailable local draft does not prevent preparation. */
  }
  return {
    definition: newMission(),
    sessionId: sessionId ?? "",
    operationId: crypto.randomUUID(),
  };
}
const toggle = (values: string[], id: string) =>
  values.includes(id) ? values.filter((v) => v !== id) : [...values, id];
export default function MissionBuilder({
  initialSessionId,
  onPrepared,
  onClose,
}: {
  initialSessionId?: string | null;
  onPrepared: (m: Mission) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState(() => recover(initialSessionId));
  const [sessions, setSessions] = useState<SessionChoice[]>([]);
  const [choices, setChoices] = useState<MissionChoices | null>(null);
  const [methods, setMethods] = useState<MissionMethod[]>([]);
  const [profiles, setProfiles] = useState<{ id: string; name: string }[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const choiceVersion = useRef(0);
  useEffect(() => {
    let live = true;
    void Promise.all([
      taskClient.sessions(),
      invoke<{ id: string; name: string }[]>("list_profiles"),
    ])
      .then(([s, p]) => {
        if (live) {
          setSessions(s.filter((v) => v.workspaceName !== null));
          setProfiles(p);
        }
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, []);
  useEffect(() => {
    try {
      localStorage.setItem(draftKey, JSON.stringify(draft));
    } catch {
      /* The prepared mission is persisted natively. */
    }
  }, [draft]);
  useEffect(() => {
    const version = ++choiceVersion.current;
    setChoices(null);
    setMethods([]);
    if (draft.sessionId)
      void missionClient
        .choices(draft.sessionId)
        .then(async (c) => {
          if (version !== choiceVersion.current) return;
          setChoices(c);
          const available = await missionClient.methods(c.workspaceId);
          if (version === choiceVersion.current) setMethods(available);
        })
        .catch((e) => {
          if (version === choiceVersion.current) setError(String(e));
        });
    return () => {
      choiceVersion.current++;
    };
  }, [draft.sessionId]);
  function patch(update: Partial<MissionDefinition>) {
    setDraft((d) => ({
      ...d,
      definition: { ...d.definition, ...update },
      operationId: crypto.randomUUID(),
    }));
  }
  function policy(update: Partial<MissionDefinition["policy"]>) {
    patch({ policy: { ...draft.definition.policy, ...update } });
  }
  const d = draft.definition;
  async function prepare() {
    setBusy(true);
    setError("");
    try {
      const m = await missionClient.prepare(
        d,
        draft.sessionId,
        draft.operationId,
      );
      localStorage.removeItem(draftKey);
      onPrepared(m);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <form
      className="task-detail mission-builder"
      aria-label="New research automation"
      onSubmit={(e) => {
        e.preventDefault();
        void prepare();
      }}
    >
      <div className="task-section-heading">
        <h2>New research automation</h2>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close research automation builder"
        >
          ×
        </button>
      </div>
      <p className="task-muted">
        Set the question, available tools, and criteria for completing the
        research.
      </p>
      <fieldset disabled={busy} className="mission-fields">
        <label>
          Source conversation
          <select
            value={draft.sessionId}
            onChange={(e) => {
              const sessionId = e.target.value;
              setError("");
              setDraft((old) => ({
                ...old,
                sessionId,
                operationId: crypto.randomUUID(),
                definition: {
                  ...old.definition,
                  policy: newMission().policy,
                  methodIds: [],
                },
              }));
            }}
          >
            <option value="">Choose a project conversation</option>
            {sessions.map((s) => (
              <option key={s.id} value={s.id}>
                {s.title} · {s.workspaceName}
              </option>
            ))}
          </select>
        </label>
        <label>
          Starting point
          <select
            value={d.mode}
            onChange={(e) => {
              const next = newMission(e.target.value as MissionMode);
              patch({
                mode: next.mode,
                name: next.name,
                objective: next.objective,
                criteria: next.criteria,
              });
            }}
          >
            {(
              [
                "theory",
                "empirical",
                "quantitative",
                "literature",
                "discovery",
                "maintenance",
              ] as const
            ).map((v) => (
              <option key={v} value={v}>
                {
                  {
                    theory: "Theory investigation",
                    empirical: "Empirical investigation",
                    quantitative: "Quantitative model",
                    literature: "Literature investigation",
                    discovery: "Research discovery",
                    maintenance: "Project maintenance",
                  }[v]
                }
              </option>
            ))}
          </select>
        </label>
        <label>
          Automation name
          <input
            maxLength={160}
            value={d.name}
            onChange={(e) => patch({ name: e.target.value })}
          />
        </label>
        <label>
          Research question and remit
          <textarea
            rows={5}
            value={d.objective}
            onChange={(e) => patch({ objective: e.target.value })}
            maxLength={8000}
          />
        </label>
        <label>
          Current understanding
          <textarea
            rows={3}
            value={d.background}
            onChange={(e) => patch({ background: e.target.value })}
            placeholder="What is known, what has failed, and any important constraints"
            maxLength={16000}
          />
        </label>
        <label>
          Completion criteria <span className="task-muted">One per line</span>
          <textarea
            rows={5}
            value={d.criteria.join("\n")}
            onChange={(e) => patch({ criteria: e.target.value.split("\n") })}
          />
        </label>
        <div className="mission-budget-grid">
          <label>
            Investigation rounds
            <input
              type="number"
              min={1}
              max={32}
              value={d.budget.maxRounds}
              onChange={(e) =>
                patch({
                  budget: { ...d.budget, maxRounds: Number(e.target.value) },
                })
              }
            />
          </label>
          <label>
            Managed actions
            <input
              type="number"
              min={3}
              max={256}
              value={d.budget.maxActions}
              onChange={(e) =>
                patch({
                  budget: { ...d.budget, maxActions: Number(e.target.value) },
                })
              }
            />
          </label>
          <label>
            Active hours
            <input
              type="number"
              min={0.1}
              max={168}
              step={0.1}
              value={d.budget.activeSeconds / 3600}
              onChange={(e) =>
                patch({
                  budget: {
                    ...d.budget,
                    activeSeconds: Math.round(Number(e.target.value) * 3600),
                  },
                })
              }
            />
          </label>
          <label>
            Minutes per action
            <input
              type="number"
              min={0.5}
              max={120}
              step={0.5}
              value={d.budget.actionTimeoutSeconds / 60}
              onChange={(e) =>
                patch({
                  budget: {
                    ...d.budget,
                    actionTimeoutSeconds: Math.round(
                      Number(e.target.value) * 60,
                    ),
                  },
                })
              }
            />
          </label>
          <label>
            Deadline in hours
            <input
              type="number"
              min={1}
              max={720}
              value={d.budget.deadlineHours}
              onChange={(e) =>
                patch({
                  budget: {
                    ...d.budget,
                    deadlineHours: Number(e.target.value),
                  },
                })
              }
            />
          </label>
          <label>
            Stop after stagnant rounds
            <input
              type="number"
              min={1}
              max={8}
              value={d.budget.maxStagnantRounds}
              onChange={(e) =>
                patch({
                  budget: {
                    ...d.budget,
                    maxStagnantRounds: Number(e.target.value),
                  },
                })
              }
            />
          </label>
        </div>
        {choices && (
          <>
            <p className="task-muted">Project folder: {choices.root}</p>
            <label className="mission-check">
              <input
                type="checkbox"
                disabled={!choices.canEdit}
                checked={d.policy.allowEdits}
                onChange={(e) => policy({ allowEdits: e.target.checked })}
              />
              Allow edits in the working copy
            </label>
            {!choices.canEdit && (
              <p className="task-muted">
                To allow edits, choose a conversation with a working copy and
                Edit access.
              </p>
            )}
            <label className="mission-check">
              <input
                type="checkbox"
                disabled={!choices.commandNetwork}
                checked={d.policy.commandNetwork}
                onChange={(e) => policy({ commandNetwork: e.target.checked })}
              />
              Use the conversation’s existing command network access
            </label>
            <details>
              <summary>Computation and project changes</summary>
              <p className="task-muted">
                Select the checks and exact captured variants this automation
                may choose between. Host execution retains its existing
                authorization and input checks.
              </p>
              {!choices.canHostCompute && (
                <p className="task-muted">
                  This task conversation uses its native sandbox for
                  computation. Select a project conversation without a task
                  binding to delegate host checks.
                </p>
              )}
              <fieldset>
                <legend>Configured host checks</legend>
                {choices.checks.length ? (
                  choices.checks.map((c) => (
                    <label className="mission-check" key={c.id}>
                      <input
                        type="checkbox"
                        disabled={!choices.canHostCompute}
                        checked={d.policy.checkProfileIds.includes(c.id)}
                        onChange={() =>
                          policy({
                            checkProfileIds: toggle(
                              d.policy.checkProfileIds,
                              c.id,
                            ),
                          })
                        }
                      />
                      {c.name}
                    </label>
                  ))
                ) : (
                  <p className="task-muted">
                    Configure and authorize checks in Analyze.
                  </p>
                )}
              </fieldset>
              <fieldset>
                <legend>Captured plans and experiment grids</legend>
                {choices.experiments.length ? (
                  choices.experiments.map((c) => (
                    <label className="mission-check" key={c.id}>
                      <input
                        type="checkbox"
                        disabled={!choices.canHostCompute}
                        checked={d.policy.experimentIds.includes(c.id)}
                        onChange={() =>
                          policy({
                            experimentIds: toggle(d.policy.experimentIds, c.id),
                          })
                        }
                      />
                      {c.title}{" "}
                      <small>
                        {c.kind === "experiment_plan"
                          ? "parameter grid"
                          : "captured plan"}
                      </small>
                    </label>
                  ))
                ) : (
                  <p className="task-muted">
                    Prepare a captured analysis or parameter grid in Analyze.
                  </p>
                )}
              </fieldset>
              <fieldset>
                <legend>Wake on changes from these monitors</legend>
                {choices.monitors.length ? (
                  choices.monitors.map((c) => (
                    <label className="mission-check" key={c.id}>
                      <input
                        type="checkbox"
                        checked={d.policy.monitorIds.includes(c.id)}
                        onChange={() =>
                          policy({
                            monitorIds: toggle(d.policy.monitorIds, c.id),
                          })
                        }
                      />
                      {c.title}
                    </label>
                  ))
                ) : (
                  <p className="task-muted">
                    Create project monitors in Automate to enable
                    change-triggered work.
                  </p>
                )}
              </fieldset>
            </details>
          </>
        )}
        <label>
          Additional review
          <select
            value={d.policy.reviewProfileId ?? ""}
            onChange={(e) =>
              policy({ reviewProfileId: e.target.value || null })
            }
          >
            <option value="">Separate project challenge only</option>
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        {!!methods.length && (
          <details>
            <summary>Retained research methods</summary>
            <p className="task-muted">
              Choose methods to reuse. Their evidence and limitations are
              included.
            </p>
            {methods.map((method) => (
              <label className="mission-check" key={method.id}>
                <input
                  type="checkbox"
                  checked={d.methodIds.includes(method.id)}
                  onChange={() =>
                    patch({ methodIds: toggle(d.methodIds, method.id) })
                  }
                />
                <span>
                  {method.name}
                  <small>{method.whenToUse}</small>
                </span>
              </label>
            ))}
          </details>
        )}
      </fieldset>
      {error && (
        <p role="alert" className="task-error">
          {error}
        </p>
      )}
      <button
        className="task-primary"
        disabled={
          busy ||
          !choices ||
          !d.name.trim() ||
          !d.objective.trim() ||
          !d.criteria.length ||
          d.criteria.some((c) => !c.trim())
        }
      >
        {busy ? "Preparing…" : "Prepare research automation →"}
      </button>
      <p className="task-muted">
        Review the preview before starting. Starting allows the automation to
        run actions within the limits above. Keep Pipeline running and this
        computer awake.
      </p>
    </form>
  );
}

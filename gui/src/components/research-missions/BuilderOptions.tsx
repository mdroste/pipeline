import { displayPath } from "../../lib/displayPath";
import { AllowanceField, ReviewEstimate } from "../tasks/Budget";
/** Form fields stay owned by the builder; changing sections never resets a draft. */
import type {
  MissionChoices,
  MissionDefinition,
  MissionMethod,
} from "../../lib/missionClient";

type FieldsProps = {
  definition: MissionDefinition;
  onChange: (update: Partial<MissionDefinition>) => void;
};

export function BudgetFields({ definition: d, onChange: patch }: FieldsProps) {
  const actionsPerRound = d.policy.reviewProfileId ? 4 : 3;
  const fullRounds = Math.min(
    d.budget.maxRounds,
    Math.floor(d.budget.maxActions / actionsPerRound),
  );
  return (
    <section className="mission-builder-card">
      <h3>Research limits</h3>
      <p>
        One investigated and challenged result needs {actionsPerRound} actions:
        plan, investigate, {d.policy.reviewProfileId ? "Review, " : ""}and
        challenge.{" "}
        {fullRounds < 1
          ? "This action limit cannot reach that milestone."
          : `Up to ${fullRounds} full ${fullRounds === 1 ? "round fits" : "rounds fit"} before retries or extra planning turns.`}
      </p>
      <ReviewEstimate profile={d.policy.reviewProfileId} />
      <div className="mission-budget-grid">
        <AllowanceField
          cents={d.budget.maxApiCostCents}
          onChange={(maxApiCostCents) =>
            patch({ budget: { ...d.budget, maxApiCostCents } })
          }
        />
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
          Maximum actions
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
                  actionTimeoutSeconds: Math.round(Number(e.target.value) * 60),
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
          Stop after this many rounds without progress
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
    </section>
  );
}

const toggle = (values: string[], id: string) =>
  values.includes(id) ? values.filter((v) => v !== id) : [...values, id];

export function ToolFields({
  definition: d,
  onChange: patch,
  choices,
  methods,
  profiles,
}: FieldsProps & {
  choices: MissionChoices | null;
  methods: MissionMethod[];
  profiles: { id: string; name: string }[];
}) {
  const policy = (update: Partial<MissionDefinition["policy"]>) =>
    patch({ policy: { ...d.policy, ...update } });
  return (
    <div className="mission-builder-columns">
      <section className="mission-builder-card mission-builder-stack">
        <h3>Project access and review</h3>
        {choices ? (
          <>
            <p className="task-muted mission-builder-path">
              Project folder: {displayPath(choices.root)}
            </p>
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
          </>
        ) : (
          <p className="task-muted">
            Choose a source conversation to see its project permissions.
          </p>
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
      </section>
      <section className="mission-builder-card mission-builder-stack">
        <h3>Computation and project changes</h3>
        <p className="task-muted">
          Choose checks, saved analysis plans, and project monitors. Each keeps
          its existing permissions.
        </p>
        {choices ? (
          <>
            {!choices.canHostCompute && (
              <p className="task-muted">
                This conversation runs code in its task’s restricted working
                area. To use configured local checks, choose a project
                conversation that is not linked to a task.
              </p>
            )}
            <details>
              <summary>
                Configured local checks{" "}
                <span>{d.policy.checkProfileIds.length} selected</span>
              </summary>
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
            </details>
            <details>
              <summary>
                Captured plans and experiment grids{" "}
                <span>{d.policy.experimentIds.length} selected</span>
              </summary>
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
                    <span>
                      {c.title}{" "}
                      <small>
                        {c.kind === "experiment_plan"
                          ? "parameter grid"
                          : "captured plan"}
                      </small>
                    </span>
                  </label>
                ))
              ) : (
                <p className="task-muted">
                  Prepare a captured analysis or parameter grid in Analyze.
                </p>
              )}
            </details>
            <details>
              <summary>
                Wake on changes from these monitors{" "}
                <span>{d.policy.monitorIds.length} selected</span>
              </summary>
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
                  Create project monitors in Automate to enable change-triggered
                  work.
                </p>
              )}
            </details>
          </>
        ) : (
          <p className="task-muted">
            Available tools load after you choose a source conversation.
          </p>
        )}
        {!!methods.length && (
          <details>
            <summary>
              Saved research methods <span>{d.methodIds.length} selected</span>
            </summary>
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
      </section>
    </div>
  );
}

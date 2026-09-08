import { useState } from "react";
import {
  deskClient,
  reference,
  type DeskRecord,
  type PlanStatus,
  type OpenResearchObject,
} from "../../lib/deskClient";
import { workbenchClient } from "../../lib/workbenchClient";
import type { AssetSpec, Entry, Publication } from "../../lib/programClient";
import {
  button,
  input,
  card,
  muted,
  Field,
  Inspect,
  ObjectSelect,
  Artifacts,
  operation,
  useProgram,
  type DeskProps,
} from "./shared";
const empty: AssetSpec = {
  kind: "table",
  title: "Research results",
  entries: [],
  digits: 3,
  uncertainty: "ci",
  notes: "",
  sampleComparisonRationale: null,
};
export default function Assets(props: DeskProps) {
  const p = useProgram(props, ["publication_asset", "figure_recipe"]);
  const [spec, setSpec] = useState<AssetSpec>(empty),
    [source, setSource] = useState<OpenResearchObject | null>(null),
    [label, setLabel] = useState(""),
    [profile, setProfile] = useState(""),
    [supersedes, setSupersedes] = useState<string | null>(null),
    [preview, setPreview] = useState<PlanStatus | null>(null),
    [recipe, setRecipe] = useState<string | null>(null),
    [execution, setExecution] = useState("");
  const { workspaceId, onOpen, onError } = props;
  const records = (p.records.publication_asset ??
    []) as DeskRecord<Publication>[];
  const changeEntry = (i: number, patch: Partial<Entry>) =>
    setSpec((s) => ({
      ...s,
      entries: s.entries.map((e, j) => (j === i ? { ...e, ...patch } : e)),
    }));
  return (
    <div className="space-y-5">
      <section className={card}>
        <h2 className="font-semibold">Publication assets</h2>
        <p className={muted}>
          Build tables and scientific figures from retained numeric results.
          Values, units, samples, and source revisions travel with every asset.
        </p>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Asset title">
            <input
              className={input}
              value={spec.title}
              onChange={(e) =>
                setSpec((s) => ({ ...s, title: e.target.value }))
              }
            />
          </Field>
          <Field label="Asset format">
            <select
              className={input}
              value={spec.kind}
              onChange={(e) => {
                setSpec((s) => ({
                  ...s,
                  kind: e.target.value as AssetSpec["kind"],
                  entries: [],
                }));
                setSource(null);
                setSupersedes(null);
              }}
            >
              <option value="table">Table: Markdown / TeX / CSV</option>
              <option value="coefficient">Coefficient and interval plot</option>
              <option value="irf">Impulse-response overlay</option>
            </select>
          </Field>
        </div>
        <ObjectSelect
          choices={p.choices?.objects ?? []}
          value={source}
          label={
            spec.kind === "irf" ? "Numeric series" : "Structured numeric result"
          }
          filter={(o) =>
            spec.kind === "irf"
              ? o.reference.kind === "record" && o.title === "series"
              : o.reference.kind === "result"
          }
          onChange={(r) => {
            setSource(r);
            if (!label)
              setLabel(
                p.choices?.objects.find((o) => o.reference.id === r?.id)
                  ?.title ?? "",
              );
          }}
        />
        <Field label="Display label">
          <input
            className={input}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
        </Field>
        <button
          className={button}
          disabled={!source || !label.trim()}
          onClick={() => {
            if (source)
              setSpec((s) => ({
                ...s,
                entries: [...s.entries, { label, source, manual: null }],
              }));
            setLabel("");
            setSource(null);
          }}
        >
          Add result in this order
        </button>
        {spec.entries.map((entry, i) => (
          <div className="space-y-2 rounded border p-3" key={i}>
            <div className="flex gap-2">
              <input
                className={input}
                aria-label={`Entry ${i + 1} label`}
                value={entry.label}
                onChange={(e) => changeEntry(i, { label: e.target.value })}
              />
              <button className={button} onClick={() => onOpen(entry.source)}>
                Source
              </button>
              <button
                className={button}
                onClick={() =>
                  setSpec((s) => ({
                    ...s,
                    entries: s.entries.filter((_, j) => j !== i),
                  }))
                }
              >
                Remove
              </button>
            </div>
            {spec.kind !== "irf" && (
              <details>
                <summary className="text-xs">Manual numeric override</summary>
                <p className={muted}>
                  An override clears uncertainty and remains visibly manual. The
                  original estimate is retained.
                </p>
                <label className="text-xs">
                  <input
                    type="checkbox"
                    checked={!!entry.manual}
                    onChange={(e) =>
                      changeEntry(i, {
                        manual: e.target.checked
                          ? { value: 0, reason: "" }
                          : null,
                      })
                    }
                  />{" "}
                  Record a manual value
                </label>
                {entry.manual && (
                  <div className="grid grid-cols-2 gap-2">
                    <Field label="Manual value">
                      <input
                        className={input}
                        type="number"
                        value={entry.manual.value}
                        onChange={(e) =>
                          changeEntry(i, {
                            manual: {
                              ...entry.manual!,
                              value: Number(e.target.value),
                            },
                          })
                        }
                      />
                    </Field>
                    <Field label="Reason for override">
                      <input
                        className={input}
                        value={entry.manual.reason}
                        onChange={(e) =>
                          changeEntry(i, {
                            manual: {
                              ...entry.manual!,
                              reason: e.target.value,
                            },
                          })
                        }
                      />
                    </Field>
                  </div>
                )}
              </details>
            )}
          </div>
        ))}
        <div className="grid grid-cols-2 gap-3">
          <Field label="Displayed decimal places">
            <input
              className={input}
              type="number"
              min="0"
              max="10"
              value={spec.digits}
              onChange={(e) =>
                setSpec((s) => ({ ...s, digits: Number(e.target.value) }))
              }
            />
          </Field>
          <Field label="Uncertainty convention">
            <select
              className={input}
              value={spec.uncertainty}
              onChange={(e) =>
                setSpec((s) => ({
                  ...s,
                  uncertainty: e.target.value as AssetSpec["uncertainty"],
                }))
              }
            >
              <option value="ci">Retained confidence interval</option>
              <option value="se">One standard error</option>
              <option value="none">No uncertainty displayed</option>
            </select>
          </Field>
        </div>
        <Field label="Asset notes">
          <textarea
            className={input}
            value={spec.notes}
            onChange={(e) => setSpec((s) => ({ ...s, notes: e.target.value }))}
          />
        </Field>
        <Field label="Rationale for comparing different samples (if needed)">
          <input
            className={input}
            value={spec.sampleComparisonRationale ?? ""}
            onChange={(e) =>
              setSpec((s) => ({
                ...s,
                sampleComparisonRationale: e.target.value || null,
              }))
            }
          />
        </Field>
        {spec.kind !== "table" && (
          <Field label="Local Python profile with matplotlib">
            <select
              className={input}
              value={profile}
              onChange={(e) => setProfile(e.target.value)}
            >
              <option value="">Choose configured Python toolchain</option>
              {p.choices?.profiles
                .filter((pr) => pr.adapter === "command")
                .map((pr) => (
                  <option key={pr.id} value={pr.id}>
                    {pr.name}
                  </option>
                ))}
            </select>
          </Field>
        )}
        <button
          className={button}
          disabled={
            p.busy ||
            !spec.entries.length ||
            (spec.kind !== "table" && !profile)
          }
          onClick={() =>
            void p.run(async () => {
              if (spec.kind === "table") {
                await p.call({
                  action: "table",
                  spec,
                  supersedes,
                  operationId: operation(),
                });
                setSupersedes(null);
              } else {
                const r = await p.call<DeskRecord<{ planId: string }>>({
                  action: "figurePlan",
                  request: {
                    workspaceId,
                    spec,
                    pythonProfileId: profile,
                    operationId: operation(),
                  },
                });
                setRecipe(r.id);
                setPreview(
                  await deskClient.planStatus(workspaceId, r.body.planId),
                );
              }
            })
          }
        >
          {spec.kind === "table"
            ? "Generate table draft"
            : "Prepare reproducible figure plan"}
        </button>
        {supersedes && (
          <p className={muted}>
            This creates a new asset version. Earlier files and accepted changes
            remain available for comparison.
          </p>
        )}
      </section>
      {p.records.figure_recipe?.map((r) => (
        <button
          className={button}
          key={r.id}
          onClick={() =>
            void p.run(async () => {
              setRecipe(r.id);
              setPreview(
                await deskClient.planStatus(
                  workspaceId,
                  (r.body as { planId: string }).planId,
                ),
              );
            })
          }
        >
          Figure plan: {r.title}
        </button>
      ))}
      {preview && (
        <section className={card}>
          <h3 className="font-semibold">Review figure execution</h3>
          <Inspect
            value={preview.record.body}
            label="Plot source, numeric inputs, command and output manifest"
          />
          <p className={muted}>
            Python runs with your computer’s file permissions. The figure is saved
            as SVG, PDF, and PNG, along with the matplotlib version and run details.
          </p>
          <button
            className={button}
            disabled={p.busy || preview.authorized}
            onClick={() =>
              void p.run(async () =>
                setPreview(
                  await deskClient.authorizePlan(
                    workspaceId,
                    preview.record.id,
                    preview.record.contentHash,
                  ),
                ),
              )
            }
          >
            Authorize exact plotting plan
          </button>
          <button
            className={button}
            disabled={p.busy || !preview.authorized}
            onClick={() =>
              void p.run(async () => {
                const profile = preview.record.body.profile as { id: string };
                await workbenchClient.runExecution({
                  planId: preview.record.id,
                  profileId: profile.id,
                  sessionId: null,
                  testOnly: true,
                  operationId: operation(),
                });
              })
            }
          >
            Run plot and retain outputs
          </button>
          <button className={button} onClick={() => void p.run(p.refresh)}>
            Refresh completed runs
          </button>
          <Field label="Completed figure execution">
            <select
              className={input}
              value={execution}
              onChange={(e) => setExecution(e.target.value)}
            >
              <option value="">Choose matching completed execution</option>
              {p.choices?.executions
                .filter(
                  (e) =>
                    e.outcome === "completed" &&
                    e.inputManifest.planId === preview.record.id,
                )
                .map((e) => (
                  <option key={e.id} value={e.id}>
                    {new Date(e.createdAt).toLocaleString()} · {e.id.slice(-8)}
                  </option>
                ))}
            </select>
          </Field>
          <button
            className={button}
            disabled={p.busy || !recipe || !execution}
            onClick={() =>
              void p.run(async () => {
                await p.call({
                  action: "figureAdopt",
                  recipeId: recipe,
                  executionId: execution,
                  operationId: operation(),
                });
              })
            }
          >
            Adopt figure as publication asset
          </button>
        </section>
      )}
      {records.map((r) => (
        <section className={card} key={r.id}>
          <div className="flex justify-between gap-2">
            <h3 className="font-semibold">
              {r.title} · {r.body.spec.kind}
            </h3>
            <button
              className={button}
              onClick={() => {
                setSpec(r.body.spec);
                setSupersedes(r.id);
              }}
            >
              Edit as new version
            </button>
          </div>
          <p className={muted}>
            {r.contentHash.slice(0, 12)} ·{" "}
            {new Date(r.createdAt).toLocaleString()}
          </p>
          {r.body.entries.map((e, i) => (
            <button key={i} className={button} onClick={() => onOpen(e.source)}>
              {e.label}
              {e.manual ? " · manual" : ""}
            </button>
          ))}
          <button className={button} onClick={() => onOpen(reference(r))}>
            Inspect provenance
          </button>
          <Artifacts
            workspaceId={workspaceId}
            recordId={r.id}
            artifacts={r.body.artifacts}
            choices={p.choices}
            onError={onError}
          />
        </section>
      ))}
    </div>
  );
}

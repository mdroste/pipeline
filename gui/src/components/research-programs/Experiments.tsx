import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import type {
  Artifact,
  Factor,
  Exclusion,
  GridStatus,
} from "../../lib/programClient";
import { studioClient } from "../../lib/studioClient";
import Jobs from "../research-studio/Jobs";
import {
  button,
  input,
  card,
  muted,
  Field,
  Inspect,
  operation,
  useProgram,
  type DeskProps,
} from "./shared";
export default function Experiments(props: DeskProps & { active?: boolean }) {
  const { workspaceId, onError } = props;
  const p = useProgram(props, ["experiment_plan"]);
  const [title, setTitle] = useState("Specification comparison"),
    [question, setQuestion] = useState(""),
    [base, setBase] = useState(""),
    [factors, setFactors] = useState<{ name: string; values: string }[]>([
      { name: "calibration", values: "0.9, 0.95, 0.99" },
    ]),
    [exclusions, setExclusions] = useState("[]"),
    [rationale, setRationale] = useState(""),
    [maxRuns, setMaxRuns] = useState(16),
    [maxSeconds, setMaxSeconds] = useState(3600),
    [preview, setPreview] = useState<GridStatus | null>(null),
    [sort, setSort] = useState<"ordinal" | "state">("ordinal");
  useEffect(() => {
    if (props.active === false || !preview || !["running", "paused"].includes(preview.run.state)) return;
    let alive = true;
    const timer = setInterval(() => {
      void p
        .call<GridStatus>({ action: "experimentStatus", id: preview.record.id })
        .then((v) => {
          if (alive) setPreview(v);
        })
        .catch(() => undefined);
    }, 3000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [workspaceId, preview?.record.id, preview?.run.state, props.active]);
  const prepare = () =>
    p.run(async () => {
      const expanded: Factor[] = factors.map((f) => ({
        name: f.name,
        values: JSON.parse(`[${f.values}]`) as Factor["values"],
      }));
      const rule = JSON.parse(exclusions) as Exclusion[];
      const record = await p.call<{ id: string }>({
        action: "experimentPrepare",
        request: {
          workspaceId,
          title,
          question,
          basePlanId: base,
          factors: expanded,
          exclusions: rule,
          interpretation: rationale,
          maxRuns,
          maxSeconds,
          operationId: operation(),
        },
      });
      setPreview(
        await p.call<GridStatus>({ action: "experimentStatus", id: record.id }),
      );
    });
  const rows =
    preview?.plan.specifications
      .map((s) => ({
        ...s,
        attempt: preview.attempts.find((a) => a.ordinal === s.ordinal),
      }))
      .sort((a, b) =>
        sort === "ordinal"
          ? a.ordinal - b.ordinal
          : (a.attempt?.state ?? "not attempted").localeCompare(
              b.attempt?.state ?? "not attempted",
            ),
      ) ?? [];
  return (
    <div className="space-y-5">
      <section className={card}>
        <h2 className="font-semibold">Specification grids</h2>
        <p className={muted}>
          Enumerate the complete comparison before launch. Each specification
          gets a fresh copy of the captured inputs; runs proceed serially.
          Parameters are available to the script through
          PIPELINE_PARAMETERS_FILE.
        </p>
        <Field label="Experiment title">
          <input
            className={input}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </Field>
        <Field label="Research question">
          <textarea
            className={input}
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
          />
        </Field>
        <Field label="Captured baseline plan">
          <select
            className={input}
            value={base}
            onChange={(e) => setBase(e.target.value)}
          >
            <option value="">Choose captured execution</option>
            {p.choices?.plans.map((c) => (
              <option key={c.id} value={c.id}>
                {c.title} · {c.contentHash.slice(0, 8)}
              </option>
            ))}
          </select>
        </Field>
        {factors.map((f, i) => (
          <div className="grid grid-cols-2 gap-2" key={i}>
            <Field label={`Factor ${i + 1} name`}>
              <input
                className={input}
                value={f.name}
                onChange={(e) =>
                  setFactors((old) =>
                    old.map((v, j) =>
                      j === i ? { ...v, name: e.target.value } : v,
                    ),
                  )
                }
              />
            </Field>
            <Field label="Values (numbers, true/false, or quoted strings)">
              <input
                className={input}
                value={f.values}
                onChange={(e) =>
                  setFactors((old) =>
                    old.map((v, j) =>
                      j === i ? { ...v, values: e.target.value } : v,
                    ),
                  )
                }
              />
            </Field>
          </div>
        ))}
        <button
          className={button}
          disabled={factors.length >= 8}
          onClick={() =>
            setFactors((old) => [...old, { name: "", values: "" }])
          }
        >
          Add factor
        </button>
        <details>
          <summary className="text-xs">Exclusion rules</summary>
          <p className={muted}>
            Excluded specifications remain in the attempt table. Example:{" "}
            {`[{"matches":{"calibration":0.9},"reason":"Outside the declared domain"}]`}
          </p>
          <textarea
            className={input}
            aria-label="Exclusion rules"
            value={exclusions}
            onChange={(e) => setExclusions(e.target.value)}
          />
        </details>
        <Field label="Interpretation and selection rationale">
          <textarea
            className={input}
            value={rationale}
            onChange={(e) => setRationale(e.target.value)}
            placeholder="State the comparison and limitations before inspecting outcomes."
          />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Maximum specifications">
            <input
              className={input}
              type="number"
              min="1"
              max="64"
              value={maxRuns}
              onChange={(e) => setMaxRuns(Number(e.target.value))}
            />
          </Field>
          <Field label="Total timeout budget (seconds)">
            <input
              className={input}
              type="number"
              min="1"
              max="604800"
              value={maxSeconds}
              onChange={(e) => setMaxSeconds(Number(e.target.value))}
            />
          </Field>
        </div>
        <button
          className={button}
          disabled={p.busy || !base || !question.trim() || !rationale.trim()}
          onClick={() => void prepare()}
        >
          Prepare complete grid
        </button>
      </section>
      <div className="flex flex-wrap gap-2">
        {p.records.experiment_plan?.map((r) => (
          <button
            key={r.id}
            className={button}
            onClick={() =>
              void p.run(async () =>
                setPreview(
                  await p.call<GridStatus>({
                    action: "experimentStatus",
                    id: r.id,
                  }),
                ),
              )
            }
          >
            {r.title}
          </button>
        ))}
      </div>
      {preview && (
        <section className={card}>
          <h2 className="font-semibold">
            {preview.record.title} · {preview.run.state}
          </h2>
          <p className="text-sm">
            {preview.plan.totalRuns} runs · maximum{" "}
            {preview.plan.timeoutBudgetSeconds} seconds ·{" "}
            {(preview.plan.capturedInputBytes / 1048576).toFixed(2)} MiB
            captured inputs before per-run copies
          </p>
          <p className={muted}>
            Every enumerated command has host access. Toolchain availability and
            license limits still apply. This plan does not change the accepted
            baseline.
          </p>
          <Inspect
            value={preview.basePlan.body}
            label="Review captured command and complete plan"
          />
          {preview.run.reason && <p role="alert">{preview.run.reason}</p>}
          <div className="flex flex-wrap gap-2">
            {["ready", "paused"].includes(preview.run.state) && (
              <button
                className={button}
                disabled={p.busy}
                onClick={() =>
                  void p.run(async () =>
                    setPreview(
                      await p.call<GridStatus>({
                        action: "experimentControl",
                        id: preview.record.id,
                        control: "start",
                        fingerprint: preview.record.contentHash,
                      }),
                    ),
                  )
                }
              >
                Authorize this family and{" "}
                {preview.run.state === "paused" ? "resume" : "start"}
              </button>
            )}
            {["running", "attention", "paused"].includes(preview.run.state) &&
              ["pause", "cancel", "reconcile"].map((control) => (
                <button
                  className={button}
                  key={control}
                  disabled={p.busy}
                  onClick={() =>
                    void p.run(async () =>
                      setPreview(
                        await p.call<GridStatus>({
                          action: "experimentControl",
                          id: preview.record.id,
                          control,
                          fingerprint: preview.record.contentHash,
                        }),
                      ),
                    )
                  }
                >
                  {control}
                </button>
              ))}
            <button
              className={button}
              onClick={() =>
                void p.run(async () => {
                  const artifact = await p.call<Artifact>({
                    action: "experimentExport",
                    id: preview.record.id,
                  });
                  const path = await save({
                    defaultPath: "complete-experiment-attempts.json",
                  });
                  if (path)
                    await p.call({
                      action: "exportArtifact",
                      id: artifact.id,
                      path,
                    });
                })
              }
            >
              Export full attempted set
            </button>
          </div>
          <Field label="Sort specifications">
            <select
              className={input}
              value={sort}
              onChange={(e) => setSort(e.target.value as typeof sort)}
            >
              <option value="ordinal">Declared order</option>
              <option value="state">Outcome</option>
            </select>
          </Field>
          <div className="overflow-auto">
            <table className="w-full text-left text-xs">
              <thead>
                <tr>
                  <th>Spec</th>
                  <th>Parameters</th>
                  <th>Outcome</th>
                  <th>Exit / elapsed</th>
                  <th>Evidence</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => (
                  <tr className="border-t" key={r.ordinal}>
                    <td className="p-2">{r.ordinal + 1}</td>
                    <td>{JSON.stringify(r.factors)}</td>
                    <td>
                      {r.attempt?.state ??
                        (r.excludedReason ? "excluded" : "not attempted")}
                      {r.excludedReason && <p>{r.excludedReason}</p>}
                    </td>
                    <td>
                      {r.attempt?.receipt.exitStatus ?? "—"} /{" "}
                      {r.attempt?.receipt.startedAt &&
                      r.attempt?.receipt.endedAt
                        ? `${((Date.parse(r.attempt.receipt.endedAt) - Date.parse(r.attempt.receipt.startedAt)) / 1000).toFixed(1)}s`
                        : "—"}
                    </td>
                    <td>
                      {r.attempt && (
                        <Inspect
                          value={r.attempt.receipt}
                          label="Run details"
                        />
                      )}
                      {r.attempt?.state === "completed" &&
                        (
                          r.attempt.receipt.outputManifest?.artifacts as
                            | { path: string; artifactId: string }[]
                            | undefined
                        )
                          ?.filter((a) => a.path.endsWith(".json"))
                          .map((a) => (
                            <button
                              key={a.artifactId}
                              className={button}
                              onClick={() =>
                                void p.run(async () => {
                                  await studioClient.mutate(workspaceId, {
                                    action: "importResults",
                                    executionId: r.attempt!.executionId!,
                                    artifactId: a.artifactId,
                                  });
                                })
                              }
                            >
                              Import results: {a.path}
                            </button>
                          ))}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p className={muted}>
            Use Publication assets to compare imported estimates or overlay
            numeric IRFs. Failed, missing, timed-out, and excluded rows remain
            here.
          </p>
        </section>
      )}
      <Jobs
        workspaceId={workspaceId}
        onCompleted={() => {
          void p.refresh().catch(onError);
        }}
      />
    </div>
  );
}

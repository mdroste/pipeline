import { useEffect, useState } from "react";
import {
  studioClient,
  resultRef,
  type BindingCoverage,
  type NumericBinding,
} from "../../lib/studioClient";
import { workbenchClient } from "../../lib/workbenchClient";
import type { ResearchResultV1 } from "../../lib/workbenchTypes";
import {
  button,
  input,
  panel,
  Text,
  Select,
  Field,
  ErrorNotice,
  Inspect,
  useAction,
  type StudioProps,
} from "./shared";
const empty: NumericBinding = {
  result: { executionId: "", resultId: "" },
  anchorId: "",
  role: "prose",
  component: "estimate",
  printed: "",
  precision: 2,
  reportedUnits: "",
  origin: "manual",
  confirmed: false,
};
export default function Bindings({
  workspaceId,
  data,
  onRefresh,
  onDocument,
}: StudioProps) {
  const [coverage, setCoverage] = useState<BindingCoverage[]>([]);
  const [results, setResults] = useState<ResearchResultV1[]>([]);
  const [selected, setSelected] = useState("");
  const [binding, setBinding] = useState(empty);
  const [task, setTask] = useState("");
  const [macro, setMacro] = useState("HeadlineEstimate");
  const [file, setFile] = useState("pipeline_values.tex");
  const [notice, setNotice] = useState("");
  const { error, busy, run, setError } = useAction();
  useEffect(() => {
    let disposed = false;
    void Promise.all([
      studioClient.coverage(workspaceId),
      workbenchClient.listStructuredResults(workspaceId),
    ])
      .then(([c, r]) => {
        if (!disposed) {
          setCoverage(c);
          setResults(r);
        }
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
    };
  }, [workspaceId, setError]);
  const current = coverage.find((c) => c.record.id === selected);
  const set = (
    key: keyof NumericBinding,
    value: NumericBinding[keyof NumericBinding],
  ) => setBinding((b) => ({ ...b, [key]: value }));
  return (
    <div className="space-y-5">
      <ErrorNotice error={error} />
      <section className={panel}>
        <h2 className="font-semibold">Headline-result coverage</h2>
        <p className="text-xs text-gray-500">
          Links retain exact manuscript passages and historical output
          identities. Freshness checks only declared dependencies. An unlinked
          claim has unknown result coverage.
        </p>
        <button
          className={button}
          disabled={busy}
          onClick={() =>
            void run(async () =>
              setCoverage(await studioClient.coverage(workspaceId)),
            )
          }
        >
          Refresh dependent results and passages
        </button>
        <div className="overflow-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr>
                <th>Passage / role</th>
                <th>Quantity</th>
                <th>State</th>
                <th>Action</th>
              </tr>
            </thead>
            <tbody>
              {coverage.map((c) => (
                <tr className="border-t" key={c.record.id}>
                  <td className="max-w-lg p-2">
                    {c.anchor.selection.quote.slice(0, 220)}
                    <p>
                      {c.record.body.role} · {c.record.body.origin}
                    </p>
                  </td>
                  <td>
                    {c.record.body.printed} {c.record.body.reportedUnits}
                    <br />
                    Expected: {c.expected ?? "Unavailable"}
                  </td>
                  <td>
                    {c.state}
                    <br />
                    {c.reasons.join("; ")}
                  </td>
                  <td>
                    <button
                      className={button}
                      onClick={() => {
                        setSelected(c.record.id);
                        setBinding(c.record.body);
                      }}
                    >
                      Inspect / edit link
                    </button>
                    <button
                      className={button}
                      onClick={() => onDocument(c.anchor.selection.revisionId)}
                    >
                      Open exact passage
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {!coverage.length && (
          <p className="text-sm">
            No numeric bindings yet. Select a manuscript passage in Documents,
            then bind its reported value below.
          </p>
        )}
      </section>
      <div className="grid gap-4 lg:grid-cols-2">
        <section className={panel}>
          <h2 className="font-semibold">Bind a reported number</h2>
          <button
            className={button}
            onClick={() => {
              setSelected("");
              setBinding(empty);
            }}
          >
            New binding
          </button>
          <Select
            label="Immutable result"
            value={
              binding.result.executionId
                ? `${binding.result.executionId}/${binding.result.resultId}`
                : ""
            }
            onChange={(value) => {
              const r = results.find(
                (r) => `${r.sourceExecutionId}/${r.resultId}` === value,
              );
              if (r)
                setBinding((b) => ({
                  ...b,
                  result: resultRef(r),
                  reportedUnits: r.units,
                }));
            }}
            options={results.map((r) => ({
              value: `${r.sourceExecutionId}/${r.resultId}`,
              label: `${r.resultId} · ${r.sourceExecutionId.slice(-8)}`,
            }))}
          />
          <Select
            label="Exact manuscript selection"
            value={binding.anchorId}
            onChange={(anchorId) => set("anchorId", anchorId)}
            options={data.anchors.map((a) => ({
              value: a.id,
              label: a.body.selection.quote.slice(0, 180) || a.body.body,
            }))}
          />
          <div className="grid grid-cols-2 gap-3">
            <Select
              label="Reported in"
              value={binding.role}
              onChange={(v) => set("role", v)}
              options={["prose", "table", "figure"].map((v) => ({
                value: v,
                label: v,
              }))}
            />
            <Select
              label="Quantity component"
              value={binding.component}
              onChange={(v) => set("component", v)}
              options={[
                "estimate",
                "standardError",
                "intervalLower",
                "intervalUpper",
              ].map((v) => ({ value: v, label: v }))}
            />
            <Text
              label="Exact printed value in passage"
              value={binding.printed}
              onChange={(v) => set("printed", v)}
            />
            <Field label="Reported decimal places">
              <input
                className={input}
                type="number"
                min={0}
                max={12}
                value={binding.precision}
                onChange={(e) => set("precision", Number(e.target.value))}
              />
            </Field>
          </div>
          <Text
            label="Reported units"
            value={binding.reportedUnits}
            onChange={(v) => set("reportedUnits", v)}
          />
          <Select
            label="Link origin"
            value={binding.origin}
            onChange={(v) => set("origin", v)}
            options={[
              { value: "manual", label: "Manual" },
              { value: "proposed", label: "Proposed" },
            ]}
          />
          <label className="flex gap-2 text-xs">
            <input
              type="checkbox"
              checked={binding.confirmed}
              onChange={(e) => set("confirmed", e.target.checked)}
            />
            I confirm this quantity belongs to this exact, unambiguous printed
            occurrence
          </label>
          <button
            className={button}
            disabled={busy || !binding.anchorId || !binding.result.resultId}
            onClick={() =>
              void run(async () => {
                await studioClient.mutate(workspaceId, {
                  action: "bindNumber",
                  id: selected || null,
                  expectedRevision: current?.record.revision ?? 0,
                  binding,
                });
                setCoverage(await studioClient.coverage(workspaceId));
                await onRefresh();
              })
            }
          >
            Save numeric binding
          </button>
        </section>
        <section className={panel}>
          <h2 className="font-semibold">Evidence and dependencies</h2>
          {current ? (
            <>
              <p className="text-sm">
                {current.state} · deterministic numeric check:{" "}
                {current.numericPassed === null
                  ? "unavailable"
                  : current.numericPassed
                    ? "passes"
                    : "disagrees"}
              </p>
              <p className="text-xs">{current.reasons.join("; ")}</p>
              <Inspect
                label="Exact result, specification IDs and artifact locator"
                value={current.result}
              />
              <Inspect
                label="Execution receipt and captured dependencies"
                value={data.executions.find(
                  (e) => e.id === current.record.body.result.executionId,
                )}
              />
              <Inspect
                label="Changed or unavailable dependencies"
                value={current.dependency}
              />
            </>
          ) : (
            <p className="text-sm text-gray-500">
              Choose a coverage item to trace its result, execution and declared
              dependencies.
            </p>
          )}
          <h3 className="pt-4 text-sm font-semibold">
            Generate a reviewed TeX value macro
          </h3>
          <p className="text-xs">
            Uses the selected result’s estimate. The file is staged in a task
            copy for review in Changes.
          </p>
          <Select
            label="Task change set"
            value={task}
            onChange={setTask}
            options={data.changes
              .filter((c) => ["isolated", "review"].includes(c.body.state))
              .map((c) => ({
                value: c.id,
                label:
                  data.tasks.find((t) => t.id === c.body.taskId)?.body
                    .objective ?? c.id,
              }))}
          />
          <Text label="TeX macro file" value={file} onChange={setFile} />
          <Text
            label="Macro name (letters only)"
            value={macro}
            onChange={setMacro}
          />
          <button
            className={button}
            disabled={!task || !binding.result.resultId || busy}
            onClick={() =>
              void run(async () => {
                await studioClient.mutate(workspaceId, {
                  action: "generateValues",
                  checkpointId: task,
                  path: file,
                  values: [
                    {
                      name: macro,
                      result: binding.result,
                      precision: binding.precision,
                    },
                  ],
                });
                setNotice(
                  "Macro staged. Review and accept its captured task in Changes.",
                );
                await onRefresh();
              })
            }
          >
            Stage value macro
          </button>
          <p role="status" className="text-xs">
            {notice}
          </p>
        </section>
      </div>
    </div>
  );
}

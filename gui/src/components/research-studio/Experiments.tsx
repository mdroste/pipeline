import { useEffect, useState } from "react";
import {
  studioClient,
  resultRef,
  type Experiment,
  type Comparison,
  type UnitConversion,
  type Specification,
  type SeriesRecord,
} from "../../lib/studioClient";
import { workbenchClient } from "../../lib/workbenchClient";
import type {
  ExecutionProfile,
  ResearchResultV1,
} from "../../lib/workbenchTypes";
import type { ProjectRecord } from "../../lib/projectClient";
import {
  button,
  input,
  panel,
  Text,
  Select,
  Field,
  ErrorNotice,
  Inspect,
  lines,
  useAction,
  type StudioProps,
} from "./shared";
import { JobLauncher } from "./Jobs";
const key = (r: ResearchResultV1) => `${r.sourceExecutionId}/${r.resultId}`;
export default function Experiments({
  workspaceId,
  data,
  onRefresh,
}: StudioProps) {
  const [results, setResults] = useState<ResearchResultV1[]>([]);
  const [profiles, setProfiles] = useState<ExecutionProfile[]>([]);
  const [experiments, setExperiments] = useState<ProjectRecord<Experiment>[]>(
    [],
  );
  const [specs, setSpecs] = useState<ProjectRecord<Specification>[]>([]);
  const [series, setSeries] = useState<ProjectRecord<SeriesRecord>[]>([]);
  const [root, setRoot] = useState("");
  const { error, busy, run, setError } = useAction();
  const [executionId, setExecutionId] = useState("");
  const [artifactId, setArtifactId] = useState("");
  const [left, setLeft] = useState("");
  const [right, setRight] = useState("");
  const [rationale, setRationale] = useState("");
  const [relative, setRelative] = useState(false);
  const [convert, setConvert] = useState(false);
  const [factor, setFactor] = useState(1);
  const [offset, setOffset] = useState(0);
  const [comparison, setComparison] = useState<Comparison | null>(null);
  const [experimentId, setExperimentId] = useState("");
  const [experiment, setExperiment] = useState<Experiment>({
    question: "",
    baselineExecutionId: "",
    intendedChange: "",
    executionIds: [],
    interpretation: "",
  });
  const [profileId, setProfileId] = useState("");
  const [profileName, setProfileName] = useState("");
  const [language, setLanguage] = useState("python3");
  const [script, setScript] = useState("");
  const [cwd, setCwd] = useState("");
  const [inputs, setInputs] = useState("");
  const [outputs, setOutputs] = useState("");
  const [timeout, setTimeout] = useState(120);
  const [argv, setArgv] = useState("");
  const [specId, setSpecId] = useState("");
  const [sampleId, setSampleId] = useState("");
  const [spec, setSpec] = useState("{}");
  const [specRecord, setSpecRecord] = useState("");
  const [seriesLeft, setSeriesLeft] = useState("");
  const [seriesRight, setSeriesRight] = useState("");
  const [seriesComparison, setSeriesComparison] = useState<{
    comparable: boolean;
    blockers: string[];
    horizons: number[];
    changes: (number | null)[];
  } | null>(null);
  const refresh = async () => {
    const [r, p, e, s, irf] = await Promise.all([
      workbenchClient.listStructuredResults(workspaceId),
      workbenchClient.listExecutionProfiles(workspaceId),
      studioClient.records<Experiment>(workspaceId, "experiment"),
      studioClient.records<Specification>(workspaceId, "specification"),
      studioClient.records<SeriesRecord>(workspaceId, "series"),
    ]);
    setResults(r);
    setProfiles(p);
    setExperiments(e);
    setSpecs(s);
    setSeries(irf);
  };
  useEffect(() => {
    let disposed = false;
    void Promise.all([
      workbenchClient.listStructuredResults(workspaceId),
      workbenchClient.listExecutionProfiles(workspaceId),
      studioClient.records<Experiment>(workspaceId, "experiment"),
      studioClient.records<Specification>(workspaceId, "specification"),
      studioClient.records<SeriesRecord>(workspaceId, "series"),
      workbenchClient.getWorkspace(workspaceId),
    ])
      .then(([r, p, e, s, irf, w]) => {
        if (!disposed) {
          setResults(r);
          setProfiles(p);
          setExperiments(e);
          setSpecs(s);
          setSeries(irf);
          setRoot(w.root ?? "");
        }
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
    };
  }, [workspaceId, data.executions, setError]);
  const execution = data.executions.find((e) => e.id === executionId);
  const artifacts = (execution?.outputManifest.artifacts ?? []) as {
    artifactId: string;
    path: string;
  }[];
  const selectedProfile = profiles.find((p) => p.id === profileId);
  const l = results.find((r) => key(r) === left);
  const r = results.find((r) => key(r) === right);
  const number = (value: number | null | undefined) =>
    value == null
      ? "Unavailable"
      : value.toLocaleString(undefined, { maximumSignificantDigits: 10 });
  return (
    <div className="space-y-5">
      <ErrorNotice error={error} />
      <div className="grid gap-4 lg:grid-cols-2">
        <section className={panel}>
          <h2 className="font-semibold">Local execution profiles</h2>
          <Select
            label="Configured profile"
            value={profileId}
            onChange={(id) => {
              setProfileId(id);
              const p = profiles.find((p) => p.id === id);
              if (p) {
                setProfileName(p.name);
                setCwd(p.cwd);
                setInputs(p.inputs.join("\n"));
                setOutputs(p.outputs.join("\n"));
                setTimeout(p.timeoutSeconds);
                setArgv(JSON.stringify(p.argv, null, 2));
                setLanguage(p.adapter === "stata" ? "oldstata" : "python3");
              } else {
                setArgv("");
                setCwd(root);
              }
            }}
            options={profiles.map((p) => ({ value: p.id, label: p.name }))}
            placeholder="New profile"
          />
          <Text
            label="Profile name"
            value={profileName}
            onChange={setProfileName}
          />
          <Select
            label="Toolchain"
            value={language}
            onChange={setLanguage}
            options={["python3", "Rscript", "julia", "oldstata"].map((v) => ({
              value: v,
              label: v,
            }))}
          />
          <Text
            label="Script (relative to working directory)"
            value={script}
            onChange={setScript}
          />
          <Text
            label="Working directory (absolute)"
            value={cwd || root}
            onChange={setCwd}
          />
          <Text
            label="Declared inputs (one per line)"
            multiline
            value={inputs}
            onChange={setInputs}
          />
          <Text
            label="Expected outputs (one per line)"
            multiline
            value={outputs}
            onChange={setOutputs}
          />
          <Field label="Timeout (seconds)">
            <input
              type="number"
              min={1}
              max={7200}
              className={input}
              value={timeout}
              onChange={(e) => setTimeout(Number(e.target.value))}
            />
          </Field>
          <details>
            <summary className="text-xs">Advanced argv</summary>
            <Text
              label="Explicit command argument array"
              multiline
              value={argv}
              onChange={setArgv}
            />
          </details>
          <p className="text-xs text-gray-500">
            Stata uses /bin/zsh -lic and oldstata. Include its log among
            expected outputs. Export structured JSON for repeatable result
            comparisons.
          </p>
          <button
            className={button}
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const directory = cwd || root;
                const quote = (s: string) => `'${s.replaceAll("'", "'\\''")}'`;
                const command = argv.trim()
                  ? JSON.parse(argv)
                  : language === "oldstata"
                    ? [
                        "/bin/zsh",
                        "-lic",
                        `oldstata -q -b do ${quote(`${directory}/${script}`)}`,
                      ]
                    : [language, script];
                const p = await workbenchClient.saveExecutionProfile({
                  profileId: profileId || null,
                  workspaceId,
                  name: profileName,
                  adapter: language === "oldstata" ? "stata" : "command",
                  argv: command,
                  cwd: directory,
                  environment: {},
                  inputs: [
                    ...new Set([...lines(inputs), ...(script ? [script] : [])]),
                  ],
                  outputs: lines(outputs),
                  timeoutSeconds: timeout,
                  expectedRevision: selectedProfile?.revision ?? null,
                  operationId: `profile-${crypto.randomUUID()}`,
                });
                setProfileId(p.id);
                await refresh();
              })
            }
          >
            Save local profile
          </button>
          {selectedProfile && <JobLauncher profile={selectedProfile} />}
        </section>
        <section className={panel}>
          <h2 className="font-semibold">Experiment notebook</h2>
          <Select
            label="Experiment"
            value={experimentId}
            onChange={(id) => {
              setExperimentId(id);
              setExperiment(
                experiments.find((e) => e.id === id)?.body ?? {
                  question: "",
                  baselineExecutionId: "",
                  intendedChange: "",
                  executionIds: [],
                  interpretation: "",
                },
              );
            }}
            options={experiments.map((e) => ({
              value: e.id,
              label: e.body.question,
            }))}
            placeholder="New experiment"
          />
          <Text
            label="Research question"
            value={experiment.question}
            onChange={(question) => setExperiment((e) => ({ ...e, question }))}
          />
          <Select
            label="Accepted baseline for this experiment"
            value={experiment.baselineExecutionId}
            onChange={(baselineExecutionId) =>
              setExperiment((e) => ({ ...e, baselineExecutionId }))
            }
            options={data.executions
              .filter((e) => e.outcome === "completed")
              .map((e) => ({
                value: e.id,
                label: e.command.join(" ") + " · " + e.id.slice(-8),
              }))}
          />
          <Text
            label="Intended change"
            multiline
            value={experiment.intendedChange}
            onChange={(intendedChange) =>
              setExperiment((e) => ({ ...e, intendedChange }))
            }
          />
          <Field label="Associated executions">
            <div className="max-h-32 overflow-auto">
              {data.executions.map((e) => (
                <label className="flex gap-2" key={e.id}>
                  <input
                    type="checkbox"
                    checked={experiment.executionIds.includes(e.id)}
                    onChange={() =>
                      setExperiment((v) => ({
                        ...v,
                        executionIds: v.executionIds.includes(e.id)
                          ? v.executionIds.filter((id) => id !== e.id)
                          : [...v.executionIds, e.id],
                      }))
                    }
                  />
                  {e.outcome} · {e.command.join(" ")}
                </label>
              ))}
            </div>
          </Field>
          <Text
            label="Interpretation (separate from solver checks)"
            multiline
            value={experiment.interpretation}
            onChange={(interpretation) =>
              setExperiment((e) => ({ ...e, interpretation }))
            }
          />
          <button
            className={button}
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const e = await studioClient.mutate<ProjectRecord<Experiment>>(
                  workspaceId,
                  {
                    action: "saveExperiment",
                    id: experimentId || null,
                    expectedRevision:
                      experiments.find((e) => e.id === experimentId)
                        ?.revision ?? 0,
                    experiment,
                  },
                );
                setExperimentId(e.id);
                await refresh();
              })
            }
          >
            Save experiment
          </button>
        </section>
      </div>
      <section className={panel}>
        <h2 className="font-semibold">Immutable result registry</h2>
        <div className="grid gap-3 md:grid-cols-2">
          <Select
            label="Completed execution"
            value={executionId}
            onChange={(v) => {
              setExecutionId(v);
              setArtifactId("");
            }}
            options={data.executions
              .filter((e) => e.outcome === "completed")
              .map((e) => ({ value: e.id, label: e.id }))}
          />
          <Select
            label="Adopted result export"
            value={artifactId}
            onChange={setArtifactId}
            options={artifacts
              .filter((a) => a.path.endsWith(".json"))
              .map((a) => ({ value: a.artifactId, label: a.path }))}
          />
        </div>
        <button
          className={button}
          disabled={!artifactId || busy}
          onClick={() =>
            void run(async () => {
              await studioClient.mutate(workspaceId, {
                action: "importResults",
                executionId,
                artifactId,
              });
              await refresh();
              await onRefresh();
            })
          }
        >
          Import research-results-v1 / v2
        </button>
        <p className="text-xs text-gray-500">
          Pipeline binds values to this execution and the adopted output. Logs
          are not parsed into authoritative coefficients.
        </p>
        <div className="overflow-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr>
                {[
                  "Quantity",
                  "Estimate",
                  "SE / interval",
                  "N",
                  "Units",
                  "Specification / sample",
                  "Execution",
                ].map((h) => (
                  <th key={h} className="p-2">
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {results.map((r) => (
                <tr className="border-t" key={key(r)}>
                  <td className="p-2">{r.estimand}</td>
                  <td>{number(r.estimate)}</td>
                  <td>
                    {number(r.standardError)} /{" "}
                    {r.confidenceInterval?.map(number).join(" to ") ??
                      "Unavailable"}
                  </td>
                  <td>{number(r.n)}</td>
                  <td>{r.units}</td>
                  <td>
                    {r.specificationId} / {r.sampleId}
                  </td>
                  <td>
                    <Inspect
                      label={r.sourceExecutionId.slice(-8)}
                      value={{
                        result: r,
                        execution: data.executions.find(
                          (e) => e.id === r.sourceExecutionId,
                        ),
                      }}
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
      <section className={panel}>
        <h2 className="font-semibold">Compare results</h2>
        <div className="grid gap-3 md:grid-cols-2">
          <Select
            label="Baseline result"
            value={left}
            onChange={(v) => {
              setLeft(v);
              setComparison(null);
            }}
            options={results.map((r) => ({
              value: key(r),
              label: `${r.resultId} · ${r.sourceExecutionId.slice(-8)}`,
            }))}
          />
          <Select
            label="Alternative result"
            value={right}
            onChange={(v) => {
              setRight(v);
              setComparison(null);
            }}
            options={results.map((r) => ({
              value: key(r),
              label: `${r.resultId} · ${r.sourceExecutionId.slice(-8)}`,
            }))}
          />
        </div>
        <Text
          label="Comparison rationale"
          multiline
          value={rationale}
          onChange={setRationale}
        />
        <label className="flex gap-2 text-xs">
          <input
            type="checkbox"
            checked={relative}
            onChange={(e) => setRelative(e.target.checked)}
          />
          Relative changes have an economically meaningful denominator for this
          quantity
        </label>
        <label className="flex gap-2 text-xs">
          <input
            type="checkbox"
            checked={convert}
            onChange={(e) => setConvert(e.target.checked)}
          />
          Record a conversion from alternative units to baseline units
        </label>
        {convert && (
          <div className="grid gap-3 md:grid-cols-2">
            <Field label={`Multiply ${r?.units ?? "alternative"} by`}>
              <input
                className={input}
                type="number"
                step="any"
                value={factor}
                onChange={(e) => setFactor(Number(e.target.value))}
              />
            </Field>
            <Field label={`Then add (in ${l?.units ?? "baseline units"})`}>
              <input
                className={input}
                type="number"
                step="any"
                value={offset}
                onChange={(e) => setOffset(Number(e.target.value))}
              />
            </Field>
          </div>
        )}
        <button
          className={button}
          disabled={!l || !r || busy}
          onClick={() =>
            void run(async () => {
              const conversion: UnitConversion | null = convert
                ? {
                    fromUnits: r!.units,
                    toUnits: l!.units,
                    factor,
                    offset,
                    rationale,
                  }
                : null;
              setComparison(
                await studioClient.compare(
                  workspaceId,
                  resultRef(l!),
                  resultRef(r!),
                  rationale,
                  conversion,
                  relative,
                ),
              );
            })
          }
        >
          Compare declared quantities
        </button>
        {comparison && (
          <>
            <p className="text-sm">
              {comparison.comparable
                ? "Quantities are comparable"
                : "Comparison unavailable"}
            </p>
            {comparison.blockers.map((b) => (
              <p className="text-xs text-amber-700" key={b}>
                {b}
              </p>
            ))}
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
              {[
                ["Signed change", comparison.signedChange],
                ["Absolute change", comparison.absoluteChange],
                ["Relative change", comparison.relativeChange],
                ["Sample size change", comparison.nChange],
              ].map(([label, value]) => (
                <p className="text-sm" key={String(label)}>
                  {label}: <strong>{number(value as number | null)}</strong>
                </p>
              ))}
            </div>
            <div className="grid gap-3 md:grid-cols-2">
              <Inspect
                label="Baseline specification, sample and uncertainty"
                value={{
                  result: comparison.left,
                  specification:
                    comparison.leftSpecification?.body ??
                    "Metadata unavailable",
                }}
              />
              <Inspect
                label="Alternative specification, sample and uncertainty"
                value={{
                  result: comparison.rightConverted,
                  specification:
                    comparison.rightSpecification?.body ??
                    "Metadata unavailable",
                  conversion: comparison.conversion,
                }}
              />
            </div>
            <p className="text-xs text-gray-500">
              Numerical agreement, solver convergence and economic
              interpretation are separate assessments.
            </p>
          </>
        )}
      </section>
      <section className={panel}>
        <h2 className="font-semibold">
          Specification, sample and calibration metadata
        </h2>
        <Select
          label="Metadata record"
          value={specRecord}
          onChange={(id) => {
            setSpecRecord(id);
            const s = specs.find((r) => r.id === id);
            setSpecId(s?.body.specificationId ?? "");
            setSampleId(s?.body.sampleId ?? "");
            setSpec(JSON.stringify(s?.body.fields ?? {}, null, 2));
          }}
          options={specs.map((s) => ({
            value: s.id,
            label: `${s.body.specificationId} / ${s.body.sampleId}`,
          }))}
          placeholder="New metadata"
        />
        <div className="grid gap-3 md:grid-cols-2">
          <Text label="Specification ID" value={specId} onChange={setSpecId} />
          <Text label="Sample ID" value={sampleId} onChange={setSampleId} />
        </div>
        <p className="text-xs text-gray-500">
          Each field retains its value, declared/inferred origin and source.
          Typical fields: controls, weights, inference, calibration and solver
          convergence.
        </p>
        <Text
          label={
            'Fields JSON: {"controls":{"value":["age"],"origin":"declared","source":"analysis.py"}}'
          }
          multiline
          value={spec}
          onChange={setSpec}
        />
        <button
          className={button}
          disabled={busy}
          onClick={() =>
            void run(async () => {
              const saved = await studioClient.mutate<
                ProjectRecord<Specification>
              >(workspaceId, {
                action: "saveSpecification",
                id: specRecord || null,
                expectedRevision:
                  specs.find((s) => s.id === specRecord)?.revision ?? 0,
                specification: {
                  specificationId: specId,
                  sampleId,
                  fields: JSON.parse(spec),
                },
              });
              setSpecRecord(saved.id);
              await refresh();
            })
          }
        >
          Save metadata
        </button>
      </section>
      {series.length > 0 && (
        <section className={panel}>
          <h2 className="font-semibold">Impulse responses</h2>
          <div className="grid gap-3 md:grid-cols-2">
            <Select
              label="Baseline series"
              value={seriesLeft}
              onChange={setSeriesLeft}
              options={series.map((s) => ({
                value: s.id,
                label: `${s.body.series.variable} · ${s.body.executionId.slice(-8)}`,
              }))}
            />
            <Select
              label="Alternative series"
              value={seriesRight}
              onChange={setSeriesRight}
              options={series.map((s) => ({
                value: s.id,
                label: `${s.body.series.variable} · ${s.body.executionId.slice(-8)}`,
              }))}
            />
          </div>
          <button
            className={button}
            disabled={!seriesLeft || !seriesRight || busy}
            onClick={() =>
              void run(async () =>
                setSeriesComparison(
                  await studioClient.compareSeries(
                    workspaceId,
                    seriesLeft,
                    seriesRight,
                    rationale,
                  ),
                ),
              )
            }
          >
            Compare series using rationale above
          </button>
          {seriesComparison && (
            <>
              <p className="text-xs">
                {seriesComparison.blockers.join("; ") ||
                  "Variable, units, shock normalization and horizon axes match."}
              </p>
              <table className="text-xs">
                <thead>
                  <tr>
                    <th>Horizon</th>
                    <th>Signed change</th>
                  </tr>
                </thead>
                <tbody>
                  {seriesComparison.changes.map((v, i) => (
                    <tr key={i}>
                      <td className="p-2">{seriesComparison.horizons[i]}</td>
                      <td>{number(v)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          )}
        </section>
      )}
    </div>
  );
}

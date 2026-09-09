import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  deskClient,
  reference,
  type DeskRecord,
  type DataPolicy,
  type DatasetVersion,
  type SampleDefinition,
} from "../../lib/deskClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { addContextObject } from "../WorkspaceContextTray";
import { button, input, card, muted, type DeskProps } from "./shared";
export default function Data({ workspaceId, onOpen, onError }: DeskProps) {
  const [metadata, setMetadata] = useState("");
  const [preview, setPreview] = useState<Awaited<
    ReturnType<typeof deskClient.rows>
  > | null>(null);
  const [datasets, setDatasets] = useState<DeskRecord<DatasetVersion>[]>([]);
  const [samples, setSamples] = useState<DeskRecord<SampleDefinition>[]>([]);
  const [policy, setPolicy] = useState<DataPolicy | null>(null);
  const [busy, setBusy] = useState(false);
  const [title, setTitle] = useState("");
  const [source, setSource] = useState("");
  const [vintage, setVintage] = useState("");
  const [supersedes, setSupersedes] = useState("");
  const [selected, setSelected] = useState<string[]>([]);
  const [rules, setRules] = useState("");
  const [unit, setUnit] = useState("");
  const [weights, setWeights] = useState("");
  const [dateRange, setDateRange] = useState("");
  const [samplePrevious, setSamplePrevious] = useState("");
  const [series, setSeries] = useState("");
  const [key, setKey] = useState("");
  const refresh = useCallback(async () => {
    const [d, s, p] = await Promise.all([
      deskClient.records<DatasetVersion>(workspaceId, "dataset"),
      deskClient.records<SampleDefinition>(workspaceId, "sample"),
      deskClient.policy(workspaceId),
    ]);
    setDatasets(d);
    setSamples(s);
    setPolicy(p);
  }, [workspaceId]);
  useEffect(() => {
    void refresh().catch((e) => onError(workbenchErrorMessage(e)));
  }, [refresh, onError]);
  const run = async (fn: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    try {
      await fn();
      await refresh();
    } catch (e) {
      onError(workbenchErrorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const importData = () =>
    run(async () => {
      const path = await open({
        multiple: false,
        filters: [
          { name: "Delimited data (up to 32 MiB)", extensions: ["csv", "tsv"] },
        ],
      });
      if (typeof path !== "string") return;
      await deskClient.importDataset(
        workspaceId,
        title || path.split("/").pop() || "Dataset",
        path,
        {
          provider: "local import",
          source: source || path,
          retrievedAt: new Date().toISOString(),
          requestedVintage: vintage || null,
          returnedVintage: null,
          seriesIds: [],
          units: null,
          frequency: null,
          transformation: null,
        },
        supersedes || null,
      );
    });
  return (
    <div className="space-y-5">
      <section className={card}>
        <h2 className="font-semibold">Dataset catalog</h2>
        <div className="grid gap-2">
          <input
            aria-label="Dataset title"
            className={input}
            placeholder="Dataset title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
          <input
            aria-label="Dataset source"
            className={input}
            placeholder="Source and acquisition method"
            value={source}
            onChange={(e) => setSource(e.target.value)}
          />
          <input
            aria-label="Data vintage"
            className={input}
            placeholder="Vintage (YYYY-MM-DD for FRED)"
            value={vintage}
            onChange={(e) => setVintage(e.target.value)}
          />
          <select
            aria-label="Dataset version relationship"
            className={input}
            value={supersedes}
            onChange={(e) => setSupersedes(e.target.value)}
          >
            <option value="">Independent dataset version</option>
            {datasets.map((d) => (
              <option key={d.id} value={d.id}>
                Supersedes {d.title} · {d.contentHash.slice(0, 8)}
              </option>
            ))}
          </select>
        </div>
        <button
          className={button}
          disabled={busy}
          onClick={() => void importData()}
        >
          Capture CSV / TSV
        </button>
        <p className={muted}>
          Save a copy of the data with a row count and detected variable types.
          Missing values are preserved. For large files or other formats, import
          metadata from an exporter.
        </p>
      </section>
      <details className={card}>
        <summary className="font-semibold">Project sharing policy</summary>
        <p className={muted}>
          Choose what Pipeline includes in search, conversations, and shared
          packages. These settings do not restrict local commands, which run
          with your computer’s file permissions.
        </p>
        {policy &&
          (
            ["dictionary", "summaries", "assistantRows", "packageData"] as const
          ).map((k) => (
            <label key={k} className="flex gap-2 text-sm">
              <input
                type="checkbox"
                disabled={busy}
                checked={policy[k]}
                onChange={(e) =>
                  void run(async () =>
                    setPolicy(
                      await deskClient.policy(workspaceId, {
                        ...policy,
                        [k]: e.target.checked,
                      }),
                    ),
                  )
                }
              />
              {
                {
                  dictionary: "Dictionary in search and context",
                  summaries: "Derived diagnostics in search and context",
                  assistantRows: "Allow the assistant to preview rows",
                  packageData: "Allow raw data in sharing packages",
                }[k]
              }
            </label>
          ))}
      </details>
      <details className={card}>
        <summary className="font-semibold">
          Import an exporter dictionary
        </summary>
        <p className={muted}>
          Paste DatasetVersion JSON (schemaVersion 1) with a SHA-256 hash and an
          externalReference to the saved data version. This imports metadata
          without reading the data rows.
        </p>
        <textarea
          aria-label="Dataset metadata JSON"
          rows={6}
          className={input}
          value={metadata}
          onChange={(e) => setMetadata(e.target.value)}
          placeholder="DatasetVersion JSON"
        />
        <button
          className={button}
          disabled={busy || !metadata.trim() || !title.trim()}
          onClick={() =>
            void run(() =>
              deskClient.importMetadata(
                workspaceId,
                title,
                JSON.parse(metadata) as DatasetVersion,
              ),
            )
          }
        >
          Import declared metadata
        </button>
      </details>
      {preview && (
        <section className={card}>
          <p className={muted}>
            {preview.coverage} Total rows: {preview.totalRows}
          </p>
          <div className="max-h-80 overflow-auto">
            <table className="text-left text-xs">
              <thead>
                <tr>
                  {preview.columns.map((c) => (
                    <th key={c} className="px-2">
                      {c}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {preview.rows.map((r, i) => (
                  <tr key={i}>
                    {r.map((v, j) => (
                      <td key={j} className="px-2">
                        {v ?? "Missing"}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <button className={button} onClick={() => setPreview(null)}>
            Close row preview
          </button>
        </section>
      )}
      <details className={card}>
        <summary className="font-semibold">FRED / ALFRED vintage</summary>
        <input
          aria-label="FRED series ID"
          className={input}
          placeholder="Series ID, e.g. GDP"
          value={series}
          onChange={(e) => setSeries(e.target.value)}
        />
        <input
          aria-label="FRED API key"
          type="password"
          autoComplete="off"
          className={input}
          placeholder="API key for this request only"
          value={key}
          onChange={(e) => setKey(e.target.value)}
        />
        <p className={muted}>
          Uses the exact vintage entered above and original units (lin). Enable
          project acquisition in Library first. The key is not saved.
        </p>
        <button
          className={button}
          disabled={busy || !series || !vintage || !key}
          onClick={() => {
            const apiKey = key;
            setKey("");
            void run(async () => {
              const receipt = await deskClient.fred(
                workspaceId,
                series,
                vintage,
                apiKey,
              );
              if (receipt.kind === "acquisition")
                onError(String(receipt.body.error ?? "Acquisition failed"));
            });
          }}
        >
          Capture series vintage
        </button>
      </details>
      {datasets.map((d) => (
        <article key={d.id} className={card}>
          <div className="flex gap-2">
            <input
              aria-label={`Use ${d.title} in sample`}
              type="checkbox"
              checked={selected.includes(d.id)}
              onChange={(e) =>
                setSelected((old) =>
                  e.target.checked
                    ? [...old, d.id]
                    : old.filter((id) => id !== d.id),
                )
              }
            />
            <button
              className="font-semibold underline"
              onClick={() => onOpen(reference(d))}
            >
              {d.title}
            </button>
          </div>
          <p className={muted}>
            {d.body.rows.toLocaleString()} rows · {d.body.columns.length}{" "}
            columns · {d.body.diagnosticsCoverage} ·{" "}
            {d.body.acquisition.requestedVintage ?? "No vintage declared"}
          </p>
          <div className="max-h-64 overflow-auto">
            <table className="w-full text-left text-xs">
              <thead>
                <tr>
                  <th>Variable</th>
                  <th>Inferred type</th>
                  <th>Missing</th>
                  <th>Units</th>
                </tr>
              </thead>
              <tbody>
                {d.body.columns.map((c) => (
                  <tr key={c.name}>
                    <td>{c.name}</td>
                    <td>{c.inferredType}</td>
                    <td>{c.missing}</td>
                    <td>{c.units ?? "Undeclared"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <button
            className={button}
            onClick={() => addContextObject(workspaceId, reference(d))}
          >
            Add dictionary to conversation
          </button>
          <button
            className={button}
            disabled={busy || !d.body.artifactId}
            onClick={() =>
              void run(async () =>
                setPreview(await deskClient.rows(workspaceId, d.id)),
              )
            }
          >
            Preview local rows
          </button>
        </article>
      ))}
      <section className={card}>
        <h2 className="font-semibold">Declare a sample</h2>
        <p className={muted}>
          Select exact datasets above. Rules are researcher declarations; this
          form does not claim to have executed them.
        </p>
        <textarea
          aria-label="Sample inclusion rules"
          className={input}
          value={rules}
          onChange={(e) => setRules(e.target.value)}
          placeholder="Inclusion and exclusion rules; one filter per line"
        />
        <input
          aria-label="Unit of observation"
          className={input}
          value={unit}
          onChange={(e) => setUnit(e.target.value)}
          placeholder="Unit of observation"
        />
        <input
          aria-label="Sample weights"
          className={input}
          value={weights}
          onChange={(e) => setWeights(e.target.value)}
          placeholder="Optional weights"
        />
        <input
          aria-label="Sample dates"
          className={input}
          value={dateRange}
          onChange={(e) => setDateRange(e.target.value)}
          placeholder="Optional date range"
        />
        <select
          aria-label="Sample supersession"
          className={input}
          value={samplePrevious}
          onChange={(e) => setSamplePrevious(e.target.value)}
        >
          <option value="">New sample</option>
          {samples.map((s) => (
            <option key={s.id} value={s.id}>
              Revise {s.title}
            </option>
          ))}
        </select>
        <button
          className={button}
          disabled={busy || !rules.trim() || !unit.trim() || !selected.length}
          onClick={() =>
            void run(() =>
              deskClient.sample(
                workspaceId,
                `Sample: ${unit}`,
                {
                  datasets: datasets
                    .filter((d) => selected.includes(d.id))
                    .map(reference),
                  inclusionRules: rules,
                  filters: rules.split("\n").filter(Boolean),
                  weights: weights || null,
                  dateRange: dateRange || null,
                  unitOfObservation: unit,
                  membershipHash: null,
                  origin: "declared",
                },
                samplePrevious || null,
              ),
            )
          }
        >
          Save new sample identity
        </button>
        {samples.map((s) => (
          <button
            key={s.id}
            className="block text-sm underline"
            onClick={() => onOpen(reference(s))}
          >
            {s.title} · {s.contentHash.slice(0, 12)}
          </button>
        ))}
      </section>
    </div>
  );
}

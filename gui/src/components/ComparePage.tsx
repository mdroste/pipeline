import { useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { lineDiff, type DiffOp } from "../lib/diff";
import type { PipelineReport, StepOutput } from "../lib/types";
import type { RunManifest } from "./ArtifactExplorer";
import SafeMarkdownLink from "./SafeMarkdownLink";

interface Props {
  runA: string;
  runB: string;
  onBack: () => void;
}

function baseId(id: string): string {
  return id.split("/")[0];
}

/** Map each report's outputs by base step id (joining multi-agent outputs). */
function byStep(report: PipelineReport | null): Map<string, StepOutput> {
  const map = new Map<string, StepOutput>();
  for (const o of report?.step_outputs ?? []) {
    const key = baseId(o.step_id);
    const prev = map.get(key);
    if (prev) prev.raw_text += "\n\n" + o.raw_text;
    else map.set(key, { ...o, step_id: key });
  }
  return map;
}

function DiffBlock({ ops }: { ops: DiffOp[] }) {
  return (
    <pre className="text-xs font-mono whitespace-pre-wrap leading-relaxed overflow-x-auto">
      {ops.map((op, i) => (
        <span
          key={i}
          className={`block ${
            op.type === "add"
              ? "bg-green-50 dark:bg-green-950 text-green-800 dark:text-green-300"
              : op.type === "del"
                ? "bg-red-50 dark:bg-red-950 text-red-800 dark:text-red-300"
                : "text-gray-600 dark:text-gray-400"
          }`}
        >
          <span className="select-none opacity-50 mr-2">
            {op.type === "add" ? "+" : op.type === "del" ? "−" : " "}
          </span>
          {op.text || " "}
        </span>
      ))}
    </pre>
  );
}

function LazyDiffBlock({ before, after }: { before: string; after: string }) {
  const ops = useMemo(() => lineDiff(before, after), [before, after]);
  return <DiffBlock ops={ops} />;
}

export default function ComparePage({ runA, runB, onBack }: Props) {
  const [reportA, setReportA] = useState<PipelineReport | null>(null);
  const [reportB, setReportB] = useState<PipelineReport | null>(null);
  const [manifestA, setManifestA] = useState<RunManifest | null>(null);
  const [manifestB, setManifestB] = useState<RunManifest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [reconcile, setReconcile] = useState<string | null>(null);
  const [reconciling, setReconciling] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  useEffect(() => {
    setLoading(true);
    Promise.all([
      invoke<PipelineReport>("get_run_report", { runId: runA }),
      invoke<PipelineReport>("get_run_report", { runId: runB }),
      invoke<RunManifest>("get_run_manifest", { runId: runA }),
      invoke<RunManifest>("get_run_manifest", { runId: runB }),
    ])
      .then(([a, b, aManifest, bManifest]) => {
        setReportA(a);
        setReportB(b);
        setManifestA(aManifest);
        setManifestB(bManifest);
        setError(null);
      })
      .catch((e) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setLoading(false));
  }, [runA, runB]);

  const mapA = useMemo(() => byStep(reportA), [reportA]);
  const mapB = useMemo(() => byStep(reportB), [reportB]);

  // Union of step ids, preserving each report's order.
  const stepKeys = useMemo(() => {
    const keys: string[] = [];
    const seen = new Set<string>();
    for (const o of reportA?.step_outputs ?? []) {
      const k = baseId(o.step_id);
      if (!seen.has(k)) {
        seen.add(k);
        keys.push(k);
      }
    }
    for (const o of reportB?.step_outputs ?? []) {
      const k = baseId(o.step_id);
      if (!seen.has(k)) {
        seen.add(k);
        keys.push(k);
      }
    }
    return keys;
  }, [reportA, reportB]);

  const comparisonSummary = useMemo(() => {
    let changed = 0;
    let added = 0;
    let removed = 0;
    let unchanged = 0;
    for (const key of stepKeys) {
      const older = mapA.get(key);
      const newer = mapB.get(key);
      if (!older) added++;
      else if (!newer) removed++;
      else if (older.raw_text !== newer.raw_text) changed++;
      else unchanged++;
    }
    return { changed, added, removed, unchanged };
  }, [mapA, mapB, stepKeys]);

  const exportDiff = async () => {
    const blocks = stepKeys.map((key) => {
      const older = mapA.get(key);
      const newer = mapB.get(key);
      const label = newer?.step_label || older?.step_label || key;
      const lines = lineDiff(older?.raw_text ?? "", newer?.raw_text ?? "")
        .map(
          (operation) =>
            `${operation.type === "add" ? "+" : operation.type === "del" ? "-" : " "} ${operation.text}`,
        )
        .join("\n");
      return `## ${label}\n\n\`\`\`diff\n${lines}\n\`\`\``;
    });
    const content = [
      "# Pipeline report comparison",
      `Older run: ${runA}`,
      `Newer run: ${runB}`,
      `Summary: ${comparisonSummary.changed} changed, ${comparisonSummary.added} added, ${comparisonSummary.removed} removed, ${comparisonSummary.unchanged} unchanged.`,
      ...blocks,
    ].join("\n\n");
    try {
      await invoke("save_text_file", {
        content,
        suggestedName: `pipeline-diff-${runA.slice(0, 8)}-${runB.slice(0, 8)}.md`,
      });
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  const runReconcile = async () => {
    setReconciling(true);
    setError(null);
    try {
      const md = await invoke<string>("reconcile_runs", { runA, runB });
      setReconcile(md);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setReconciling(false);
    }
  };

  const toggle = (k: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(k) ? next.delete(k) : next.add(k);
      return next;
    });

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-3 px-6 py-2.5 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 shrink-0">
        <button
          onClick={onBack}
          className="text-sm text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100"
        >
          ← Back
        </button>
        <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
          Compare reports
        </span>
        <button
          onClick={() => void exportDiff()}
          disabled={loading}
          className="ml-auto px-3 py-1 text-xs rounded-lg border border-gray-300 dark:border-gray-700 disabled:opacity-40"
        >
          Export diff…
        </button>
        <button
          onClick={runReconcile}
          disabled={reconciling || loading}
          className="px-3 py-1 text-xs rounded-lg bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 hover:opacity-90 disabled:opacity-40"
          title="Ask an LLM which concerns were addressed, which remain, and what's new (costs one call)"
        >
          {reconciling ? "Reconciling…" : "Reconcile with LLM"}
        </button>
      </div>

      <div className="flex-1 overflow-auto p-4 space-y-3">
        {error && (
          <div className="p-3 bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800 rounded text-sm text-red-700 dark:text-red-400">
            {error}
          </div>
        )}
        {loading ? (
          <p className="text-sm text-gray-500 dark:text-gray-400 px-2">
            Loading both reports…
          </p>
        ) : (
          <>
            <div className="grid gap-3 sm:grid-cols-2">
              {(
                [
                  [
                    "Older report",
                    runA,
                    manifestA,
                    "border-red-200 dark:border-red-900",
                  ],
                  [
                    "Newer report",
                    runB,
                    manifestB,
                    "border-green-200 dark:border-green-900",
                  ],
                ] as const
              ).map(([label, id, manifest, tone]) => (
                <section
                  key={id}
                  className={`rounded-lg border bg-white p-3 dark:bg-gray-900 ${tone}`}
                >
                  <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">
                    {label}
                  </h3>
                  <p className="mt-1 text-sm text-gray-800 dark:text-gray-200">
                    {manifest?.title ||
                      manifest?.profile_name ||
                      "Untitled report"}
                  </p>
                  <p className="mt-1 break-all font-mono text-[11px] text-gray-500 dark:text-gray-400">
                    {id}
                  </p>
                  <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
                    <dt className="text-gray-500">Created</dt>
                    <dd>
                      {manifest?.created
                        ? new Date(manifest.created).toLocaleString()
                        : "Unknown"}
                    </dd>
                    <dt className="text-gray-500">Provider</dt>
                    <dd>{manifest?.provider || "Unknown"}</dd>
                    <dt className="text-gray-500">Workflow</dt>
                    <dd>{manifest?.profile_name || "Unknown"}</dd>
                    <dt className="text-gray-500">Artifacts</dt>
                    <dd>{manifest?.artifacts.length ?? 0}</dd>
                  </dl>
                </section>
              ))}
            </div>
            <div className="rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-xs text-gray-600 dark:border-gray-800 dark:bg-gray-900/60 dark:text-gray-300">
              <span className="font-medium">Summary:</span>{" "}
              {comparisonSummary.changed} changed · {comparisonSummary.added}{" "}
              added · {comparisonSummary.removed} removed ·{" "}
              {comparisonSummary.unchanged} unchanged.
              <span className="ml-2 text-gray-500 dark:text-gray-400">
                Diffs show{" "}
                <span className="text-red-700 dark:text-red-400">− older</span>{" "}
                and{" "}
                <span className="text-green-700 dark:text-green-400">
                  + newer
                </span>{" "}
                lines. Reconciliation sends both reports and their disclosed
                provider-produced artifacts to the selected model for one
                additional call.
              </span>
            </div>

            {reconcile && (
              <div className="border border-gray-200 dark:border-gray-800 rounded-lg p-4 bg-white dark:bg-gray-900">
                <div className="report-content !max-w-none !p-0">
                  <ReactMarkdown
                    remarkPlugins={[remarkGfm]}
                    components={{ a: SafeMarkdownLink }}
                  >
                    {reconcile}
                  </ReactMarkdown>
                </div>
              </div>
            )}

            {stepKeys.map((k) => {
              const a = mapA.get(k);
              const b = mapB.get(k);
              const label = b?.step_label || a?.step_label || k;
              const before = a?.raw_text ?? "";
              const after = b?.raw_text ?? "";
              const changed = before !== after;
              const isOpen = expanded.has(k);
              return (
                <div
                  key={k}
                  className="border border-gray-200 dark:border-gray-800 rounded-lg bg-white dark:bg-gray-900"
                >
                  <button
                    onClick={() => toggle(k)}
                    className="w-full flex items-center gap-2 px-3 py-2 text-left"
                  >
                    <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
                      {label}
                    </span>
                    {!a && (
                      <span className="text-[10px] text-green-700 dark:text-green-400">
                        only in newer
                      </span>
                    )}
                    {!b && (
                      <span className="text-[10px] text-red-700 dark:text-red-400">
                        only in older
                      </span>
                    )}
                    <span
                      className={`ml-auto text-xs ${changed ? "text-amber-700 dark:text-amber-300" : "text-gray-500 dark:text-gray-400"}`}
                    >
                      {changed ? "changed" : "unchanged"}
                    </span>
                    <span className="text-gray-500 dark:text-gray-400 text-xs">
                      {isOpen ? "▾" : "▸"}
                    </span>
                  </button>
                  {isOpen && (
                    <div className="px-3 pb-3 border-t border-gray-100 dark:border-gray-800 pt-2">
                      <LazyDiffBlock before={before} after={after} />
                    </div>
                  )}
                </div>
              );
            })}
          </>
        )}
      </div>
    </div>
  );
}

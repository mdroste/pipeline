import { useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { lineDiff, hasChanges, type DiffOp } from "../lib/diff";
import type { PipelineReport, StepOutput } from "../lib/types";

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
        <div
          key={i}
          className={
            op.type === "add"
              ? "bg-green-50 dark:bg-green-950 text-green-800 dark:text-green-300"
              : op.type === "del"
                ? "bg-red-50 dark:bg-red-950 text-red-800 dark:text-red-300"
                : "text-gray-600 dark:text-gray-400"
          }
        >
          <span className="select-none opacity-50 mr-2">
            {op.type === "add" ? "+" : op.type === "del" ? "−" : " "}
          </span>
          {op.text || " "}
        </div>
      ))}
    </pre>
  );
}

export default function ComparePage({ runA, runB, onBack }: Props) {
  const [reportA, setReportA] = useState<PipelineReport | null>(null);
  const [reportB, setReportB] = useState<PipelineReport | null>(null);
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
    ])
      .then(([a, b]) => {
        setReportA(a);
        setReportB(b);
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
      if (!seen.has(k)) { seen.add(k); keys.push(k); }
    }
    for (const o of reportB?.step_outputs ?? []) {
      const k = baseId(o.step_id);
      if (!seen.has(k)) { seen.add(k); keys.push(k); }
    }
    return keys;
  }, [reportA, reportB]);

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
        <button onClick={onBack} className="text-sm text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100">
          ← Back
        </button>
        <span className="text-sm font-medium text-gray-800 dark:text-gray-200">Compare runs</span>
        <button
          onClick={runReconcile}
          disabled={reconciling || loading}
          className="ml-auto px-3 py-1 text-xs rounded-lg bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 hover:opacity-90 disabled:opacity-40"
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
          <p className="text-sm text-gray-400 px-2">Loading both runs…</p>
        ) : (
          <>
            <div className="text-xs text-gray-400 px-1">
              <span className="text-red-500">− older run</span> vs <span className="text-green-600">+ newer run</span>{" "}
              (per step; deletions from the first run, additions in the second)
            </div>

            {reconcile && (
              <div className="border border-gray-200 dark:border-gray-800 rounded-lg p-4 bg-white dark:bg-gray-900">
                <div className="report-content !max-w-none !p-0">
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>{reconcile}</ReactMarkdown>
                </div>
              </div>
            )}

            {stepKeys.map((k) => {
              const a = mapA.get(k);
              const b = mapB.get(k);
              const label = b?.step_label || a?.step_label || k;
              const ops = lineDiff(a?.raw_text ?? "", b?.raw_text ?? "");
              const changed = hasChanges(ops);
              const isOpen = expanded.has(k);
              return (
                <div key={k} className="border border-gray-200 dark:border-gray-800 rounded-lg bg-white dark:bg-gray-900">
                  <button
                    onClick={() => toggle(k)}
                    className="w-full flex items-center gap-2 px-3 py-2 text-left"
                  >
                    <span className="text-sm font-medium text-gray-800 dark:text-gray-200">{label}</span>
                    {!a && <span className="text-[10px] text-green-600">only in newer</span>}
                    {!b && <span className="text-[10px] text-red-500">only in older</span>}
                    <span className={`ml-auto text-xs ${changed ? "text-amber-600 dark:text-amber-400" : "text-gray-400"}`}>
                      {changed ? "changed" : "unchanged"}
                    </span>
                    <span className="text-gray-400 text-xs">{isOpen ? "▾" : "▸"}</span>
                  </button>
                  {isOpen && (
                    <div className="px-3 pb-3 border-t border-gray-100 dark:border-gray-800 pt-2">
                      <DiffBlock ops={ops} />
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

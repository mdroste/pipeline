import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import ComparePage from "./ComparePage";
import ReportWorkspace from "./ReportWorkspace";
import type { RunSummary, RunsDiskUsage, ToolCallCounts } from "../lib/types";

interface Props {
  onClose: () => void;
  showClose?: boolean;
  /** When set, open this run's detail view immediately (e.g. from a batch job). */
  initialRunId?: string | null;
  /** Re-run a past run (reusing cached extraction/orientation and steps). */
  onRerun?: (runId: string, onlyFailed: boolean) => void;
}

function fmtBytes(n: number): string {
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(1) + " GB";
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + " MB";
  if (n >= 1_000) return (n / 1_000).toFixed(0) + " KB";
  return n + " B";
}

function fmtTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1).replace(/\.0$/, "") + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1).replace(/\.0$/, "") + "k";
  return String(n);
}

function freshInputTokens(run: RunSummary): number {
  return Math.max(
    0,
    run.input_tokens -
      (run.cached_input_tokens ?? 0) -
      (run.cache_write_input_tokens ?? 0),
  );
}

function runToolCalls(run: RunSummary): ToolCallCounts {
  return run.tool_calls ?? {
    text_file: 0,
    image: 0,
    web: 0,
    shell_or_other: 0,
    unknown: 0,
  };
}

function toolCallTotal(counts: ToolCallCounts): number {
  return (
    counts.text_file +
    counts.image +
    counts.web +
    counts.shell_or_other +
    counts.unknown
  );
}

function toolCallBreakdown(counts: ToolCallCounts): string {
  return [
    counts.text_file > 0 ? `${counts.text_file.toLocaleString()} text/file` : "",
    counts.image > 0 ? `${counts.image.toLocaleString()} image` : "",
    counts.web > 0 ? `${counts.web.toLocaleString()} web` : "",
    counts.shell_or_other > 0
      ? `${counts.shell_or_other.toLocaleString()} shell/other`
      : "",
    counts.unknown > 0 ? `${counts.unknown.toLocaleString()} unknown` : "",
  ]
    .filter(Boolean)
    .join(", ");
}

function reportedActivity(run: RunSummary): string {
  const counts = runToolCalls(run);
  const tools = toolCallTotal(counts);
  return [
    (run.model_round_trips ?? 0) > 0
      ? `${(run.model_round_trips ?? 0).toLocaleString()} reported model round trips`
      : "",
    tools > 0
      ? `${tools.toLocaleString()} reported tool calls (${toolCallBreakdown(counts)})`
      : "",
  ]
    .filter(Boolean)
    .join(" and ");
}

function hasReportedUsage(run: RunSummary): boolean {
  return (
    run.input_tokens + run.output_tokens > 0 ||
    (run.model_round_trips ?? 0) > 0 ||
    toolCallTotal(runToolCalls(run)) > 0
  );
}

function usageDescription(run: RunSummary): string {
  const cacheRead = run.cached_input_tokens ?? 0;
  const cacheWrite = run.cache_write_input_tokens ?? 0;
  const activity = reportedActivity(run);
  return [
    `Token usage: ${run.input_tokens.toLocaleString()} logical input tokens equals ${freshInputTokens(run).toLocaleString()} fresh input tokens plus ${cacheRead.toLocaleString()} cache-read tokens plus ${cacheWrite.toLocaleString()} cache-write tokens; ${run.output_tokens.toLocaleString()} output tokens.`,
    activity ? `Model activity: ${activity}.` : "",
    "Cache reads and cache writes are subsets of logical input, not additional tokens.",
    "Fresh input equals logical input minus cache reads minus cache writes.",
    "Model round trips and tool calls are shown only when the provider or CLI reports them; unknown tool kinds remain in the unknown bucket.",
    "The completed report's Run summary prices these categories separately for a labelled API-equivalent estimate when the model is recognized.",
    "Only providers that report usage are included.",
  ]
    .filter(Boolean)
    .join(" ");
}

function fmtDuration(secs: number): string {
  if (secs <= 0) return "—";
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

function fmtDate(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function StatusBadge({ status }: { status: string }) {
  const map: Record<string, string> = {
    done: "bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300",
    partial: "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300",
    failed: "bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300",
    interrupted: "bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300",
    cancelled: "bg-gray-200 text-gray-600 dark:bg-gray-700 dark:text-gray-300",
  };
  const cls = map[status] || map.done;
  return <span className={`px-1.5 py-0.5 rounded text-xs font-medium ${cls}`}>{status || "done"}</span>;
}

export default function HistoryPage({ onClose, showClose = true, initialRunId, onRerun }: Props) {
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [usage, setUsage] = useState<RunsDiskUsage | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [openRunId, setOpenRunId] = useState<string | null>(initialRunId ?? null);
  // Which run's metadata is being edited inline (title + tags).
  const [editing, setEditing] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [editTags, setEditTags] = useState("");
  // Compare mode: pick two runs to diff.
  const [compareMode, setCompareMode] = useState(false);
  const [compareIds, setCompareIds] = useState<string[]>([]);
  const [comparing, setComparing] = useState<[string, string] | null>(null);
  const refreshRequestRef = useRef(0);

  useEffect(
    () => () => {
      refreshRequestRef.current += 1;
    },
    [],
  );

  const toggleCompareId = (id: string) =>
    setCompareIds((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id].slice(-2)));

  const compareSelected = () => {
    if (compareIds.length !== 2) return;
    const [first, second] = compareIds;
    const firstCreated = Date.parse(runs.find((run) => run.run_id === first)?.created ?? "");
    const secondCreated = Date.parse(runs.find((run) => run.run_id === second)?.created ?? "");
    if (Number.isFinite(firstCreated) && Number.isFinite(secondCreated) && firstCreated > secondCreated) {
      setComparing([second, first]);
    } else {
      setComparing([first, second]);
    }
  };

  const refresh = useCallback(async () => {
    const request = ++refreshRequestRef.current;
    setLoading(true);
    try {
      const [nextRuns, nextUsage] = await Promise.all([
        invoke<RunSummary[]>("list_runs"),
        invoke<RunsDiskUsage>("runs_disk_usage").catch(() => null),
      ]);
      if (request !== refreshRequestRef.current) return;
      setRuns(nextRuns);
      setUsage(nextUsage);
      setError(null);
    } catch (e) {
      if (request === refreshRequestRef.current) {
        setError(e instanceof Error ? e.message : String(e));
      }
    } finally {
      if (request === refreshRequestRef.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const needle = filter.trim().toLowerCase();
  const visible = useMemo(() => {
    if (!needle) return runs;
    return runs.filter((r) =>
      [r.title, r.input_name, r.profile_name, r.provider, ...r.tags]
        .join(" ")
        .toLowerCase()
        .includes(needle)
    );
  }, [runs, needle]);

  const startEdit = (r: RunSummary) => {
    setEditing(r.run_id);
    setEditTitle(r.title);
    setEditTags(r.tags.join(", "));
  };

  const saveEdit = async (runId: string) => {
    const tags = editTags
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean);
    try {
      await invoke("update_run_meta", { runId, title: editTitle, tags });
      setEditing(null);
      void refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const deleteRun = async (r: RunSummary) => {
    const label = r.title || r.input_name;
    if (!window.confirm(`Delete run "${label}" and all its artifacts? This cannot be undone.`)) return;
    try {
      await invoke("delete_run", { runId: r.run_id });
      if (openRunId === r.run_id) setOpenRunId(null);
      void refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  // ── Compare view: diff two runs ──
  if (comparing) {
    return <ComparePage runA={comparing[0]} runB={comparing[1]} onBack={() => setComparing(null)} />;
  }

  // ── Detail view: browse one past run's artifacts ──
  if (openRunId) {
    const run = runs.find((r) => r.run_id === openRunId);
    return (
      <ReportWorkspace
        runId={openRunId}
        summary={run}
        onBack={() => setOpenRunId(null)}
      />
    );
  }

  // ── List view ──
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-3 px-6 py-3 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 shrink-0">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">Run history</h2>
        {usage && (
          <span className="text-xs text-gray-500 dark:text-gray-400">
            {usage.count} run{usage.count === 1 ? "" : "s"} · {fmtBytes(usage.bytes)} on disk
          </span>
        )}
        <input
          aria-label="Filter run history"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter by name, profile, tag…"
          className="ml-auto w-64 py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-400"
        />
        <button
          onClick={() => { setCompareMode((v) => !v); setCompareIds([]); }}
          className={`text-sm transition-colors ${compareMode ? "text-gray-900 dark:text-gray-100 font-medium" : "text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100"}`}
          title="Select two runs to compare"
        >
          Compare
        </button>
        <button
          onClick={refresh}
          className="text-sm text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
          title="Reload"
        >
          Refresh
        </button>
        {showClose && (
          <button
            onClick={onClose}
            aria-label="Close run history"
            className="text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100"
            title="Close"
          >
            ✕
          </button>
        )}
      </div>

      {compareMode && (
        <div className="flex items-center gap-3 px-6 py-2 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-800 text-sm shrink-0">
          <span className="text-gray-500 dark:text-gray-400">
            {compareIds.length === 0 ? "Select two runs to compare." : `${compareIds.length} of 2 selected`}
          </span>
          <button
            onClick={compareSelected}
            disabled={compareIds.length !== 2}
            className="ml-auto px-3 py-1 text-xs rounded-lg bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            Compare selected
          </button>
        </div>
      )}

      <div className="flex-1 overflow-auto p-4">
        {error && (
          <div className="mb-3 p-3 bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800 rounded text-sm text-red-700 dark:text-red-400">
            {error}
          </div>
        )}
        {loading ? (
          <p className="text-gray-500 dark:text-gray-400 text-sm px-2">Loading…</p>
        ) : visible.length === 0 ? (
          <p className="text-gray-500 dark:text-gray-400 text-sm px-2">
            {runs.length === 0 ? "No runs yet. Generate a report to see it here." : "No runs match the filter."}
          </p>
        ) : (
          <div className="space-y-2">
            {visible.map((r) => (
              <div
                key={r.run_id}
                className="border border-gray-200 dark:border-gray-800 rounded-lg bg-white dark:bg-gray-900 p-3 hover:border-gray-300 dark:hover:border-gray-700 transition-colors"
              >
                {editing === r.run_id ? (
                  <div className="space-y-2">
                    <input
                      aria-label={`Title for ${r.input_name}`}
                      value={editTitle}
                      onChange={(e) => setEditTitle(e.target.value)}
                      placeholder={r.input_name}
                      className="w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                      autoFocus
                    />
                    <input
                      aria-label={`Tags for ${r.title || r.input_name}`}
                      value={editTags}
                      onChange={(e) => setEditTags(e.target.value)}
                      placeholder="tags, comma separated"
                      className="w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                      onKeyDown={(e) => {
                        if (e.key === "Enter") saveEdit(r.run_id);
                        if (e.key === "Escape") setEditing(null);
                      }}
                    />
                    <div className="flex gap-2">
                      <button
                        onClick={() => saveEdit(r.run_id)}
                        className="px-2 py-1 text-xs rounded bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900 hover:opacity-90"
                      >
                        Save
                      </button>
                      <button
                        onClick={() => setEditing(null)}
                        className="px-2 py-1 text-xs rounded border border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-300"
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="flex items-start gap-3">
                    {compareMode && (
                      <input
                        type="checkbox"
                        checked={compareIds.includes(r.run_id)}
                        onChange={() => toggleCompareId(r.run_id)}
                        className="mt-1 shrink-0"
                        aria-label={`Select ${r.title || r.input_name} for comparison`}
                      />
                    )}
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 flex-wrap">
                        <button
                          onClick={() => setOpenRunId(r.run_id)}
                          className="text-sm font-medium text-gray-900 dark:text-gray-100 hover:underline truncate"
                          title="Open this run"
                        >
                          {r.title || r.input_name}
                        </button>
                        <StatusBadge status={r.status} />
                        {r.tags.map((t) => (
                          <span
                            key={t}
                            className="px-1.5 py-0.5 rounded text-xs bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400"
                          >
                            {t}
                          </span>
                        ))}
                      </div>
                      <div className="mt-1 text-xs text-gray-500 dark:text-gray-400 flex flex-wrap gap-x-3 gap-y-0.5">
                        <span>{fmtDate(r.created)}</span>
                        <span>{r.profile_name || r.profile_id}</span>
                        <span className="capitalize">{r.provider}</span>
                        <span>{r.step_count} step{r.step_count === 1 ? "" : "s"}</span>
                        <span>{fmtDuration(r.duration_secs)}</span>
                        {hasReportedUsage(r) && (
                          <span
                            aria-label={usageDescription(r)}
                            title={usageDescription(r)}
                            tabIndex={0}
                          >
                            {fmtTokens(r.input_tokens)} logical input ={" "}
                            <span className="text-gray-600 dark:text-gray-300">
                              {fmtTokens(freshInputTokens(r))} fresh
                            </span>
                            {(r.cached_input_tokens ?? 0) > 0 && (
                              <span className="text-green-700 dark:text-green-400">
                                {" "}+ {fmtTokens(r.cached_input_tokens)} cache read
                              </span>
                            )}
                            {(r.cache_write_input_tokens ?? 0) > 0 && (
                              <span className="text-blue-600 dark:text-blue-400">
                                {" "}+ {fmtTokens(r.cache_write_input_tokens)} cache write
                              </span>
                            )}
                            {" · "}
                            {fmtTokens(r.output_tokens)} output
                            {reportedActivity(r) && (
                              <span className="text-violet-600 dark:text-violet-400">
                                {" · "}
                                {reportedActivity(r)}
                              </span>
                            )}
                          </span>
                        )}
                        {r.failed_steps.length > 0 && (
                          <span className="text-amber-700 dark:text-amber-300">
                            {r.failed_steps.length} failed
                          </span>
                        )}
                      </div>
                    </div>
                    <div className="flex items-center gap-1 shrink-0">
                      <button
                        onClick={() => setOpenRunId(r.run_id)}
                        className="px-2 py-1 text-xs rounded border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
                      >
                        Open
                      </button>
                      {onRerun && (
                        <>
                          {r.resumable && (
                            <button
                              onClick={() => onRerun(r.run_id, true)}
                              className="px-2 py-1 text-xs rounded text-amber-700 hover:text-amber-900 dark:text-amber-300 dark:hover:text-amber-200"
                              title="Continue from the last completed step, reusing successful outputs and rerunning failed or missing work"
                            >
                              Resume
                            </button>
                          )}
                          <button
                            onClick={() => onRerun(r.run_id, false)}
                            className="px-2 py-1 text-xs rounded text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100"
                            title="Re-run all steps with the active profile, reusing the cached extraction and orientation"
                          >
                            Re-run
                          </button>
                        </>
                      )}
                      <button
                        onClick={() => startEdit(r)}
                        className="px-2 py-1 text-xs rounded text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100"
                        title="Rename / tag"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => deleteRun(r)}
                        className="px-2 py-1 text-xs rounded text-red-600 hover:text-red-800 dark:text-red-400 dark:hover:text-red-300"
                        title="Delete run"
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

import { useState, useEffect, useMemo, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import ArtifactExplorer from "./ArtifactExplorer";
import ComparePage from "./ComparePage";
import type { RunSummary, RunsDiskUsage } from "../lib/types";

interface Props {
  onClose: () => void;
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

export default function HistoryPage({ onClose, initialRunId, onRerun }: Props) {
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

  const toggleCompareId = (id: string) =>
    setCompareIds((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id].slice(-2)));

  const refresh = useCallback(() => {
    setLoading(true);
    Promise.all([
      invoke<RunSummary[]>("list_runs"),
      invoke<RunsDiskUsage>("runs_disk_usage").catch(() => null),
    ])
      .then(([r, u]) => {
        setRuns(r);
        setUsage(u);
        setError(null);
      })
      .catch((e) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refresh();
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
      refresh();
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
      refresh();
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
      <div className="flex flex-col h-full">
        <div className="flex items-center gap-3 px-6 py-2.5 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 shrink-0">
          <button
            onClick={() => setOpenRunId(null)}
            className="text-sm text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
          >
            ← Back to history
          </button>
          <span className="text-sm font-medium text-gray-800 dark:text-gray-200 truncate">
            {run ? run.title || run.input_name : openRunId}
          </span>
        </div>
        <div className="flex-1 overflow-auto min-h-0">
          <ArtifactExplorer runId={openRunId} fallbackMarkdown="" />
        </div>
      </div>
    );
  }

  // ── List view ──
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-3 px-6 py-3 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 shrink-0">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">Run history</h2>
        {usage && (
          <span className="text-xs text-gray-400 dark:text-gray-500">
            {usage.count} run{usage.count === 1 ? "" : "s"} · {fmtBytes(usage.bytes)} on disk
          </span>
        )}
        <input
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
        <button
          onClick={onClose}
          className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300"
          title="Close"
        >
          ✕
        </button>
      </div>

      {compareMode && (
        <div className="flex items-center gap-3 px-6 py-2 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-800 text-sm shrink-0">
          <span className="text-gray-500 dark:text-gray-400">
            {compareIds.length === 0 ? "Select two runs to compare." : `${compareIds.length} of 2 selected`}
          </span>
          <button
            onClick={() => compareIds.length === 2 && setComparing([compareIds[0], compareIds[1]])}
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
          <p className="text-gray-400 text-sm px-2">Loading…</p>
        ) : visible.length === 0 ? (
          <p className="text-gray-400 text-sm px-2">
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
                      value={editTitle}
                      onChange={(e) => setEditTitle(e.target.value)}
                      placeholder={r.input_name}
                      className="w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                      autoFocus
                    />
                    <input
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
                        title="Select for comparison"
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
                        {r.input_tokens + r.output_tokens > 0 && (
                          <span
                            title={[
                              `${r.input_tokens.toLocaleString()} input`,
                              `${r.output_tokens.toLocaleString()} output`,
                              `${(r.cached_input_tokens ?? 0).toLocaleString()} cached`,
                              `${(r.cache_write_input_tokens ?? 0).toLocaleString()} warmed`,
                            ].join(" · ")}
                          >
                            {fmtTokens(r.input_tokens)} / {fmtTokens(r.output_tokens)} tok
                            {(r.cached_input_tokens ?? 0) > 0 && (
                              <span className="text-green-600 dark:text-green-400">
                                {" "}· {fmtTokens(r.cached_input_tokens)} cached
                              </span>
                            )}
                            {(r.cache_write_input_tokens ?? 0) > 0 && (
                              <span className="text-blue-600 dark:text-blue-400">
                                {" "}· {fmtTokens(r.cache_write_input_tokens)} warmed
                              </span>
                            )}
                          </span>
                        )}
                        {r.failed_steps.length > 0 && (
                          <span className="text-amber-600 dark:text-amber-400">
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
                              className="px-2 py-1 text-xs rounded text-amber-600 hover:text-amber-800 dark:text-amber-400"
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
                        className="px-2 py-1 text-xs rounded text-red-500 hover:text-red-700 dark:hover:text-red-400"
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

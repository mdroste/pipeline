import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirmDialog, notify } from "./DialogService";
import ComparePage from "./ComparePage";
import ErrorBoundary from "./ErrorBoundary";
import ReportWorkspace from "./ReportWorkspace";
import type { Project, ProjectsResponse, RunSummary, RunsDiskUsage, TrashedRun } from "../lib/types";
import type { ArtifactSelectionTarget } from "./ArtifactExplorer";

interface Props {
  onClose: () => void;
  showClose?: boolean;
  /** When set, open this run's detail view immediately (e.g. from a batch job). */
  initialRunId?: string | null;
  initialSourceSelection?: ArtifactSelectionTarget | null;
  /** Re-run a past run (reusing its captured document/orientation and steps). */
  onRerun?: (runId: string, onlyFailed: boolean) => void;
  /** Disables Resume/Regenerate while a run or batch is already active. */
  runInProgress?: boolean;
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

function hasReportedUsage(run: RunSummary): boolean {
  return run.input_tokens + run.output_tokens > 0;
}

function usageDescription(run: RunSummary): string {
  const cacheRead = run.cached_input_tokens ?? 0;
  const cacheWrite = run.cache_write_input_tokens ?? 0;
  return [
    `Token usage: ${run.input_tokens.toLocaleString()} logical input tokens equals ${freshInputTokens(run).toLocaleString()} fresh input tokens plus ${cacheRead.toLocaleString()} cache-read tokens plus ${cacheWrite.toLocaleString()} cache-write tokens; ${run.output_tokens.toLocaleString()} output tokens.`,
    "Cache reads and cache writes are subsets of logical input, not additional tokens.",
    "Fresh input equals logical input minus cache reads minus cache writes.",
    "The completed report's summary prices these categories separately for a labelled API-equivalent estimate when the model is recognized.",
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
    degraded: "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300",
    partial: "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300",
    failed: "bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300",
    interrupted: "bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300",
    cancelled: "bg-gray-200 text-gray-600 dark:bg-gray-700 dark:text-gray-300",
  };
  const cls = map[status] || map.done;
  return <span className={`px-1.5 py-0.5 rounded text-xs font-medium ${cls}`}>{status || "done"}</span>;
}

export default function HistoryPage({
  onClose,
  showClose = true,
  initialRunId,
  initialSourceSelection,
  onRerun,
  runInProgress = false,
}: Props) {
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [usage, setUsage] = useState<RunsDiskUsage | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState("all");
  const [providerFilter, setProviderFilter] = useState("all");
  const [profileFilter, setProfileFilter] = useState("all");
  const [projectFilter, setProjectFilter] = useState("all");
  const [dateFilter, setDateFilter] = useState("all");
  const [sortOrder, setSortOrder] = useState("newest");
  const [pageNumber, setPageNumber] = useState(1);
  const [openRunId, setOpenRunId] = useState<string | null>(initialRunId ?? null);
  const [openSourceSelection, setOpenSourceSelection] = useState<ArtifactSelectionTarget | null>(
    initialSourceSelection ?? null,
  );
  // Which run's metadata is being edited inline (title + tags).
  const [editing, setEditing] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [editTags, setEditTags] = useState("");
  // Compare mode: pick two runs to diff.
  const [compareMode, setCompareMode] = useState(false);
  const [compareIds, setCompareIds] = useState<string[]>([]);
  const [comparing, setComparing] = useState<[string, string] | null>(null);
  const [trashOpen, setTrashOpen] = useState(false);
  const [trashedRuns, setTrashedRuns] = useState<TrashedRun[]>([]);
  const [trashLoading, setTrashLoading] = useState(false);
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
      const [nextRuns, nextUsage, nextProjects] = await Promise.all([
        invoke<RunSummary[]>("list_runs"),
        invoke<RunsDiskUsage>("runs_disk_usage").catch(() => null),
        invoke<ProjectsResponse>("list_projects").catch(() => null),
      ]);
      if (request !== refreshRequestRef.current) return;
      setRuns(nextRuns);
      setUsage(nextUsage);
      if (nextProjects) setProjects(nextProjects.projects);
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

  const refreshTrash = useCallback(async () => {
    setTrashLoading(true);
    try {
      setTrashedRuns(await invoke<TrashedRun[]>("list_trashed_runs"));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setTrashLoading(false);
    }
  }, []);

  useEffect(() => {
    if (trashOpen) void refreshTrash();
  }, [trashOpen, refreshTrash]);

  const needle = filter.trim().toLowerCase();
  const providers = useMemo(
    () => Array.from(new Set(runs.map((run) => run.provider).filter(Boolean))).sort(),
    [runs],
  );
  const profiles = useMemo(
    () => Array.from(new Set(runs.map((run) => run.profile_name || run.profile_id).filter(Boolean))).sort(),
    [runs],
  );
  const visible = useMemo(() => {
    const matching = runs.filter((run) => {
      if (statusFilter !== "all" && run.status !== statusFilter) return false;
      if (providerFilter !== "all" && run.provider !== providerFilter) return false;
      if (profileFilter !== "all" && (run.profile_name || run.profile_id) !== profileFilter) return false;
      if (projectFilter !== "all") {
        const project = projects.find((candidate) => candidate.id === projectFilter);
        if (!project?.run_ids.includes(run.run_id)) return false;
      }
      if (dateFilter !== "all") {
        const age = Date.now() - Date.parse(run.created);
        if (!Number.isFinite(age) || age > Number(dateFilter) * 24 * 60 * 60 * 1000) return false;
      }
      return !needle || [run.title, run.input_name, run.profile_name, run.provider, ...run.tags]
        .join(" ")
        .toLowerCase()
        .includes(needle);
    });
    return matching.sort((left, right) => {
      if (sortOrder === "oldest") return Date.parse(left.created) - Date.parse(right.created);
      if (sortOrder === "name") return (left.title || left.input_name).localeCompare(right.title || right.input_name);
      return Date.parse(right.created) - Date.parse(left.created);
    });
  }, [runs, needle, dateFilter, profileFilter, projectFilter, projects, providerFilter, sortOrder, statusFilter]);
  const pageSize = 25;
  const pageCount = Math.max(1, Math.ceil(visible.length / pageSize));
  const pagedRuns = visible.slice((pageNumber - 1) * pageSize, pageNumber * pageSize);

  useEffect(() => {
    setPageNumber(1);
  }, [dateFilter, filter, profileFilter, projectFilter, providerFilter, sortOrder, statusFilter]);

  useEffect(() => {
    setPageNumber((page) => Math.min(page, pageCount));
  }, [pageCount]);

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
    if (!(await confirmDialog(
      `Move report “${label}” and all its artifacts to Trash? You can restore it later.`,
      { title: "Move report to Trash", confirmLabel: "Move to Trash", destructive: true },
    ))) return;
    try {
      await invoke("delete_run", { runId: r.run_id });
      notify("Report moved to Trash.", "success");
      if (openRunId === r.run_id) {
        setOpenRunId(null);
        setOpenSourceSelection(null);
      }
      if (trashOpen) void refreshTrash();
      void refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const restoreRun = async (run: TrashedRun) => {
    try {
      await invoke("restore_trashed_run", { runId: run.run_id });
      notify("Report restored to History.", "success");
      await Promise.all([refresh(), refreshTrash()]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const permanentlyDeleteRun = async (run: TrashedRun) => {
    const label = run.title || run.input_name || run.run_id;
    if (!(await confirmDialog(
      `Permanently delete “${label}” and all its artifacts? This cannot be undone.`,
      { title: "Delete report forever", confirmLabel: "Delete forever", destructive: true },
    ))) return;
    try {
      await invoke("permanently_delete_trashed_run", { runId: run.run_id });
      notify("Report permanently deleted.", "success");
      await refreshTrash();
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
      // A rendering defect in one saved run must not take down History.
      <ErrorBoundary>
        <ReportWorkspace
          runId={openRunId}
          summary={run}
          onBack={() => {
            setOpenRunId(null);
            setOpenSourceSelection(null);
          }}
          initialSourceSelection={openSourceSelection}
        />
      </ErrorBoundary>
    );
  }

  // ── List view ──
  return (
    <div className="flex flex-col h-full">
      <div className="flex flex-wrap items-center gap-3 px-4 sm:px-6 py-3 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 shrink-0">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">Report history</h2>
        {usage && (
          <span className="text-xs text-gray-500 dark:text-gray-400">
            {usage.count} report{usage.count === 1 ? "" : "s"} · {fmtBytes(usage.bytes)} on disk
          </span>
        )}
        <input
          aria-label="Filter report history"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter by name, profile, tag…"
          className="sm:ml-auto w-full sm:w-64 py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-400"
        />
        <button
          onClick={() => setTrashOpen((value) => !value)}
          aria-expanded={trashOpen}
          aria-controls="report-trash"
          className={`text-sm transition-colors ${trashOpen ? "text-gray-900 dark:text-gray-100 font-medium" : "text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100"}`}
          title="Restore or permanently delete reports"
        >
          Trash{trashedRuns.length > 0 ? ` (${trashedRuns.length})` : ""}
        </button>
        <button
          onClick={() => { setCompareMode((v) => !v); setCompareIds([]); }}
          className={`text-sm transition-colors ${compareMode ? "text-gray-900 dark:text-gray-100 font-medium" : "text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100"}`}
          title="Select two reports to compare"
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
            aria-label="Close report history"
            className="text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100"
            title="Close"
          >
            ✕
          </button>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-2 border-b border-gray-200 bg-gray-50 px-4 py-2 text-xs dark:border-gray-800 dark:bg-gray-900/60 sm:px-6">
        <label>
          <span className="sr-only">Status</span>
          <select aria-label="Filter by status" value={statusFilter} onChange={(event) => setStatusFilter(event.target.value)} className="rounded border border-gray-300 bg-white px-2 py-1 dark:border-gray-700 dark:bg-gray-900">
            <option value="all">All statuses</option>
            {["done", "degraded", "partial", "failed", "interrupted", "cancelled"].map((status) => <option key={status} value={status}>{status}</option>)}
          </select>
        </label>
        <select aria-label="Filter by provider" value={providerFilter} onChange={(event) => setProviderFilter(event.target.value)} className="rounded border border-gray-300 bg-white px-2 py-1 dark:border-gray-700 dark:bg-gray-900">
          <option value="all">All providers</option>
          {providers.map((provider) => <option key={provider} value={provider}>{provider}</option>)}
        </select>
        <select aria-label="Filter by workflow" value={profileFilter} onChange={(event) => setProfileFilter(event.target.value)} className="max-w-56 rounded border border-gray-300 bg-white px-2 py-1 dark:border-gray-700 dark:bg-gray-900">
          <option value="all">All workflows</option>
          {profiles.map((profile) => <option key={profile} value={profile}>{profile}</option>)}
        </select>
        <select aria-label="Filter by project" value={projectFilter} onChange={(event) => setProjectFilter(event.target.value)} className="max-w-56 rounded border border-gray-300 bg-white px-2 py-1 dark:border-gray-700 dark:bg-gray-900">
          <option value="all">All projects</option>
          {projects.map((project) => <option key={project.id} value={project.id}>{project.name}</option>)}
        </select>
        <select aria-label="Filter by date" value={dateFilter} onChange={(event) => setDateFilter(event.target.value)} className="rounded border border-gray-300 bg-white px-2 py-1 dark:border-gray-700 dark:bg-gray-900">
          <option value="all">Any date</option><option value="7">Last 7 days</option><option value="30">Last 30 days</option><option value="365">Last year</option>
        </select>
        <select aria-label="Sort reports" value={sortOrder} onChange={(event) => setSortOrder(event.target.value)} className="rounded border border-gray-300 bg-white px-2 py-1 dark:border-gray-700 dark:bg-gray-900 sm:ml-auto">
          <option value="newest">Newest first</option>
          <option value="oldest">Oldest first</option>
          <option value="name">Name</option>
        </select>
        <span className="text-gray-500 dark:text-gray-400">{visible.length} matching</span>
      </div>

      {trashOpen && (
        <section
          id="report-trash"
          aria-label="Report Trash"
          className="px-6 py-3 border-b border-gray-200 dark:border-gray-800 bg-gray-50 dark:bg-gray-900/70 shrink-0"
        >
          <div className="flex items-center gap-3 mb-2">
            <h3 className="text-sm font-medium text-gray-900 dark:text-gray-100">Report Trash</h3>
            <span className="text-xs text-gray-500 dark:text-gray-400">
              Restore reports here, or deliberately delete them forever.
            </span>
            <button
              onClick={refreshTrash}
              className="ml-auto text-xs text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100"
            >
              Refresh Trash
            </button>
          </div>
          {trashLoading ? (
            <p className="text-xs text-gray-500 dark:text-gray-400">Loading Trash…</p>
          ) : trashedRuns.length === 0 ? (
            <p className="text-xs text-gray-500 dark:text-gray-400">Trash is empty.</p>
          ) : (
            <ul className="space-y-1 max-h-40 overflow-auto">
              {trashedRuns.map((run) => (
                <li
                  key={run.run_id}
                  className="flex items-center gap-3 rounded border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 px-3 py-2"
                >
                  <div className="min-w-0 flex-1">
                    <p className="text-sm text-gray-900 dark:text-gray-100 truncate">
                      {run.title || run.input_name || run.run_id}
                    </p>
                    <p className="text-xs text-gray-500 dark:text-gray-400">
                      Moved {fmtDate(run.deleted_at)}
                    </p>
                  </div>
                  <button
                    onClick={() => restoreRun(run)}
                    className="px-2 py-1 text-xs rounded border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
                  >
                    Restore
                  </button>
                  <button
                    onClick={() => permanentlyDeleteRun(run)}
                    className="px-2 py-1 text-xs rounded text-red-600 hover:text-red-800 dark:text-red-400 dark:hover:text-red-300"
                  >
                    Delete forever
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {compareMode && (
        <div className="flex items-center gap-3 px-6 py-2 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-800 text-sm shrink-0">
          <span className="text-gray-500 dark:text-gray-400">
            {compareIds.length === 0 ? "Select two reports to compare." : `${compareIds.length} of 2 selected`}
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
            {runs.length === 0 ? "No reports yet. Generate one to see it here." : "No reports match the filter."}
          </p>
        ) : (
          <div className="space-y-2">
            {pagedRuns.map((r) => (
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
                          onClick={() => {
                            setOpenSourceSelection(null);
                            setOpenRunId(r.run_id);
                          }}
                          className="text-sm font-medium text-gray-900 dark:text-gray-100 hover:underline truncate"
                          title="Open this report"
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
                          </span>
                        )}
                        {(r.model_round_trips ?? 0) > 0 && (
                          <span>{r.model_round_trips} model call{r.model_round_trips === 1 ? "" : "s"}</span>
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
                        onClick={() => {
                          setOpenSourceSelection(null);
                          setOpenRunId(r.run_id);
                        }}
                        className="px-2 py-1 text-xs rounded border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
                      >
                        Open
                      </button>
                      {onRerun && r.resumable && (
                        <button
                          onClick={() => onRerun(r.run_id, true)}
                          disabled={runInProgress}
                          className="rounded px-2 py-1 text-xs text-amber-700 hover:bg-amber-50 dark:text-amber-300 dark:hover:bg-amber-950/40 disabled:opacity-40"
                          title={runInProgress ? "A report is already being generated" : "Continue from the last completed step"}
                        >
                          Resume
                        </button>
                      )}
                      <details className="relative">
                        <summary aria-label={`More actions for ${r.title || r.input_name}`} className="cursor-pointer list-none rounded px-2 py-1 text-gray-500 hover:bg-gray-100 hover:text-gray-900 dark:hover:bg-gray-800 dark:hover:text-gray-100">•••</summary>
                        <div className="absolute right-0 z-20 mt-1 min-w-36 rounded-lg border border-gray-200 bg-white p-1 shadow-lg dark:border-gray-700 dark:bg-gray-900">
                          {onRerun && (
                          <button
                            onClick={() => onRerun(r.run_id, false)}
                            disabled={runInProgress}
                            className="block w-full rounded px-2 py-1.5 text-left text-xs text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800 disabled:opacity-40"
                            title={
                              runInProgress
                                ? "A report is already being generated"
                                : "Regenerate with the active workflow, reusing the captured document and orientation"
                            }
                          >
                            Regenerate
                          </button>
                          )}
                          <button onClick={() => startEdit(r)} className="block w-full rounded px-2 py-1.5 text-left text-xs text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800">Rename and tag</button>
                          <button onClick={() => deleteRun(r)} className="block w-full rounded px-2 py-1.5 text-left text-xs text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950/40">Move to Trash</button>
                        </div>
                      </details>
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
      {!loading && visible.length > pageSize && (
        <nav aria-label="Report history pages" className="flex shrink-0 items-center justify-center gap-3 border-t border-gray-200 px-4 py-2 text-xs dark:border-gray-800">
          <button disabled={pageNumber === 1} onClick={() => setPageNumber((page) => page - 1)} className="rounded border border-gray-300 px-2 py-1 disabled:opacity-40 dark:border-gray-700">Previous</button>
          <span>Page {pageNumber} of {pageCount}</span>
          <button disabled={pageNumber === pageCount} onClick={() => setPageNumber((page) => page + 1)} className="rounded border border-gray-300 px-2 py-1 disabled:opacity-40 dark:border-gray-700">Next</button>
        </nav>
      )}
    </div>
  );
}

import { selectHistoryRuns } from "./history-page/selectors";
import {
  isRunning,
  RUNNING_TITLE,
  fmtBytes,
  fmtTokens,
  freshInputTokens,
  hasReportedUsage,
  usageDescription,
  fmtDuration,
  fmtDate,
} from "./history-page/presentation";
import { reviewHistoryClient } from "../lib/reviewHistoryClient";
import IconButton from "../ui/IconButton";
import { Icon } from "../ui/icons";
import Badge, { type StatusTone } from "../ui/Badge";
import { lazy, Suspense, useState, useEffect, useMemo } from "react";
import { listedRunStatus } from "../lib/reportPresentation";
import { router } from "../lib/router";
import { confirmDialog, notify } from "./DialogService";
import ErrorBoundary from "./ErrorBoundary";
import ReportWorkspace from "./ReportWorkspace";
import useListScrollMemory from "./history-page/useListScrollMemory";
import ReviewHistoryTrash from "./history-page/ReviewHistoryTrash";
import { useHistoryData } from "./history-page/useHistoryData";
import useRefreshWhenRunsFinish from "./history-page/useRefreshWhenRunsFinish";
import type { RunSummary } from "../lib/reviewTypes";
import type { ArtifactSelectionTarget } from "../lib/artifactTypes";
import ReportIssueLink from "./ReportIssueLink";

const ComparePage = lazy(() => import("./ComparePage"));

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

function StatusBadge({ status }: { status: string }) {
  const tones: Record<string, StatusTone> = {
    running: "running",
    done: "ok",
    partial: "attention",
    failed: "error",
    interrupted: "error",
    cancelled: "idle",
  };
  return (
    <Badge
      kind={tones[status] ?? "idle"}
      label={status || "Unknown"}
      showLabel
    />
  );
}

export default function HistoryPage({
  onClose,
  showClose = true,
  initialRunId,
  initialSourceSelection,
  onRerun,
  runInProgress = false,
}: Props) {
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState("all");
  const [providerFilter, setProviderFilter] = useState("all");
  const [profileFilter, setProfileFilter] = useState("all");
  const [projectFilter, setProjectFilter] = useState("all");
  const [dateFilter, setDateFilter] = useState("all");
  const [sortOrder, setSortOrder] = useState("newest");
  const [pageNumber, setPageNumber] = useState(1);
  const [openRunId, setOpenRunId] = useState<string | null>(
    initialRunId ?? null,
  );
  const [openSourceSelection, setOpenSourceSelection] =
    useState<ArtifactSelectionTarget | null>(initialSourceSelection ?? null);
  // Which run's metadata is being edited inline (title + tags).
  const [editing, setEditing] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [editTags, setEditTags] = useState("");
  // Compare mode: pick two runs to diff.
  const [compareMode, setCompareMode] = useState(false);
  const [compareIds, setCompareIds] = useState<string[]>([]);
  const [comparing, setComparing] = useState<[string, string] | null>(null);
  const [trashOpen, setTrashOpen] = useState(false);
  const {
    runs,
    projects,
    usage,
    loading,
    error,
    setError,
    refresh,
    refreshRequestRef,
    trashedRuns,
    trashLoading,
    refreshTrash,
    restoreRun,
    permanentlyDeleteRun,
  } = useHistoryData(setCompareIds);

  useEffect(
    () => () => {
      refreshRequestRef.current += 1;
    },
    [],
  );

  // The route can change while this page stays mounted, and App may supply
  // the cited source just after the route commits.
  useEffect(() => {
    setOpenRunId(initialRunId ?? null);
  }, [initialRunId]);
  useEffect(() => {
    setOpenSourceSelection(initialSourceSelection ?? null);
  }, [initialSourceSelection]);

  // Rows stay on screen during a reload; only the first load replaces them.
  const showLoading = loading && runs.length === 0;
  const scroll = useListScrollMemory(
    [
      pageNumber,
      filter,
      statusFilter,
      providerFilter,
      profileFilter,
      projectFilter,
      dateFilter,
      sortOrder,
    ].join("\0"),
    !comparing && !openRunId && !showLoading,
  );

  const openRun = (runId: string) => {
    scroll.remember();
    setOpenSourceSelection(null);
    setOpenRunId(runId);
  };

  const toggleCompareId = (id: string) =>
    setCompareIds((prev) =>
      prev.includes(id)
        ? prev.filter((x) => x !== id)
        : [...prev, id].slice(-2),
    );

  const compareSelected = () => {
    if (compareIds.length !== 2) return;
    const [first, second] = compareIds;
    const firstCreated = Date.parse(
      runs.find((run) => run.run_id === first)?.created ?? "",
    );
    const secondCreated = Date.parse(
      runs.find((run) => run.run_id === second)?.created ?? "",
    );
    scroll.remember();
    if (
      Number.isFinite(firstCreated) &&
      Number.isFinite(secondCreated) &&
      firstCreated > secondCreated
    ) {
      setComparing([second, first]);
    } else {
      setComparing([first, second]);
    }
  };

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (trashOpen) void refreshTrash();
  }, [trashOpen, refreshTrash]);

  useRefreshWhenRunsFinish(refresh);

  const needle = filter.trim().toLowerCase();
  const providers = useMemo(
    () =>
      Array.from(
        new Set(runs.map((run) => run.provider).filter(Boolean)),
      ).sort(),
    [runs],
  );
  const profiles = useMemo(
    () =>
      Array.from(
        new Set(
          runs.map((run) => run.profile_name || run.profile_id).filter(Boolean),
        ),
      ).sort(),
    [runs],
  );
  const visible = useMemo(
    () =>
      selectHistoryRuns({
        runs,
        needle,
        dateFilter,
        profileFilter,
        projectFilter,
        projects,
        providerFilter,
        sortOrder,
        statusFilter,
      }),
    [
      runs,
      needle,
      dateFilter,
      profileFilter,
      projectFilter,
      projects,
      providerFilter,
      sortOrder,
      statusFilter,
    ],
  );
  const pageSize = 25;
  const pageCount = Math.max(1, Math.ceil(visible.length / pageSize));
  const pagedRuns = visible.slice(
    (pageNumber - 1) * pageSize,
    pageNumber * pageSize,
  );

  useEffect(() => {
    setPageNumber(1);
  }, [
    dateFilter,
    filter,
    profileFilter,
    projectFilter,
    providerFilter,
    sortOrder,
    statusFilter,
  ]);

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
      await reviewHistoryClient.updateMeta({ runId, title: editTitle, tags });
      setEditing(null);
      void refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const deleteRun = async (r: RunSummary) => {
    const label = r.title || r.input_name;
    if (
      !(await confirmDialog(
        `Move report “${label}” and all its artifacts to Trash? You can restore it later.`,
        {
          title: "Move report to Trash",
          confirmLabel: "Move to Trash",
          destructive: true,
        },
      ))
    )
      return;
    try {
      await reviewHistoryClient.trash({ runId: r.run_id });
      notify("Report moved to Trash.", "success");
      setCompareIds((prev) => prev.filter((id) => id !== r.run_id));
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

  // ── Compare view: diff two runs ──
  if (comparing) {
    return (
      <Suspense fallback={null}>
        <ComparePage
          runA={comparing[0]}
          runB={comparing[1]}
          onBack={() => setComparing(null)}
        />
      </Suspense>
    );
  }

  // ── Detail view: browse one past run's artifacts ──
  if (openRunId) {
    const run = runs.find((r) => r.run_id === openRunId);
    return (
      // A rendering defect in one saved run must not take down History.
      <ErrorBoundary nested>
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

  const filterSummary = [
    statusFilter !== "all" && `Status: ${statusFilter}`,
    providerFilter !== "all" && `Provider: ${providerFilter}`,
    profileFilter !== "all" && `Workflow: ${profileFilter}`,
    projectFilter !== "all" &&
      `Collection: ${projects.find((project) => project.id === projectFilter)?.name ?? projectFilter}`,
    dateFilter !== "all" &&
      (dateFilter === "365" ? "Last year" : `Last ${dateFilter} days`),
  ].filter(Boolean);
  // ── List view ──
  return (
    <div ref={scroll.rootRef} className="flex flex-col h-full">
      <header className="shrink-0 space-y-3 border-b border-line bg-surface px-[var(--ui-page-gutter)] py-4">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="mr-auto text-xl font-semibold tracking-tight text-ink">
            Review history
          </h2>
          <div
            className="flex flex-wrap items-center gap-2"
            role="group"
            aria-label="History actions"
          >
            <button
              className="ui-button ui-button--sm"
              onClick={() => void router.navigate({ page: "projects" })}
            >
              Collections
            </button>
            <button
              className="ui-button ui-button--sm"
              aria-expanded={trashOpen}
              aria-controls="report-trash"
              onClick={() => setTrashOpen((value) => !value)}
            >
              Trash{trashedRuns.length > 0 ? ` (${trashedRuns.length})` : ""}
            </button>
            <button
              className="ui-button ui-button--sm"
              onClick={() => void refresh()}
            >
              Refresh
            </button>
            {showClose && (
              <IconButton label="Close report history" onClick={onClose}>
                <Icon name="close" />
              </IconButton>
            )}
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <input
            type="search"
            aria-label="Filter report history"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter by name, profile, tag…"
            className="ui-input min-w-0 flex-[1_1_12rem]"
          />
          <button
            type="button"
            className="ui-button"
            aria-expanded={filtersOpen}
            aria-controls="review-history-filters"
            onClick={() => setFiltersOpen((open) => !open)}
          >
            Filters
            {filterSummary.length > 0 ? ` (${filterSummary.length})` : ""}
          </button>
          <button
            onClick={() => {
              setCompareMode((v) => !v);
              setCompareIds([]);
            }}
            aria-pressed={compareMode}
            className="ui-button aria-pressed:bg-sunken"
            title="Select two reports to compare"
          >
            Compare
          </button>
          <select
            aria-label="Sort reports"
            value={sortOrder}
            onChange={(event) => setSortOrder(event.target.value)}
            className="max-w-full"
          >
            <option value="newest">Newest first</option>
            <option value="oldest">Oldest first</option>
            <option value="name">Name</option>
          </select>
          <span className="text-ui-meta text-ink-muted">
            {visible.length} matching
          </span>
        </div>
        {filterSummary.length > 0 && (
          <ul
            aria-label="Active filters"
            className="flex flex-wrap gap-2 text-ui-meta text-ink-muted"
          >
            {filterSummary.map((label) => (
              <li
                key={String(label)}
                className="min-w-0 break-words rounded-ui-sm bg-sunken px-2 py-1"
              >
                {label}
              </li>
            ))}
          </ul>
        )}
        {usage && (
          <p className="text-ui-meta text-ink-muted">
            {usage.count} report{usage.count === 1 ? "" : "s"} ·{" "}
            {fmtBytes(usage.bytes)} on disk
          </p>
        )}
        <div
          id="review-history-filters"
          role="group"
          aria-label="Review filters"
          hidden={!filtersOpen}
          className={
            filtersOpen
              ? "flex flex-wrap gap-2 rounded-ui-md bg-sunken p-3"
              : undefined
          }
        >
          <label>
            <span className="sr-only">Status</span>
            <select
              aria-label="Filter by status"
              value={statusFilter}
              onChange={(event) => setStatusFilter(event.target.value)}
              className="max-w-full"
            >
              <option value="all">All statuses</option>
              {[
                "running",
                "done",
                "partial",
                "failed",
                "interrupted",
                "cancelled",
              ].map((status) => (
                <option key={status} value={status}>
                  {status}
                </option>
              ))}
            </select>
          </label>
          <select
            aria-label="Filter by provider"
            value={providerFilter}
            onChange={(event) => setProviderFilter(event.target.value)}
            className="max-w-full"
          >
            <option value="all">All providers</option>
            {providers.map((provider) => (
              <option key={provider} value={provider}>
                {provider}
              </option>
            ))}
          </select>
          <select
            aria-label="Filter by workflow"
            value={profileFilter}
            onChange={(event) => setProfileFilter(event.target.value)}
            className="max-w-full sm:max-w-56"
          >
            <option value="all">All workflows</option>
            {profiles.map((profile) => (
              <option key={profile} value={profile}>
                {profile}
              </option>
            ))}
          </select>
          <select
            aria-label="Filter by review collection"
            value={projectFilter}
            onChange={(event) => setProjectFilter(event.target.value)}
            className="max-w-full sm:max-w-56"
          >
            <option value="all">All review collections</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.name}
              </option>
            ))}
          </select>
          <select
            aria-label="Filter by date"
            value={dateFilter}
            onChange={(event) => setDateFilter(event.target.value)}
            className="max-w-full"
          >
            <option value="all">Any date</option>
            <option value="7">Last 7 days</option>
            <option value="30">Last 30 days</option>
            <option value="365">Last year</option>
          </select>
        </div>
      </header>

      {trashOpen && (
        <ReviewHistoryTrash
          runs={trashedRuns}
          loading={trashLoading}
          refresh={refreshTrash}
          restore={restoreRun}
          remove={permanentlyDeleteRun}
        />
      )}

      {compareMode && (
        <div className="flex flex-wrap items-center gap-3 border-b border-line bg-sunken px-[var(--ui-page-gutter)] py-2 text-ui-label shrink-0">
          <span className="text-gray-500 dark:text-gray-400">
            {compareIds.length === 0
              ? "Select two reports to compare."
              : `${compareIds.length} of 2 selected`}
          </span>
          <button
            onClick={compareSelected}
            disabled={compareIds.length !== 2}
            className="ui-button ui-button--sm ui-button--primary ml-auto"
          >
            Compare selected
          </button>
        </div>
      )}

      {error && (
        <div
          role="alert"
          className="ui-notice ui-notice--error mx-4 mt-4 shrink-0"
        >
          {error}
          <ReportIssueLink />
        </div>
      )}
      <div ref={scroll.listRef} className="flex-1 overflow-auto p-4">
        {showLoading ? (
          <p className="text-gray-500 dark:text-gray-400 text-sm px-2">
            Loading…
          </p>
        ) : visible.length === 0 ? (
          <p className="text-gray-500 dark:text-gray-400 text-sm px-2">
            {runs.length === 0
              ? "No reports yet. Generate one to see it here."
              : "No reports match the filter."}
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
                        disabled={isRunning(r)}
                        onChange={() => toggleCompareId(r.run_id)}
                        className="mt-1 shrink-0"
                        aria-label={`Select ${r.title || r.input_name} for comparison`}
                        title={isRunning(r) ? RUNNING_TITLE : undefined}
                      />
                    )}
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 flex-wrap">
                        {isRunning(r) ? (
                          <span className="text-sm font-medium text-gray-900 dark:text-gray-100 truncate">
                            {r.title || r.input_name}
                          </span>
                        ) : (
                          <button
                            onClick={() => openRun(r.run_id)}
                            className="text-sm font-medium text-gray-900 dark:text-gray-100 hover:underline truncate"
                            title="Open this report"
                          >
                            {r.title || r.input_name}
                          </button>
                        )}
                        <StatusBadge status={listedRunStatus(r.status)} />
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
                        <span>
                          {r.step_count} step{r.step_count === 1 ? "" : "s"}
                        </span>
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
                              <span className="text-ink dark:text-green-400">
                                {" "}
                                + {fmtTokens(r.cached_input_tokens)} cache read
                              </span>
                            )}
                            {(r.cache_write_input_tokens ?? 0) > 0 && (
                              <span className="text-blue-600 dark:text-blue-400">
                                {" "}
                                + {fmtTokens(r.cache_write_input_tokens)} cache
                                write
                              </span>
                            )}
                            {" · "}
                            {fmtTokens(r.output_tokens)} output
                          </span>
                        )}
                        {(r.model_round_trips ?? 0) > 0 && (
                          <span>
                            {r.model_round_trips} model call
                            {r.model_round_trips === 1 ? "" : "s"}
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
                        onClick={() => openRun(r.run_id)}
                        disabled={isRunning(r)}
                        title={isRunning(r) ? RUNNING_TITLE : undefined}
                        className="px-2 py-1 text-xs rounded border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-40"
                      >
                        Open
                      </button>
                      {/* A running review cannot be resumed, regenerated,
                          renamed or moved to Trash until it finishes. */}
                      {!isRunning(r) && onRerun && r.resumable && (
                        <button
                          onClick={() => onRerun(r.run_id, true)}
                          disabled={runInProgress}
                          className="rounded px-2 py-1 text-xs text-amber-700 hover:bg-amber-50 dark:text-amber-300 dark:hover:bg-amber-950/40 disabled:opacity-40"
                          title={
                            runInProgress
                              ? "A report is already being generated"
                              : `Continue from the last completed step with ${r.profile_name || "this run's review design"}`
                          }
                        >
                          Resume
                        </button>
                      )}
                      {!isRunning(r) && (
                        <details className="relative">
                          <summary
                            aria-label={`More actions for ${r.title || r.input_name}`}
                            className="cursor-pointer list-none rounded px-2 py-1 text-gray-500 hover:bg-gray-100 hover:text-gray-900 dark:hover:bg-gray-800 dark:hover:text-gray-100"
                          >
                            •••
                          </summary>
                          <div className="absolute right-0 z-20 mt-1 min-w-36 rounded-lg border border-gray-200 bg-white p-1 shadow-lg dark:border-gray-700 dark:bg-gray-900">
                            {onRerun && (
                              <button
                                onClick={() => onRerun(r.run_id, false)}
                                disabled={runInProgress}
                                className="block w-full rounded px-2 py-1.5 text-left text-xs text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800 disabled:opacity-40"
                                title={
                                  runInProgress
                                    ? "A report is already being generated"
                                    : `Run ${r.profile_name || "this run's review design"} again using the saved document and document bundle preparation.`
                                }
                              >
                                Regenerate
                              </button>
                            )}
                            <button
                              onClick={() => startEdit(r)}
                              className="block w-full rounded px-2 py-1.5 text-left text-xs text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800"
                            >
                              Rename and tag
                            </button>
                            <button
                              onClick={() => deleteRun(r)}
                              className="block w-full rounded px-2 py-1.5 text-left text-xs text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950/40"
                            >
                              Move to Trash
                            </button>
                          </div>
                        </details>
                      )}
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
      {!loading && visible.length > pageSize && (
        <nav
          aria-label="Review history pages"
          className="flex shrink-0 items-center justify-center gap-3 border-t border-gray-200 px-4 py-2 text-xs dark:border-gray-800"
        >
          <button
            disabled={pageNumber === 1}
            onClick={() => setPageNumber((page) => page - 1)}
            className="rounded border border-gray-300 px-2 py-1 disabled:opacity-40 dark:border-gray-700"
          >
            Previous
          </button>
          <span>
            Page {pageNumber} of {pageCount}
          </span>
          <button
            disabled={pageNumber === pageCount}
            onClick={() => setPageNumber((page) => page + 1)}
            className="rounded border border-gray-300 px-2 py-1 disabled:opacity-40 dark:border-gray-700"
          >
            Next
          </button>
        </nav>
      )}
    </div>
  );
}

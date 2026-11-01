import {
  fmtTokens,
  fmtClock,
  isError,
  isWarn,
  logLineClass,
  hasTokenUsage,
  sessionName,
  summarizeLogs,
  usageDescription,
} from "../lib/consolePresentation";
import RequestDetails from "./console/RequestDetails";
import TokenUsage from "./console/TokenUsage";
import { useState, useMemo, useRef, useEffect, useCallback } from "react";
import type { LogEntry, UsageState } from "../lib/reviewRuntimeTypes";

interface Props {
  logs: LogEntry[];
  usage: UsageState;
  /** True only while the foreground or batch run that owns these logs is active. */
  active?: boolean;
}

type LevelFilter = "all" | "warn" | "error";

/** The first rendered line while paused, under the filter it was taken with. */
interface WindowAnchor {
  entry: LogEntry;
  filter: string;
}

const DEFAULT_CONSOLE_HEIGHT = 192;
const MIN_CONSOLE_HEIGHT = 96;
const CONSOLE_VIEWPORT_MARGIN = 96;
const MAX_RENDERED_LOG_LINES = 600;

/** The console leaves room in the column it shares with the review view, which
 *  banners can make shorter than the window. */
function maxConsoleHeight(panel: HTMLElement | null): number {
  const column = panel?.parentElement?.clientHeight || window.innerHeight;
  return Math.max(
    MIN_CONSOLE_HEIGHT,
    Math.min(column, window.innerHeight) - CONSOLE_VIEWPORT_MARGIN,
  );
}

function clampConsoleHeight(height: number, panel: HTMLElement | null): number {
  return Math.min(
    maxConsoleHeight(panel),
    Math.max(MIN_CONSOLE_HEIGHT, height),
  );
}

/** The pipeline console: session filter, text search, level filter, timestamps,
 *  per-line copy, jump-to-error, auto-scroll that pauses when scrolled up, and
 *  copy of the visible lines. */
export default function Console({ logs, usage, active = true }: Props) {
  const [open, setOpen] = useState(true);
  const [height, setHeight] = useState(DEFAULT_CONSOLE_HEIGHT);
  const [selectedSession, setSelectedSession] = useState<number | "master">(
    "master",
  );
  const [search, setSearch] = useState("");
  const [level, setLevel] = useState<LevelFilter>("all");
  const [showTimestamps, setShowTimestamps] = useState(false);
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState<string | null>(null);
  // Auto-follow the tail unless the user scrolls up.
  const [follow, setFollow] = useState(true);
  // While paused, the rendered window starts at this line, so appended lines
  // neither slide the text being read nor push it out of the window.
  const [anchor, setAnchor] = useState<WindowAnchor | null>(null);
  // Each "jump to first error" request scrolls exactly once.
  const [errorJump, setErrorJump] = useState(0);

  const panelRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const resizeCleanupRef = useRef<(() => void) | null>(null);
  const previousActiveRef = useRef(active);

  useEffect(() => () => resizeCleanupRef.current?.(), []);

  useEffect(() => {
    const fitWindow = () =>
      setHeight((current) => clampConsoleHeight(current, panelRef.current));
    window.addEventListener("resize", fitWindow);
    return () => window.removeEventListener("resize", fitWindow);
  }, []);

  useEffect(() => {
    if (previousActiveRef.current && !active) setOpen(false);
    previousActiveRef.current = active;
  }, [active]);

  const stopResizing = useCallback(() => {
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
  }, []);

  const startResizing = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      event.preventDefault();
      resizeCleanupRef.current?.();

      const consoleElement = event.currentTarget.parentElement;
      const measuredHeight =
        consoleElement?.getBoundingClientRect().height ?? 0;
      const startHeight = measuredHeight > 0 ? measuredHeight : height;
      const startY = event.clientY;

      const onMouseMove = (moveEvent: MouseEvent) => {
        setHeight(
          clampConsoleHeight(
            startHeight + startY - moveEvent.clientY,
            panelRef.current,
          ),
        );
      };
      const onMouseUp = () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        stopResizing();
        resizeCleanupRef.current = null;
      };

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
      document.body.style.cursor = "row-resize";
      document.body.style.userSelect = "none";
      resizeCleanupRef.current = onMouseUp;
    },
    [height, stopResizing],
  );

  const resizeWithKeyboard = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      const delta = event.shiftKey ? 48 : 16;
      let next: number | null = null;
      if (event.key === "ArrowUp") next = height + delta;
      if (event.key === "ArrowDown") next = height - delta;
      if (event.key === "Home") next = MIN_CONSOLE_HEIGHT;
      if (event.key === "End") next = maxConsoleHeight(panelRef.current);
      if (next == null) return;
      event.preventDefault();
      setHeight(clampConsoleHeight(next, panelRef.current));
    },
    [height],
  );

  // Build the session summary and global error count in one pass. This runs
  // whenever the buffered log snapshot changes, so avoid separate full-log
  // scans for header metadata.
  const { sessions, errorCount } = useMemo(() => summarizeLogs(logs), [logs]);

  // A stale selection (previous run's session id) falls back to the master view.
  const activeSession =
    selectedSession !== "master" &&
    !sessions.some((s) => s.id === selectedSession)
      ? "master"
      : selectedSession;
  const activeRequest =
    activeSession === "master"
      ? undefined
      : sessions.find((session) => session.id === activeSession)?.request;
  const activeUsage =
    activeSession === "master" ? undefined : usage.bySession[activeSession];
  // Run totals sit above the master log; a selected call's usage lives in its
  // request card, or in the same strip when the call reported no request.
  const stripUsage =
    activeSession === "master"
      ? usage.total
      : activeRequest
        ? undefined
        : activeUsage;

  const needle = search.trim().toLowerCase();
  const visibleLogs = useMemo(() => {
    return logs.filter((e) => {
      if (activeSession !== "master" && e.session !== activeSession)
        return false;
      if (level === "error" && !isError(e)) return false;
      if (level === "warn" && !isError(e) && !isWarn(e)) return false;
      if (needle && !e.line.toLowerCase().includes(needle)) return false;
      return true;
    });
  }, [logs, activeSession, level, needle]);

  // Following shows the newest lines. Paused, the window stays at its anchor;
  // a filter change re-anchors at the new tail.
  const filterKey = JSON.stringify([activeSession, level, needle]);
  const anchorEntry =
    !follow && anchor?.filter === filterKey ? anchor.entry : null;
  const windowStart = useMemo(() => {
    const overflow = visibleLogs.length - MAX_RENDERED_LOG_LINES;
    if (overflow <= 0) return 0;
    if (!anchorEntry) return overflow;
    const index = visibleLogs.indexOf(anchorEntry);
    // The anchor can drop out of the bounded log buffer. Keep the oldest
    // retained lines in view rather than jumping to the tail.
    return index < 0 ? 0 : Math.min(index, overflow);
  }, [visibleLogs, anchorEntry]);
  const renderedLogs = useMemo(
    () => visibleLogs.slice(windowStart, windowStart + MAX_RENDERED_LOG_LINES),
    [visibleLogs, windowStart],
  );
  const laterLines = visibleLogs.length - windowStart - renderedLogs.length;

  // Pin the window where following stopped (scrolling up, choosing a session,
  // or filtering while paused).
  useEffect(() => {
    if (!follow && !anchorEntry && renderedLogs.length > 0)
      setAnchor({ entry: renderedLogs[0], filter: filterKey });
  }, [follow, anchorEntry, renderedLogs, filterKey]);

  // Auto-scroll to the tail when following; disabled once the user scrolls up.
  useEffect(() => {
    const element = scrollRef.current;
    if (open && follow && element) element.scrollTop = element.scrollHeight;
  }, [renderedLogs, open, follow]);

  // Only on request: new log lines must not pull the reader back to the error.
  useEffect(() => {
    if (errorJump === 0) return;
    scrollRef.current
      ?.querySelector<HTMLElement>("[data-error='1']")
      ?.scrollIntoView?.({ block: "center" });
  }, [errorJump]);

  const resumeFollowing = useCallback(() => {
    setFollow(true);
    setAnchor(null);
  }, []);

  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
    if (atBottom) resumeFollowing();
    else setFollow(false);
  }, [resumeFollowing]);

  const jumpToFirstError = useCallback(() => {
    const firstError = visibleLogs.findIndex(isError);
    if (firstError < 0) return;
    // Start the window at the error, or at the final full window when the
    // error is among the newest lines.
    const start = Math.max(
      0,
      Math.min(firstError, visibleLogs.length - MAX_RENDERED_LOG_LINES),
    );
    setOpen(true);
    setFollow(false);
    setAnchor({ entry: visibleLogs[start], filter: filterKey });
    setErrorJump((count) => count + 1);
  }, [visibleLogs, filterKey]);

  const visibleText = useCallback(
    () =>
      visibleLogs
        .map((e) => (showTimestamps ? `[${fmtClock(e.t)}] ${e.line}` : e.line))
        .join("\n"),
    [visibleLogs, showTimestamps],
  );

  const copyText = useCallback(async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopyError(null);
      return true;
    } catch {
      // Fallback for webviews without async clipboard access.
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      let copied = false;
      try {
        copied = document.execCommand("copy");
      } catch {
        copied = false;
      }
      document.body.removeChild(ta);
      if (!copied) {
        setCopyError(
          "Clipboard access failed. Select the text and copy it manually.",
        );
        return false;
      }
      setCopyError(null);
      return true;
    }
  }, []);

  const copyAll = useCallback(async () => {
    if (await copyText(visibleText())) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  }, [copyText, visibleText]);

  return (
    <div
      ref={panelRef}
      data-testid="console-panel"
      className="console-panel relative flex shrink-0 flex-col border-t border-gray-200 bg-white text-gray-700 dark:border-gray-800 dark:bg-gray-950 dark:text-gray-300"
      style={{ height: open ? `${height}px` : undefined }}
    >
      {open && (
        <div
          role="separator"
          aria-label="Resize console"
          aria-orientation="horizontal"
          aria-valuemin={MIN_CONSOLE_HEIGHT}
          aria-valuemax={maxConsoleHeight(panelRef.current)}
          aria-valuenow={Math.round(height)}
          tabIndex={0}
          title="Drag to resize the console"
          onMouseDown={startResizing}
          onKeyDown={resizeWithKeyboard}
          className="group absolute -top-1 left-0 z-20 h-2 w-full cursor-row-resize focus:outline-none"
        >
          <div className="absolute left-0 top-1/2 h-px w-full -translate-y-1/2 bg-transparent transition-colors group-hover:bg-blue-400 group-focus:bg-blue-400 group-active:bg-blue-500" />
        </div>
      )}
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-x-3 gap-y-1 border-b border-gray-200 bg-gray-50 px-3 py-1.5 text-xs text-gray-600 dark:border-gray-800 dark:bg-gray-900 dark:text-gray-400">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
          <button
            onClick={() => setOpen(!open)}
            aria-expanded={open}
            className="flex shrink-0 cursor-pointer select-none items-center gap-1.5 rounded-sm font-medium text-gray-700 transition-colors hover:text-gray-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:text-gray-300 dark:hover:text-gray-100"
          >
            <svg
              aria-hidden
              viewBox="0 0 12 12"
              className={`h-2.5 w-2.5 transition-transform ${open ? "rotate-90" : ""}`}
            >
              <path
                d="M4 2.5 7.5 6 4 9.5"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.5"
              />
            </svg>
            <span>
              {active ? "Active run console" : "Completed run console"}
            </span>
          </button>
          {sessions.length > 0 && (
            <select
              aria-label="Console session"
              value={
                activeSession === "master" ? "master" : String(activeSession)
              }
              onChange={(e) => {
                const next =
                  e.target.value === "master"
                    ? "master"
                    : Number(e.target.value);
                setSelectedSession(next);
                if (next !== "master") {
                  setFollow(false);
                  scrollRef.current?.scrollTo?.({ top: 0 });
                }
              }}
              className="w-48 max-w-full cursor-pointer truncate rounded-md border border-gray-300 bg-white py-0.5 pl-1.5 pr-6 text-xs text-gray-700 focus:outline-none focus:ring-1 focus:ring-gray-400 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-300"
              title={
                activeSession === "master"
                  ? "Show a single model call's log, or all of them"
                  : sessions.find((s) => s.id === activeSession)?.label
              }
            >
              <option value="master">All sessions ({sessions.length})</option>
              {sessions.map((s) => (
                <option
                  key={s.id}
                  value={String(s.id)}
                  title={
                    s.request
                      ? `${s.label} · ${s.request.provider_label} ${s.request.transport.toUpperCase()} · ${s.request.model}`
                      : s.label
                  }
                >
                  {(s.hasError ? "✕ " : "") + sessionName(s.label, s.request)}
                </option>
              ))}
            </select>
          )}
          {open && (
            <>
              <input
                aria-label="Search active run console"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search…"
                className="w-24 rounded-md border border-gray-300 bg-white px-1.5 py-0.5 text-xs text-gray-700 transition-all placeholder:text-gray-400 focus:w-40 focus:outline-none focus:ring-1 focus:ring-gray-400 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-300 dark:placeholder:text-gray-600 dark:focus:ring-gray-500"
              />
              {/* Level filter chips */}
              <div className="flex shrink-0 items-center gap-0.5">
                {(["all", "warn", "error"] as LevelFilter[]).map((lv) => (
                  <button
                    key={lv}
                    onClick={() => setLevel(lv)}
                    aria-pressed={level === lv}
                    className={`rounded px-1.5 py-0.5 text-xs transition-colors ${
                      level === lv
                        ? "bg-gray-700 text-white dark:bg-gray-600 dark:text-gray-100"
                        : "text-gray-500 hover:bg-gray-200 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-200"
                    }`}
                    title={
                      lv === "all"
                        ? "All lines"
                        : lv === "warn"
                          ? "Warnings + errors"
                          : "Errors only"
                    }
                  >
                    {lv === "all" ? "All" : lv === "warn" ? "Warn" : "Err"}
                  </button>
                ))}
              </div>
            </>
          )}
        </div>

        <div className="ml-auto flex shrink-0 items-center justify-end gap-2 tabular-nums">
          {!open && hasTokenUsage(usage.total) && (
            <span
              className="text-gray-500 dark:text-gray-400"
              title={usageDescription(usage.total)}
            >
              {fmtTokens(usage.total.input)} → {fmtTokens(usage.total.output)}{" "}
              tokens
            </span>
          )}
          {errorCount > 0 && (
            <button
              onClick={jumpToFirstError}
              className="cursor-pointer text-red-600 transition-colors hover:text-red-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-300 dark:text-red-400 dark:hover:text-red-300"
              title="Scroll to the first error"
            >
              {errorCount} {errorCount === 1 ? "error" : "errors"}
            </button>
          )}
          <span>{visibleLogs.length} lines</span>
          {open && (
            <button
              onClick={() => setShowTimestamps((v) => !v)}
              aria-label="Show console timestamps"
              aria-pressed={showTimestamps}
              className={`cursor-pointer rounded-md border border-gray-300 px-1.5 py-0.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-gray-400 dark:border-gray-700 ${
                showTimestamps
                  ? "bg-gray-700 text-white dark:bg-gray-600 dark:text-gray-100"
                  : "bg-white text-gray-600 hover:bg-gray-100 hover:text-gray-900 dark:bg-gray-900 dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-gray-100"
              }`}
              title="Toggle timestamps"
            >
              🕑
            </button>
          )}
          <button
            onClick={copyAll}
            disabled={visibleLogs.length === 0}
            className="cursor-pointer rounded-md border border-gray-300 bg-white px-1.5 py-0.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 hover:text-gray-900 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-gray-400 disabled:cursor-default disabled:opacity-40 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            title={
              activeSession === "master"
                ? "Copy all shown lines"
                : "Copy this session's lines"
            }
          >
            {copied ? "Copied ✓" : "Copy"}
          </button>
        </div>
      </div>

      {open && stripUsage && hasTokenUsage(stripUsage) && (
        <div className="shrink-0 border-b border-gray-200 px-3 py-1.5 text-xs dark:border-gray-800">
          <TokenUsage
            usage={stripUsage}
            label={activeSession === "master" ? "Run total" : "Tokens"}
          />
        </div>
      )}

      {copyError && (
        <div
          role="alert"
          className="shrink-0 border-b border-red-200 bg-red-50 px-4 py-1.5 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-300"
        >
          {copyError}
        </div>
      )}

      {open && (
        <div
          ref={scrollRef}
          onScroll={onScroll}
          className="relative min-h-0 flex-1 overflow-auto px-3 py-2"
        >
          {activeRequest && (
            <RequestDetails
              key={String(activeSession)}
              request={activeRequest}
              usage={activeUsage}
              copyText={copyText}
            />
          )}
          {visibleLogs.length > renderedLogs.length && (
            <div
              role="status"
              className="mb-1 font-mono text-xs text-gray-400 dark:text-gray-600"
            >
              Showing {renderedLogs.length} of {visibleLogs.length} matching
              lines. Copy includes all.
            </div>
          )}
          <pre className="font-mono text-xs leading-relaxed whitespace-pre-wrap">
            {renderedLogs.map((entry, i) => (
              <span
                key={windowStart + i}
                data-error={isError(entry) ? "1" : undefined}
                className={`group flex gap-2 ${logLineClass(entry)}`}
              >
                {showTimestamps && (
                  <span className="shrink-0 select-none text-gray-400 dark:text-gray-600">
                    {fmtClock(entry.t)}
                  </span>
                )}
                <span className="flex-1 min-w-0">{entry.line}</span>
                <button
                  onClick={() => void copyText(entry.line)}
                  className="shrink-0 select-none text-gray-400 opacity-0 transition-opacity hover:text-gray-900 focus:opacity-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-gray-400 group-hover:opacity-100 dark:text-gray-500 dark:hover:text-gray-200"
                  title="Copy this line"
                >
                  ⧉
                </button>
              </span>
            ))}
            <span ref={endRef} className="block" />
          </pre>
          {!follow && (
            <button
              onClick={resumeFollowing}
              className="sticky bottom-2 left-1/2 float-right mr-2 -translate-x-1/2 rounded-full border border-gray-300 bg-white px-3 py-1 text-xs text-gray-700 shadow-lg transition-colors hover:bg-gray-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100 dark:hover:bg-gray-700"
              title="Follow new log messages"
            >
              ↓ Follow
              {laterLines > 0 &&
                ` · ${laterLines} more ${laterLines === 1 ? "line" : "lines"}`}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

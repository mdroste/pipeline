import { useState, useMemo, useRef, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { LlmRequestDetails, LogEntry, UsageState } from "../hooks/usePipeline";
import type { ToolCallCounts } from "../lib/types";

interface Props {
  logs: LogEntry[];
  usage: UsageState;
}

type LevelFilter = "all" | "warn" | "error";

const DEFAULT_CONSOLE_HEIGHT = 192;
const MIN_CONSOLE_HEIGHT = 96;
const CONSOLE_VIEWPORT_MARGIN = 96;

function maxConsoleHeight(): number {
  return Math.max(MIN_CONSOLE_HEIGHT, window.innerHeight - CONSOLE_VIEWPORT_MARGIN);
}

function clampConsoleHeight(height: number): number {
  return Math.min(maxConsoleHeight(), Math.max(MIN_CONSOLE_HEIGHT, height));
}

// Compact token count: 1_234_567 → "1.2M", 45_678 → "45.7k", 832 → "832".
function fmtTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1).replace(/\.0$/, "") + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1).replace(/\.0$/, "") + "k";
  return String(n);
}

function isError(entry: LogEntry): boolean {
  return entry.level === "error" || entry.line.startsWith("ERROR");
}

function isWarn(entry: LogEntry): boolean {
  return entry.level === "warn" || entry.line.startsWith("WARNING");
}

// Console line color: trust the backend `level` when meaningful, fall back to
// prefix-sniffing for orchestration lines that carry no level.
function logLineClass(entry: LogEntry): string {
  if (isError(entry)) return "text-red-600 dark:text-red-400";
  if (isWarn(entry)) return "text-amber-600 dark:text-yellow-400";
  if (entry.level === "stderr" || entry.line.startsWith("[stderr]"))
    return "text-orange-600 dark:text-orange-400";
  if (entry.line.startsWith("Still waiting")) return "text-amber-700 dark:text-yellow-500";
  if (entry.level === "stdout" || entry.line.startsWith("$") || entry.line.startsWith("Wrote"))
    return "text-gray-600 dark:text-gray-400";
  return "text-gray-700 dark:text-gray-300";
}

function fmtClock(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function fmtCount(n: number): string {
  return n.toLocaleString();
}

function freshInputTokens(usage: UsageState["total"]): number {
  return Math.max(0, usage.input - usage.cached - usage.cacheWrite);
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
    counts.text_file > 0 ? `${fmtCount(counts.text_file)} text/file` : "",
    counts.image > 0 ? `${fmtCount(counts.image)} image` : "",
    counts.web > 0 ? `${fmtCount(counts.web)} web` : "",
    counts.shell_or_other > 0
      ? `${fmtCount(counts.shell_or_other)} shell/other`
      : "",
    counts.unknown > 0 ? `${fmtCount(counts.unknown)} unknown` : "",
  ]
    .filter(Boolean)
    .join(", ");
}

function reportedActivity(usage: UsageState["total"]): string {
  const tools = toolCallTotal(usage.toolCalls);
  return [
    usage.modelRoundTrips > 0
      ? `${fmtCount(usage.modelRoundTrips)} reported model round trips`
      : "",
    tools > 0
      ? `${fmtCount(tools)} reported tool calls (${toolCallBreakdown(usage.toolCalls)})`
      : "",
  ]
    .filter(Boolean)
    .join(" and ");
}

function hasReportedUsage(usage: UsageState["total"]): boolean {
  return (
    usage.input + usage.output > 0 ||
    usage.modelRoundTrips > 0 ||
    toolCallTotal(usage.toolCalls) > 0
  );
}

function usageDescription(usage: UsageState["total"]): string {
  const fresh = freshInputTokens(usage);
  const activity = reportedActivity(usage);
  return [
    `Token usage: ${fmtCount(usage.input)} logical input tokens equals ${fmtCount(fresh)} fresh input tokens plus ${fmtCount(usage.cached)} cache-read tokens plus ${fmtCount(usage.cacheWrite)} cache-write tokens; ${fmtCount(usage.output)} output tokens.`,
    activity ? `Model activity: ${activity}.` : "",
    "Cache reads and cache writes are subsets of logical input, not additional tokens.",
    "Fresh input equals logical input minus cache reads minus cache writes.",
    "Model round trips and tool calls are shown only when the provider or CLI reports them; unknown tool kinds remain in the unknown bucket.",
    "For an API-equivalent dollar estimate, price fresh input, cache reads, cache writes, and output at their separate list rates; cache reads are discounted, not free.",
    "The completed report's Run summary calculates this estimate for recognized models.",
    "Only providers that report usage are included.",
  ]
    .filter(Boolean)
    .join(" ");
}

function requestText(request: LlmRequestDetails): string {
  const parts: string[] = [];
  if (request.system_prompt) {
    parts.push(
      `SYSTEM PROMPT${request.system_prompt_truncated ? " (PREVIEW TRUNCATED)" : ""}` +
      `\n\n${request.system_prompt}`,
    );
  }
  if (request.shared_context) {
    parts.push(
      `SHARED CONTEXT${request.shared_context_truncated ? " (PREVIEW TRUNCATED)" : ""}` +
      `\n\n${request.shared_context}`,
    );
  }
  parts.push(
    `TASK PROMPT${request.prompt_truncated ? " (PREVIEW TRUNCATED)" : ""}` +
    `\n\n${request.prompt}`,
  );
  return parts.join("\n\n" + "=".repeat(72) + "\n\n");
}

function TruncatedPreviewBadge({ truncated }: { truncated?: boolean }) {
  if (!truncated) return null;
  return (
    <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-amber-700 dark:bg-amber-900/50 dark:text-amber-300">
      Preview truncated
    </span>
  );
}

function RequestDetails({
  request,
  copyText,
}: {
  request: LlmRequestDetails;
  copyText: (text: string) => Promise<void>;
}) {
  const [showPrompt, setShowPrompt] = useState(false);
  const toolText = request.tools.length > 0 ? request.tools.join(", ") : "None";
  const transport = request.transport === "api" ? "Direct API" : "CLI";
  const outputLimit =
    request.max_output_tokens == null
      ? "provider default"
      : `${fmtCount(request.max_output_tokens)} max output tokens`;
  const access = [
    request.pdf_attached ? "PDF attached" : null,
    request.shared_context ? `shared context (${fmtCount(request.shared_context_chars)} chars)` : null,
    request.write_enabled ? "artifact writes enabled" : "read-only",
  ]
    .filter(Boolean)
    .join(" · ");
  const hasTruncatedPreview =
    request.prompt_truncated ||
    request.system_prompt_truncated ||
    request.shared_context_truncated;

  return (
    <div className="mb-2 rounded-lg border border-gray-200 bg-gray-50 text-gray-700 shadow-sm dark:border-gray-800 dark:bg-gray-900/70 dark:text-gray-300 dark:shadow-none">
      <div className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 px-3 py-2 text-xs">
        <span className="text-gray-500 dark:text-gray-400">Provider</span>
        <span>
          {request.provider_label} ({request.provider}) · {transport}
          {request.local_endpoint ? ` · ${request.local_endpoint}` : ""}
        </span>
        <span className="text-gray-500 dark:text-gray-400">Model</span>
        <span>
          {request.model}{" "}
          <span className="text-gray-500 dark:text-gray-400">({request.model_policy})</span>
        </span>
        <span className="text-gray-500 dark:text-gray-400">Settings</span>
        <span>
          effort {request.effort} · tools {toolText} · timeout {fmtCount(request.timeout_secs)}s ·{" "}
          {outputLimit}
        </span>
        <span className="text-gray-500 dark:text-gray-400">Context</span>
        <span>{access}</span>
      </div>

      <div className="border-t border-gray-200 dark:border-gray-800">
        <button
          onClick={() => setShowPrompt((value) => !value)}
          className="flex w-full cursor-pointer select-none items-center gap-2 px-3 py-1.5 text-left
                     text-gray-600 transition-colors hover:bg-gray-100 hover:text-gray-900
                     focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-gray-400
                     dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-200"
        >
          <span>
            {showPrompt ? "Hide" : "View"} prompt ({fmtCount(request.prompt_chars)} task characters)
          </span>
          {!showPrompt && <TruncatedPreviewBadge truncated={Boolean(hasTruncatedPreview)} />}
        </button>
        {showPrompt && (
          <div className="border-t border-gray-200 px-3 py-2 dark:border-gray-800">
            <div className="mb-2 flex items-center justify-between">
              <span className="text-gray-500 dark:text-gray-400">
                System, shared context, and task prompt are shown separately in dispatch order.
                {hasTruncatedPreview &&
                  " Long fields show bounded previews; Copy prompt copies only the visible preview."}
              </span>
              <button
                onClick={() => void copyText(requestText(request))}
                className="rounded border border-gray-300 bg-white px-1.5 py-0.5 text-gray-600 transition-colors hover:bg-gray-100 hover:text-gray-900 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-gray-400 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-gray-100"
              >
                Copy prompt
              </button>
            </div>
            {request.system_prompt && (
              <section className="mb-3">
                <div className="mb-1 flex items-center gap-2 text-gray-500 dark:text-gray-400">
                  <span>
                    System prompt
                    {request.system_prompt_chars !== undefined
                      ? ` (${fmtCount(request.system_prompt_chars)} characters)`
                      : ""}
                  </span>
                  <TruncatedPreviewBadge truncated={request.system_prompt_truncated} />
                </div>
                <pre className="whitespace-pre-wrap text-gray-700 dark:text-gray-300">{request.system_prompt}</pre>
              </section>
            )}
            {request.shared_context && (
              <section className="mb-3">
                <div className="mb-1 flex items-center gap-2 text-gray-500 dark:text-gray-400">
                  <span>
                    Shared context ({fmtCount(request.shared_context_chars)} characters)
                  </span>
                  <TruncatedPreviewBadge truncated={request.shared_context_truncated} />
                </div>
                <pre className="whitespace-pre-wrap text-gray-700 dark:text-gray-300">{request.shared_context}</pre>
              </section>
            )}
            <section>
              <div className="mb-1 flex items-center gap-2 text-gray-500 dark:text-gray-400">
                <span>Task prompt ({fmtCount(request.prompt_chars)} characters)</span>
                <TruncatedPreviewBadge truncated={request.prompt_truncated} />
              </div>
              <pre className="whitespace-pre-wrap text-gray-700 dark:text-gray-300">{request.prompt}</pre>
            </section>
          </div>
        )}
      </div>

      {(request.working_directory ||
        request.read_directories.length > 0 ||
        request.write_directory) && (
        <details className="border-t border-gray-200 dark:border-gray-800">
          <summary className="cursor-pointer select-none px-3 py-1.5 text-gray-600 transition-colors hover:bg-gray-100 hover:text-gray-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-gray-400 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-200">
            View execution paths
          </summary>
          <div className="border-t border-gray-200 px-3 py-2 text-gray-600 dark:border-gray-800 dark:text-gray-400">
            {request.working_directory && (
              <div>
                <span className="text-gray-500 dark:text-gray-400">Working directory: </span>
                {request.working_directory}
              </div>
            )}
            {request.read_directories.map((path) => (
              <div key={path}>
                <span className="text-gray-500 dark:text-gray-400">Read root: </span>
                {path}
              </div>
            ))}
            {request.write_directory && (
              <div>
                <span className="text-gray-500 dark:text-gray-400">Write root: </span>
                {request.write_directory}
              </div>
            )}
          </div>
        </details>
      )}
    </div>
  );
}

/** The pipeline console: session filter, text search, level filter, timestamps,
 *  per-line copy, jump-to-error, auto-scroll that pauses when scrolled up, and
 *  copy/save of the visible lines. */
export default function Console({ logs, usage }: Props) {
  const [open, setOpen] = useState(true);
  const [height, setHeight] = useState(DEFAULT_CONSOLE_HEIGHT);
  const [selectedSession, setSelectedSession] = useState<number | "master">("master");
  const [search, setSearch] = useState("");
  const [level, setLevel] = useState<LevelFilter>("all");
  const [showTimestamps, setShowTimestamps] = useState(false);
  const [copied, setCopied] = useState(false);
  const [saved, setSaved] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  // Auto-follow the tail unless the user scrolls up.
  const [follow, setFollow] = useState(true);

  const scrollRef = useRef<HTMLDivElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const resizeCleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => () => resizeCleanupRef.current?.(), []);

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
      const measuredHeight = consoleElement?.getBoundingClientRect().height ?? 0;
      const startHeight = measuredHeight > 0 ? measuredHeight : height;
      const startY = event.clientY;

      const onMouseMove = (moveEvent: MouseEvent) => {
        setHeight(clampConsoleHeight(startHeight + startY - moveEvent.clientY));
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

  const resizeWithKeyboard = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    const delta = event.shiftKey ? 48 : 16;
    let next: number | null = null;
    if (event.key === "ArrowUp") next = height + delta;
    if (event.key === "ArrowDown") next = height - delta;
    if (event.key === "Home") next = MIN_CONSOLE_HEIGHT;
    if (event.key === "End") next = maxConsoleHeight();
    if (next == null) return;
    event.preventDefault();
    setHeight(clampConsoleHeight(next));
  }, [height]);

  // Group console lines by headless session for the per-session selector.
  const sessions = useMemo(() => {
    const map = new Map<
      number,
      {
        id: number;
        label: string;
        count: number;
        hasError: boolean;
        request?: LlmRequestDetails;
      }
    >();
    for (const e of logs) {
      if (e.session == null) continue;
      let s = map.get(e.session);
      if (!s) {
        s = { id: e.session, label: e.label || `Session ${e.session}`, count: 0, hasError: false };
        map.set(e.session, s);
      }
      s.count++;
      if (s.label.startsWith("Session ") && e.label) s.label = e.label;
      if (isError(e)) s.hasError = true;
      if (e.request) s.request = e.request;
    }
    return Array.from(map.values()).sort((a, b) => a.id - b.id);
  }, [logs]);

  // A stale selection (previous run's session id) falls back to the master view.
  const activeSession =
    selectedSession !== "master" && !sessions.some((s) => s.id === selectedSession)
      ? "master"
      : selectedSession;
  const activeRequest =
    activeSession === "master"
      ? undefined
      : sessions.find((session) => session.id === activeSession)?.request;

  const needle = search.trim().toLowerCase();
  const visibleLogs = useMemo(() => {
    return logs.filter((e) => {
      if (activeSession !== "master" && e.session !== activeSession) return false;
      if (level === "error" && !isError(e)) return false;
      if (level === "warn" && !isError(e) && !isWarn(e)) return false;
      if (needle && !e.line.toLowerCase().includes(needle)) return false;
      return true;
    });
  }, [logs, activeSession, level, needle]);

  const errorCount = useMemo(() => logs.filter(isError).length, [logs]);

  // Auto-scroll to the tail when following; disabled once the user scrolls up.
  useEffect(() => {
    if (open && follow) endRef.current?.scrollIntoView?.({ behavior: "smooth" });
  }, [visibleLogs, open, follow]);

  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
    setFollow(atBottom);
  }, []);

  const jumpToFirstError = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    const first = el.querySelector<HTMLElement>("[data-error='1']");
    if (first) {
      setFollow(false);
      first.scrollIntoView?.({ behavior: "smooth", block: "center" });
    }
  }, []);

  const visibleText = useCallback(
    () =>
      visibleLogs
        .map((e) => (showTimestamps ? `[${fmtClock(e.t)}] ${e.line}` : e.line))
        .join("\n"),
    [visibleLogs, showTimestamps]
  );

  const copyText = useCallback(
    async (text: string) => {
      try {
        await navigator.clipboard.writeText(text);
      } catch {
        // Fallback for webviews without async clipboard access.
        const ta = document.createElement("textarea");
        ta.value = text;
        ta.style.position = "fixed";
        ta.style.opacity = "0";
        document.body.appendChild(ta);
        ta.select();
        try {
          document.execCommand("copy");
        } catch {
          /* give up silently */
        }
        document.body.removeChild(ta);
      }
    },
    []
  );

  const copyAll = useCallback(async () => {
    await copyText(visibleText());
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [copyText, visibleText]);

  const saveToFile = useCallback(async () => {
    setSaveError(null);
    try {
      const path = await save({
        defaultPath: "pipeline-console.log",
        filters: [{ name: "Log", extensions: ["log", "txt"] }],
      });
      if (!path) return;
      await invoke("save_text_file", { path, content: visibleText() });
      setSaved(true);
      setTimeout(() => setSaved(false), 1500);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setSaveError(`Could not save the console log: ${message}`);
    }
  }, [visibleText]);

  return (
    <div
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
          aria-valuemax={maxConsoleHeight()}
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
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-x-3 gap-y-1 border-b border-gray-200 bg-gray-50 px-4 py-1.5 font-mono text-xs text-gray-600 dark:border-gray-800 dark:bg-gray-900 dark:text-gray-400">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
          <button
            onClick={() => setOpen(!open)}
            className="flex shrink-0 cursor-pointer select-none items-center gap-1.5 rounded-sm transition-colors hover:text-gray-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:hover:text-gray-200"
          >
            <span>{open ? "▼" : "▲"}</span>
            <span>Console</span>
          </button>
          {sessions.length > 0 && (
            <select
              value={activeSession === "master" ? "master" : String(activeSession)}
              onChange={(e) => {
                const next =
                  e.target.value === "master" ? "master" : Number(e.target.value);
                setSelectedSession(next);
                if (next !== "master") {
                  setFollow(false);
                  scrollRef.current?.scrollTo?.({ top: 0 });
                }
              }}
              className="min-w-40 max-w-[22rem] flex-1 cursor-pointer rounded border border-gray-300 bg-white px-1.5 py-0.5 text-xs text-gray-700 focus:outline-none focus:ring-1 focus:ring-gray-400 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-300"
              title="Show a single headless session's log, or all of them"
            >
              <option value="master">All sessions</option>
              {sessions.map((s) => {
                const u = usage.bySession[s.id];
                const cacheRead = u?.cached ? ` + ${fmtTokens(u.cached)} cache read` : "";
                const cacheWrite = u?.cacheWrite
                  ? ` + ${fmtTokens(u.cacheWrite)} cache write`
                  : "";
                const tok = u
                  ? ` · ${fmtTokens(u.input)} logical input = ${fmtTokens(freshInputTokens(u))} fresh${cacheRead}${cacheWrite} → ${fmtTokens(u.output)} output`
                  : "";
                const activity = u && reportedActivity(u)
                  ? ` · ${reportedActivity(u)}`
                  : "";
                const request = s.request
                  ? ` · ${s.request.provider_label} ${s.request.transport.toUpperCase()} · ${s.request.model}`
                  : "";
                return (
                  <option key={s.id} value={String(s.id)}>
                    {(s.hasError ? "✕ " : "") +
                      s.label +
                      request +
                      ` (${s.count})` +
                      tok +
                      activity}
                  </option>
                );
              })}
            </select>
          )}
          {open && (
            <>
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search…"
                className="w-28 rounded border border-gray-300 bg-white px-1.5 py-0.5 text-xs text-gray-700 transition-all placeholder:text-gray-400 focus:w-40 focus:outline-none focus:ring-1 focus:ring-gray-400 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-300 dark:placeholder:text-gray-600 dark:focus:ring-gray-500"
              />
              {/* Level filter chips */}
              <div className="flex items-center gap-0.5 shrink-0">
                {(["all", "warn", "error"] as LevelFilter[]).map((lv) => (
                  <button
                    key={lv}
                    onClick={() => setLevel(lv)}
                    className={`px-1.5 py-0.5 rounded text-xs transition-colors ${
                      level === lv
                        ? "bg-gray-700 text-white dark:bg-gray-600 dark:text-gray-100"
                        : "text-gray-500 hover:bg-gray-200 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-200"
                    }`}
                    title={
                      lv === "all" ? "All lines" : lv === "warn" ? "Warnings + errors" : "Errors only"
                    }
                  >
                    {lv === "all" ? "All" : lv === "warn" ? "Warn" : "Err"}
                  </button>
                ))}
              </div>
            </>
          )}
        </div>

        <div className="ml-auto flex max-w-full flex-wrap items-center justify-end gap-2">
          {saveError && (
            <span
              role="alert"
              title={saveError}
              className="max-w-64 truncate text-red-600 dark:text-red-400"
            >
              {saveError}
            </span>
          )}
          {hasReportedUsage(usage.total) && (
            <span
              className="text-gray-500 dark:text-gray-400"
              aria-label={usageDescription(usage.total)}
              title={usageDescription(usage.total)}
              tabIndex={0}
            >
              {fmtTokens(usage.total.input)} logical input ={" "}
              <span className="text-gray-600 dark:text-gray-300">
                {fmtTokens(freshInputTokens(usage.total))} fresh
              </span>
              {usage.total.cached > 0 && (
                <span className="text-emerald-600 dark:text-green-500">
                  {" "}
                  + {fmtTokens(usage.total.cached)} cache read
                </span>
              )}
              {usage.total.cacheWrite > 0 && (
                <span className="text-blue-600 dark:text-blue-400">
                  {" "}
                  + {fmtTokens(usage.total.cacheWrite)} cache write
                </span>
              )}
              {" · "}
              {fmtTokens(usage.total.output)} output
              {reportedActivity(usage.total) && (
                <span className="text-violet-600 dark:text-violet-400">
                  {" · "}
                  {reportedActivity(usage.total)}
                </span>
              )}
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
              className={`cursor-pointer rounded border border-gray-300 px-1.5 py-0.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-gray-400 dark:border-gray-700 ${
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
            className="cursor-pointer rounded border border-gray-300 bg-white px-1.5 py-0.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 hover:text-gray-900 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-gray-400 disabled:cursor-default disabled:opacity-40 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            title={activeSession === "master" ? "Copy all shown lines" : "Copy this session's lines"}
          >
            {copied ? "Copied ✓" : "Copy"}
          </button>
          <button
            onClick={saveToFile}
            disabled={visibleLogs.length === 0}
            className="cursor-pointer rounded border border-gray-300 bg-white px-1.5 py-0.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 hover:text-gray-900 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-gray-400 disabled:cursor-default disabled:opacity-40 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            title="Save shown lines to a file"
          >
            {saved ? "Saved ✓" : "Save"}
          </button>
        </div>
      </div>

      {open && (
        <div ref={scrollRef} onScroll={onScroll} className="flex-1 overflow-auto px-4 py-2 min-h-0 relative">
          {activeRequest && (
            <RequestDetails
              key={String(activeSession)}
              request={activeRequest}
              copyText={copyText}
            />
          )}
          <pre className="font-mono text-xs leading-relaxed whitespace-pre-wrap">
            {visibleLogs.map((entry, i) => (
              <div
                key={i}
                data-error={isError(entry) ? "1" : undefined}
                className={`group flex gap-2 ${logLineClass(entry)}`}
              >
                {showTimestamps && (
                  <span className="shrink-0 select-none text-gray-400 dark:text-gray-600">{fmtClock(entry.t)}</span>
                )}
                <span className="flex-1 min-w-0">{entry.line}</span>
                <button
                  onClick={() => copyText(entry.line)}
                  className="shrink-0 select-none text-gray-400 opacity-0 transition-opacity hover:text-gray-900 focus:opacity-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-gray-400 group-hover:opacity-100 dark:text-gray-500 dark:hover:text-gray-200"
                  title="Copy this line"
                >
                  ⧉
                </button>
              </div>
            ))}
            <div ref={endRef} />
          </pre>
          {!follow && (
            <button
              onClick={() => setFollow(true)}
              className="sticky bottom-2 left-1/2 float-right mr-2 -translate-x-1/2 rounded-full border border-gray-300 bg-white px-3 py-1 text-xs text-gray-700 shadow-lg transition-colors hover:bg-gray-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100 dark:hover:bg-gray-700"
              title="Resume following the log tail"
            >
              ↓ Follow
            </button>
          )}
        </div>
      )}
    </div>
  );
}

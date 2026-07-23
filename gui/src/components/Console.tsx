import { useState, useMemo, useRef, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { LogEntry, UsageState } from "../hooks/usePipeline";

interface Props {
  logs: LogEntry[];
  usage: UsageState;
}

type LevelFilter = "all" | "warn" | "error";

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
  if (isError(entry)) return "text-red-400";
  if (isWarn(entry)) return "text-yellow-500";
  if (entry.level === "stderr" || entry.line.startsWith("[stderr]")) return "text-orange-400";
  if (entry.line.startsWith("Still waiting")) return "text-yellow-600";
  if (entry.level === "stdout" || entry.line.startsWith("$") || entry.line.startsWith("Wrote"))
    return "text-gray-500";
  return "text-gray-400";
}

function fmtClock(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** The pipeline console: session filter, text search, level filter, timestamps,
 *  per-line copy, jump-to-error, auto-scroll that pauses when scrolled up, and
 *  copy/save of the visible lines. */
export default function Console({ logs, usage }: Props) {
  const [open, setOpen] = useState(true);
  const [selectedSession, setSelectedSession] = useState<number | "master">("master");
  const [search, setSearch] = useState("");
  const [level, setLevel] = useState<LevelFilter>("all");
  const [showTimestamps, setShowTimestamps] = useState(false);
  const [copied, setCopied] = useState(false);
  const [saved, setSaved] = useState(false);
  // Auto-follow the tail unless the user scrolls up.
  const [follow, setFollow] = useState(true);

  const scrollRef = useRef<HTMLDivElement>(null);
  const endRef = useRef<HTMLDivElement>(null);

  // Group console lines by headless session for the per-session selector.
  const sessions = useMemo(() => {
    const map = new Map<number, { id: number; label: string; count: number; hasError: boolean }>();
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
    }
    return Array.from(map.values()).sort((a, b) => a.id - b.id);
  }, [logs]);

  // A stale selection (previous run's session id) falls back to the master view.
  const activeSession =
    selectedSession !== "master" && !sessions.some((s) => s.id === selectedSession)
      ? "master"
      : selectedSession;

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
    try {
      const path = await save({
        defaultPath: "pipeline-console.log",
        filters: [{ name: "Log", extensions: ["log", "txt"] }],
      });
      if (!path) return;
      await invoke("save_text_file", { path, content: visibleText() });
      setSaved(true);
      setTimeout(() => setSaved(false), 1500);
    } catch {
      /* dialog cancelled or write failed; ignore */
    }
  }, [visibleText]);

  return (
    <div
      className="border-t border-gray-300 dark:border-gray-700 bg-gray-900 flex flex-col"
      style={{ height: open ? "12rem" : undefined }}
    >
      <div className="flex items-center justify-between px-4 py-1.5 bg-gray-800 text-gray-400 text-xs font-mono shrink-0 gap-3">
        <div className="flex items-center gap-2 min-w-0">
          <button
            onClick={() => setOpen(!open)}
            className="flex items-center gap-1.5 hover:text-gray-200 transition-colors cursor-pointer select-none shrink-0"
          >
            <span>{open ? "▼" : "▲"}</span>
            <span>Console</span>
          </button>
          {sessions.length > 0 && (
            <select
              value={activeSession === "master" ? "master" : String(activeSession)}
              onChange={(e) =>
                setSelectedSession(e.target.value === "master" ? "master" : Number(e.target.value))
              }
              className="bg-gray-900 border border-gray-700 rounded px-1.5 py-0.5 text-xs text-gray-300 max-w-[14rem] cursor-pointer"
              title="Show a single headless session's log, or all of them"
            >
              <option value="master">All sessions</option>
              {sessions.map((s) => {
                const u = usage.bySession[s.id];
                const cache = u?.cached ? ` · ${fmtTokens(u.cached)} cached` : "";
                const tok = u ? ` · ${fmtTokens(u.input)}→${fmtTokens(u.output)}${cache}` : "";
                return (
                  <option key={s.id} value={String(s.id)}>
                    {(s.hasError ? "✕ " : "") + s.label + ` (${s.count})` + tok}
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
                className="bg-gray-900 border border-gray-700 rounded px-1.5 py-0.5 text-xs text-gray-300 w-28 focus:w-40 transition-all focus:outline-none focus:ring-1 focus:ring-gray-500"
              />
              {/* Level filter chips */}
              <div className="flex items-center gap-0.5 shrink-0">
                {(["all", "warn", "error"] as LevelFilter[]).map((lv) => (
                  <button
                    key={lv}
                    onClick={() => setLevel(lv)}
                    className={`px-1.5 py-0.5 rounded text-xs transition-colors ${
                      level === lv
                        ? "bg-gray-600 text-gray-100"
                        : "text-gray-400 hover:text-gray-200"
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

        <div className="flex items-center gap-2 shrink-0">
          {usage.total.input + usage.total.output > 0 && (
            <span
              className="text-gray-500"
              title={[
                `${usage.total.input.toLocaleString()} logical input`,
                `${usage.total.output.toLocaleString()} output`,
                `${usage.total.cached.toLocaleString()} cache-read`,
                `${usage.total.cacheWrite.toLocaleString()} cache-write tokens`,
                "(providers that report usage)",
              ].join(" · ")}
            >
              {fmtTokens(usage.total.input)} in / {fmtTokens(usage.total.output)} out
              {usage.total.cached > 0 && (
                <span className="text-green-500"> · {fmtTokens(usage.total.cached)} cached</span>
              )}
              {usage.total.cacheWrite > 0 && (
                <span className="text-blue-400"> · {fmtTokens(usage.total.cacheWrite)} warmed</span>
              )}
            </span>
          )}
          {errorCount > 0 && (
            <button
              onClick={jumpToFirstError}
              className="text-red-400 hover:text-red-300 transition-colors cursor-pointer"
              title="Scroll to the first error"
            >
              {errorCount} {errorCount === 1 ? "error" : "errors"}
            </button>
          )}
          <span>{visibleLogs.length} lines</span>
          {open && (
            <button
              onClick={() => setShowTimestamps((v) => !v)}
              className={`border border-gray-700 rounded px-1.5 py-0.5 text-xs transition-colors cursor-pointer ${
                showTimestamps ? "bg-gray-600 text-gray-100" : "text-gray-300 hover:bg-gray-700"
              }`}
              title="Toggle timestamps"
            >
              🕑
            </button>
          )}
          <button
            onClick={copyAll}
            disabled={visibleLogs.length === 0}
            className="border border-gray-700 rounded px-1.5 py-0.5 text-xs text-gray-300 hover:bg-gray-700 hover:text-gray-100 transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-default"
            title={activeSession === "master" ? "Copy all shown lines" : "Copy this session's lines"}
          >
            {copied ? "Copied ✓" : "Copy"}
          </button>
          <button
            onClick={saveToFile}
            disabled={visibleLogs.length === 0}
            className="border border-gray-700 rounded px-1.5 py-0.5 text-xs text-gray-300 hover:bg-gray-700 hover:text-gray-100 transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-default"
            title="Save shown lines to a file"
          >
            {saved ? "Saved ✓" : "Save"}
          </button>
        </div>
      </div>

      {open && (
        <div ref={scrollRef} onScroll={onScroll} className="flex-1 overflow-auto px-4 py-2 min-h-0 relative">
          <pre className="font-mono text-xs leading-relaxed whitespace-pre-wrap">
            {visibleLogs.map((entry, i) => (
              <div
                key={i}
                data-error={isError(entry) ? "1" : undefined}
                className={`group flex gap-2 ${logLineClass(entry)}`}
              >
                {showTimestamps && (
                  <span className="text-gray-600 shrink-0 select-none">{fmtClock(entry.t)}</span>
                )}
                <span className="flex-1 min-w-0">{entry.line}</span>
                <button
                  onClick={() => copyText(entry.line)}
                  className="opacity-0 group-hover:opacity-100 text-gray-500 hover:text-gray-200 transition-opacity shrink-0 select-none"
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
              className="sticky bottom-2 left-1/2 -translate-x-1/2 float-right mr-2 bg-gray-700 hover:bg-gray-600 text-gray-100 text-xs rounded-full px-3 py-1 shadow-lg transition-colors"
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

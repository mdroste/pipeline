import type { RunSummary } from "../../lib/reviewTypes";

export function fmtBytes(n: number): string {
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(1) + " GB";
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + " MB";
  if (n >= 1_000) return (n / 1_000).toFixed(0) + " KB";
  return n + " B";
}

export function fmtTokens(n: number): string {
  if (n >= 1_000_000)
    return (n / 1_000_000).toFixed(1).replace(/\.0$/, "") + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1).replace(/\.0$/, "") + "k";
  return String(n);
}

export function freshInputTokens(run: RunSummary): number {
  return Math.max(
    0,
    run.input_tokens -
      (run.cached_input_tokens ?? 0) -
      (run.cache_write_input_tokens ?? 0),
  );
}

export function hasReportedUsage(run: RunSummary): boolean {
  return run.input_tokens + run.output_tokens > 0;
}

export function usageDescription(run: RunSummary): string {
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

export function fmtDuration(secs: number): string {
  if (secs <= 0) return "—";
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

export function fmtDate(iso: string): string {
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

export const isRunning = (run: RunSummary) => run.status === "running";
export const RUNNING_TITLE =
  "This review is still running. Its report opens here when it finishes.";

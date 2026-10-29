import type {
  LlmRequestDetails,
  LogEntry,
  TokenTotals,
} from "./reviewRuntimeTypes";

// Compact token count: 1_234_567 → "1.2M", 45_678 → "45.7k", 832 → "832".
export function fmtTokens(n: number): string {
  if (n >= 1_000_000)
    return (n / 1_000_000).toFixed(1).replace(/\.0$/, "") + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1).replace(/\.0$/, "") + "k";
  return String(n);
}

export function isError(entry: LogEntry): boolean {
  return entry.level === "error" || entry.line.startsWith("ERROR");
}

export function isWarn(entry: LogEntry): boolean {
  return entry.level === "warn" || entry.line.startsWith("WARNING");
}

// Console line color: trust the backend `level` when meaningful, fall back to
// prefix-sniffing for orchestration lines that carry no level.
export function logLineClass(entry: LogEntry): string {
  if (isError(entry)) return "text-red-600 dark:text-red-400";
  if (isWarn(entry)) return "text-amber-600 dark:text-yellow-400";
  if (entry.level === "stderr" || entry.line.startsWith("[stderr]"))
    return "text-orange-600 dark:text-orange-400";
  if (entry.line.startsWith("Still waiting"))
    return "text-amber-700 dark:text-yellow-500";
  if (
    entry.level === "stdout" ||
    entry.line.startsWith("$") ||
    entry.line.startsWith("Wrote")
  )
    return "text-gray-600 dark:text-gray-400";
  return "text-gray-700 dark:text-gray-300";
}

export function fmtClock(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

export function fmtCount(n: number): string {
  return n.toLocaleString();
}

export function freshInputTokens(usage: TokenTotals): number {
  return Math.max(0, usage.input - usage.cached - usage.cacheWrite);
}

export function hasTokenUsage(usage: TokenTotals): boolean {
  return usage.input + usage.output > 0;
}

export function usageDescription(usage: TokenTotals): string {
  const fresh = freshInputTokens(usage);
  return [
    `Token usage: ${fmtCount(usage.input)} logical input tokens equals ${fmtCount(fresh)} fresh input tokens plus ${fmtCount(usage.cached)} cache-read tokens plus ${fmtCount(usage.cacheWrite)} cache-write tokens; ${fmtCount(usage.output)} output tokens.`,
    "Cache reads and cache writes are subsets of logical input, not additional tokens.",
    "Fresh input equals logical input minus cache reads minus cache writes.",
    "For an API-equivalent dollar estimate, price fresh input, cache reads, cache writes, and output at their separate list rates; cache reads are discounted, not free.",
    "The completed report's summary calculates this estimate for recognized models.",
    "Only providers that report usage are included.",
  ]
    .filter(Boolean)
    .join(" ");
}

export function fmtDuration(secs: number): string {
  if (secs >= 3600 && secs % 3600 === 0) return `${secs / 3600} h`;
  if (secs >= 60 && secs % 60 === 0) return `${secs / 60} min`;
  return `${fmtCount(secs)} s`;
}

export function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

// Backend session labels read "Step: Technical [Section 2 · Codex]" or
// "Merge: Technical". The picker drops the "Step:" boilerplate, flattens the
// bracketed unit, and names the agent only when the label does not already.
export function sessionName(
  label: string,
  request?: LlmRequestDetails,
): string {
  const merge = /^Merge:\s*/.test(label);
  let name = label
    .replace(/^(Step|Merge):\s*/, "")
    .replace(/\s*\[([^\]]+)\]/g, " · $1");
  if (merge) name = `Merge · ${name}`;
  if (request) {
    const lower = name.toLowerCase();
    const named = [request.provider, request.provider_label].some((value) =>
      lower.includes(value.toLowerCase()),
    );
    if (!named) name += ` · ${capitalize(request.provider)}`;
  }
  return name;
}

export function requestText(request: LlmRequestDetails): string {
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

export function summarizeLogs(logs: LogEntry[]) {
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
  let errors = 0;
  for (const e of logs) {
    if (isError(e)) errors++;
    if (e.session == null) continue;
    let s = map.get(e.session);
    if (!s) {
      s = {
        id: e.session,
        label: e.label || `Session ${e.session}`,
        count: 0,
        hasError: false,
      };
      map.set(e.session, s);
    }
    s.count++;
    if (s.label.startsWith("Session ") && e.label) s.label = e.label;
    if (isError(e)) s.hasError = true;
    if (e.request) s.request = e.request;
  }
  return {
    sessions: Array.from(map.values()).sort((a, b) => a.id - b.id),
    errorCount: errors,
  };
}

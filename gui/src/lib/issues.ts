import type { PipelineReport } from "./types";

export interface Issue {
  id: string;
  title: string;
  /** "high" | "medium" | "low" | "" (unknown). */
  severity: string;
  section: string;
  body: string;
}

/** Find one balanced JSON object/array candidate at `start`, respecting mixed
 * nesting and string quoting. */
function balancedSpanAt(text: string, start: number): string | null {
  const stack: string[] = [];
  let inStr = false;
  let escaped = false;
  for (let i = start; i < text.length; i++) {
    const c = text[i];
    if (inStr) {
      if (escaped) escaped = false;
      else if (c === "\\") escaped = true;
      else if (c === '"') inStr = false;
      continue;
    }
    if (c === '"') {
      inStr = true;
    } else if (c === "{" || c === "[") {
      stack.push(c === "{" ? "}" : "]");
    } else if (c === "}" || c === "]") {
      if (stack.pop() !== c) return null;
      if (stack.length === 0) return text.slice(start, i + 1);
    }
  }
  return null;
}

/** Pull a JSON value out of a model reply: whole string, any fenced block, or
 * any balanced object/array candidate. Returns null if nothing parses. */
export function extractJson(text: string): unknown | null {
  const trimmed = text.trim();
  const tryParse = (s: string): unknown | undefined => {
    try {
      return JSON.parse(s);
    } catch {
      return undefined;
    }
  };
  let v = tryParse(trimmed);
  if (v !== undefined) return v;
  for (const fence of trimmed.matchAll(/```(?:[a-zA-Z0-9_-]*)?\s*\n?([\s\S]*?)```/g)) {
    v = tryParse(fence[1].trim());
    if (v !== undefined) return v;
  }
  for (let start = 0; start < trimmed.length; start++) {
    if (trimmed[start] !== "{" && trimmed[start] !== "[") continue;
    const span = balancedSpanAt(trimmed, start);
    if (span) {
      v = tryParse(span);
      if (v !== undefined) return v;
    }
  }
  return null;
}

/** Interpret `text` as a structured issue list, or null if it isn't one.
 *  Accepts `{ "issues": [...] }` or a bare array of issue-shaped objects. */
export function parseIssues(text: string): Issue[] | null {
  const json = extractJson(text);
  if (json == null) return null;
  let arr: unknown;
  if (Array.isArray(json)) arr = json;
  else if (typeof json === "object" && Array.isArray((json as Record<string, unknown>).issues))
    arr = (json as Record<string, unknown>).issues;
  else return null;
  if (!Array.isArray(arr) || arr.length === 0) return null;

  const issues: Issue[] = [];
  for (let i = 0; i < arr.length; i++) {
    const o = arr[i];
    if (!o || typeof o !== "object" || Array.isArray(o)) return null;
    const obj = o as Record<string, unknown>;
    const title = String(obj.title ?? obj.summary ?? "").trim();
    const body = String(obj.body ?? obj.description ?? obj.detail ?? "").trim();
    if (!title && !body) return null; // not issue-shaped
    issues.push({
      id: String(obj.id ?? i + 1),
      title: title || `Issue ${i + 1}`,
      severity: String(obj.severity ?? "").toLowerCase().trim(),
      section: String(obj.section ?? "").trim(),
      body,
    });
  }
  return issues;
}

/** Detect a structured issue list in a report's step outputs (the last step
 *  whose output parses as issues wins — usually the synthesis step). */
export function detectReportIssues(report: PipelineReport): Issue[] | null {
  const outputs = report.step_outputs ?? [];
  for (let i = outputs.length - 1; i >= 0; i--) {
    if (outputs[i].skipped) continue;
    const issues = parseIssues(outputs[i].raw_text);
    if (issues) return issues;
  }
  return null;
}

/** Sort rank for a severity string (lower = more severe / shown first). */
export function severityRank(sev: string): number {
  switch (sev) {
    case "high":
    case "major":
    case "critical":
      return 0;
    case "medium":
    case "moderate":
      return 1;
    case "low":
    case "minor":
      return 2;
    default:
      return 3;
  }
}

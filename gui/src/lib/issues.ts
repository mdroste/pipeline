import type { PipelineReport } from "./types";

export interface Issue {
  id: string;
  title: string;
  /** "high" | "medium" | "low" | "" (unknown). */
  severity: string;
  section: string;
  body: string;
}

/** Yield parseable JSON values in a model reply: whole string, fenced blocks,
 * then balanced object/array candidates.
 *
 * The structural scan is a single pass. Parsing is separately bounded by both
 * attempt count and total candidate bytes so malformed model output cannot
 * turn a report view into quadratic work. */
function jsonCandidates(text: string): unknown[] {
  const MAX_SCAN_CHARS = 1_000_000;
  const MAX_CANDIDATE_CHARS = 512_000;
  const MAX_CANDIDATES = 64;
  const MAX_PARSE_ATTEMPTS = 128;
  const MAX_PARSE_CHARS = 2_000_000;
  const MAX_RECOVERY_SPANS = 128;
  const MAX_NESTING_FRAMES = 256;
  const trimmed = text.trim().slice(0, MAX_SCAN_CHARS);
  const candidates: unknown[] = [];
  let parseAttempts = 0;
  let parsedChars = 0;
  const tryParse = (s: string): unknown | undefined => {
    try {
      return JSON.parse(s);
    } catch {
      return undefined;
    }
  };
  const add = (source: string) => {
    if (
      candidates.length >= MAX_CANDIDATES ||
      parseAttempts >= MAX_PARSE_ATTEMPTS ||
      source.length > MAX_CANDIDATE_CHARS ||
      parsedChars + source.length > MAX_PARSE_CHARS
    ) return;
    parseAttempts += 1;
    parsedChars += source.length;
    const parsed = tryParse(source);
    if (parsed !== undefined) candidates.push(parsed);
  };
  add(trimmed);
  for (const fence of trimmed.matchAll(/```(?:[a-zA-Z0-9_-]*)?\s*\n?([\s\S]*?)```/g)) {
    add(fence[1].trim());
    if (candidates.length >= MAX_CANDIDATES) return candidates;
  }

  type Frame = { expected: "}" | "]"; start: number };
  type Span = { start: number; end: number };
  // Retain only the most recent openers. Model output can contain an arbitrarily
  // long unmatched prefix, and allocating one object per opener would otherwise
  // consume tens of megabytes before the scan reaches a recoverable JSON value.
  // Recent frames are sufficient because completed nested spans are separately
  // retained below for EOF recovery.
  const stack = new Array<Frame>(MAX_NESTING_FRAMES);
  let stackStart = 0;
  let stackSize = 0;
  const pushFrame = (frame: Frame) => {
    if (stackSize < MAX_NESTING_FRAMES) {
      stack[(stackStart + stackSize) % MAX_NESTING_FRAMES] = frame;
      stackSize += 1;
      return;
    }
    stack[stackStart] = frame;
    stackStart = (stackStart + 1) % MAX_NESTING_FRAMES;
  };
  const popFrame = (): Frame | undefined => {
    if (stackSize === 0) return undefined;
    const index = (stackStart + stackSize - 1) % MAX_NESTING_FRAMES;
    const frame = stack[index];
    stackSize -= 1;
    if (stackSize === 0) stackStart = 0;
    return frame;
  };
  const clearStack = () => {
    stackStart = 0;
    stackSize = 0;
  };
  const recoverySpans: Span[] = [];
  let recoveryCursor = 0;
  let inString = false;
  let escaped = false;
  for (let i = 0; i < trimmed.length; i++) {
    const c = trimmed[i];
    if (inString) {
      if (escaped) escaped = false;
      else if (c === "\\") escaped = true;
      else if (c === '"') inString = false;
      continue;
    }
    if (c === '"') {
      inString = true;
      continue;
    }
    if (c === "{" || c === "[") {
      pushFrame({ expected: c === "{" ? "}" : "]", start: i });
      continue;
    }
    if (c !== "}" && c !== "]") continue;

    const frame = popFrame();
    if (!frame || frame.expected !== c) {
      // A mismatched closer cannot belong to any currently open JSON value.
      // Resetting lets a later independent candidate recover.
      clearStack();
      continue;
    }
    const span = { start: frame.start, end: i + 1 };
    if (stackSize === 0) {
      add(trimmed.slice(span.start, span.end));
    } else if (recoverySpans.length < MAX_RECOVERY_SPANS) {
      recoverySpans.push(span);
    } else {
      // Keep recent nested spans so a valid value following a long unmatched
      // prefix can still be recovered at EOF without retaining unbounded data.
      recoverySpans[recoveryCursor] = span;
      recoveryCursor = (recoveryCursor + 1) % MAX_RECOVERY_SPANS;
    }
  }

  // If prose contained an unmatched opener before an otherwise complete JSON
  // value, no outermost span closed. Try the bounded set of nested spans in
  // source order; the main scan above remains strictly linear.
  if (stackSize > 0) {
    recoverySpans
      .sort((a, b) => a.start - b.start || b.end - a.end)
      .forEach((span) => add(trimmed.slice(span.start, span.end)));
  }
  return candidates;
}

/** Pull the first parseable JSON value out of a model reply. */
export function extractJson(text: string): unknown | null {
  return jsonCandidates(text)[0] ?? null;
}

function normalizeSeverity(value: unknown): string {
  const severity = String(value ?? "").toLowerCase().trim();
  if (["critical", "major", "severe", "high"].includes(severity)) return "high";
  if (["moderate", "medium", "warning"].includes(severity)) return "medium";
  if (["minor", "low", "suggestion", "info", "informational"].includes(severity)) return "low";
  return severity;
}

function parseIssueCandidate(json: unknown): Issue[] | null {
  const MAX_ISSUES = 1_000;
  let arr: unknown;
  if (Array.isArray(json)) arr = json;
  else if (typeof json === "object" && json !== null &&
           Array.isArray((json as Record<string, unknown>).issues)) {
    arr = (json as Record<string, unknown>).issues;
  } else {
    return null;
  }
  if (!Array.isArray(arr) || arr.length === 0 || arr.length > MAX_ISSUES) return null;

  const issues: Issue[] = [];
  const usedIds = new Set<string>();
  for (let i = 0; i < arr.length; i++) {
    const o = arr[i];
    if (!o || typeof o !== "object" || Array.isArray(o)) return null;
    const obj = o as Record<string, unknown>;
    const title = String(obj.title ?? obj.summary ?? "").trim();
    const body = String(obj.body ?? obj.description ?? obj.detail ?? "").trim();
    if (!title && !body) return null;
    const baseId = String(obj.id ?? i + 1).trim() || String(i + 1);
    let id = baseId;
    let duplicate = 2;
    while (usedIds.has(id)) id = `${baseId}#${duplicate++}`;
    usedIds.add(id);
    issues.push({
      id,
      title: title || `Issue ${i + 1}`,
      severity: normalizeSeverity(obj.severity),
      section: String(obj.section ?? "").trim(),
      body,
    });
  }
  return issues;
}

/** Interpret `text` as a structured issue list, or null if it isn't one.
 *  Accepts `{ "issues": [...] }` or a bare array of issue-shaped objects. */
export function parseIssues(text: string): Issue[] | null {
  for (const candidate of jsonCandidates(text)) {
    const issues = parseIssueCandidate(candidate);
    if (issues) return issues;
  }
  return null;
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

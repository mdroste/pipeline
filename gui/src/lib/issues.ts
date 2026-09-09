import type { PipelineReport } from "./types";

export interface IssueEvidence {
  page?: number;
  lineStart?: number;
  lineEnd?: number;
  nodeId?: string;
  assetId?: string;
  artifactPath?: string;
  sourcePath?: string;
  sourceHash?: string;
  description?: string;
  quote?: string;
}

export interface Issue {
  id: string;
  title: string;
  /** "high" | "medium" | "low" | "" (unknown). */
  severity: string;
  section: string;
  body: string;
  /** Report ids of the reviewer analyses that support this issue. */
  sources?: string[];
  evidence?: IssueEvidence[];
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
    )
      return;
    parseAttempts += 1;
    parsedChars += source.length;
    const parsed = tryParse(source);
    if (parsed !== undefined) candidates.push(parsed);
  };
  add(trimmed);
  for (const fence of trimmed.matchAll(
    /```(?:[a-zA-Z0-9_-]*)?\s*\n?([\s\S]*?)```/g,
  )) {
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
  const severity = String(value ?? "")
    .toLowerCase()
    .trim();
  if (["critical", "major", "severe", "high"].includes(severity)) return "high";
  if (["moderate", "medium", "warning"].includes(severity)) return "medium";
  if (
    ["minor", "low", "suggestion", "info", "informational"].includes(severity)
  )
    return "low";
  return severity;
}

function boundedString(value: unknown, max = 2_000): string | undefined {
  if (typeof value !== "string" && typeof value !== "number") return undefined;
  const text = String(value).trim();
  return text ? text.slice(0, max) : undefined;
}

function safeRelativePath(value: string | undefined): string | undefined {
  if (
    !value ||
    value.startsWith("/") ||
    value.includes("\\") ||
    value.includes(":") ||
    !value.split("/").every((part) => part && part !== "." && part !== "..")
  )
    return undefined;
  return value;
}

function parseEvidence(value: unknown): IssueEvidence[] {
  if (!Array.isArray(value)) return [];
  const evidence: IssueEvidence[] = [];
  for (const candidate of value.slice(0, 50)) {
    if (!candidate || typeof candidate !== "object" || Array.isArray(candidate))
      continue;
    const object = candidate as Record<string, unknown>;
    const rawPage = Number(object.page);
    const page =
      Number.isInteger(rawPage) && rawPage > 0 && rawPage <= 5_000
        ? rawPage
        : undefined;
    const rawLineStart = Number(
      object.line_start ?? object.lineStart ?? object.line,
    );
    const lineStart =
      Number.isInteger(rawLineStart) &&
      rawLineStart > 0 &&
      rawLineStart <= 10_000_000
        ? rawLineStart
        : undefined;
    const rawLineEnd = Number(object.line_end ?? object.lineEnd);
    const lineEnd =
      Number.isInteger(rawLineEnd) &&
      rawLineEnd > 0 &&
      rawLineEnd <= 10_000_000 &&
      (!lineStart || rawLineEnd >= lineStart)
        ? rawLineEnd
        : undefined;
    const nodeId = boundedString(object.node_id ?? object.nodeId, 500);
    const assetId = boundedString(object.asset_id ?? object.assetId, 500);
    const rawArtifactPath = boundedString(
      object.artifact_path ?? object.artifactPath ?? object.rel_path,
      1_000,
    );
    const artifactPath = safeRelativePath(rawArtifactPath);
    const rawSourcePath = boundedString(
      object.source_path ??
        object.sourcePath ??
        object.file_path ??
        object.filePath ??
        object.file,
      1_000,
    );
    const sourcePath = safeRelativePath(rawSourcePath);
    const sourceHash = boundedString(
      object.source_hash ?? object.sourceHash,
      1_000,
    );
    const description = boundedString(object.description ?? object.label);
    const quote = boundedString(object.quote);
    if (
      page ||
      lineStart ||
      lineEnd ||
      nodeId ||
      assetId ||
      artifactPath ||
      sourcePath ||
      sourceHash ||
      description ||
      quote
    ) {
      evidence.push({
        page,
        lineStart,
        lineEnd,
        nodeId,
        assetId,
        artifactPath,
        sourcePath,
        sourceHash,
        description,
        quote,
      });
    }
  }
  return evidence;
}

function parseIssueCandidate(json: unknown): Issue[] | null {
  const MAX_ISSUES = 1_000;
  let arr: unknown;
  if (Array.isArray(json)) arr = json;
  else if (typeof json === "object" && json !== null) {
    const object = json as Record<string, unknown>;
    if (Array.isArray(object.findings)) arr = object.findings;
    else if (Array.isArray(object.issues)) arr = object.issues;
    else return null;
  } else {
    return null;
  }
  if (!Array.isArray(arr) || arr.length === 0 || arr.length > MAX_ISSUES)
    return null;

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
      severity: normalizeSeverity(obj.priority ?? obj.severity),
      section: String(obj.category ?? obj.section ?? "").trim(),
      body,
      sources: parseSources(obj.sources),
      evidence: parseEvidence(obj.evidence),
    });
  }
  return issues;
}

function parseSources(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const sources = value
    .slice(0, 8)
    .map((entry) => boundedString(entry, 500))
    .filter((entry): entry is string => !!entry);
  return sources.length ? sources : undefined;
}

/** Interpret `text` as a structured finding list, or null if it isn't one.
 *  Accepts canonical `{ "findings": [...] }`, legacy `{ "issues": [...] }`,
 *  or a bare array of issue-shaped objects. */
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
  if ((report.products?.schema_version ?? 0) > 0) {
    const published = report.products?.findings?.findings;
    if (!published) return null;
    return published.map((finding, index) => ({
      id: finding.id || String(index + 1),
      title: finding.title || `Issue ${index + 1}`,
      severity: normalizeSeverity(finding.priority),
      section: finding.category ?? "",
      body: finding.body ?? "",
      sources: parseSources(finding.sources),
      evidence: parseEvidence(finding.evidence ?? []),
    }));
  }
  const outputs = report.step_outputs ?? [];
  for (let i = outputs.length - 1; i >= 0; i--) {
    if (outputs[i].skipped) continue;
    const issues = parseIssues(outputs[i].raw_text);
    if (issues) return issues;
  }
  return null;
}

function specialistCitation(value: unknown): string | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const object = value as Record<string, unknown>;
  const parts: string[] = [];
  const page = Number(object.page);
  if (Number.isInteger(page) && page > 0) parts.push(`p. ${page}`);
  const sourcePath = boundedString(object.source_path, 1_000);
  if (sourcePath) {
    const start = Number(object.line_start);
    const end = Number(object.line_end);
    const lines =
      Number.isInteger(start) && start > 0
        ? Number.isInteger(end) && end > start
          ? `:${start}–${end}`
          : `:${start}`
        : "";
    parts.push(`${sourcePath}${lines}`);
  }
  const description = boundedString(object.description);
  if (description) parts.push(description);
  const nodeId = boundedString(object.node_id, 500);
  if (nodeId) parts.push(`node ${nodeId}`);
  const assetId = boundedString(object.asset_id, 500);
  if (assetId) parts.push(`asset ${assetId}`);
  const quote = boundedString(object.quote, 300);
  if (quote) parts.push(`“${quote}”`);
  return parts.length ? parts.join(" · ") : null;
}

/** Render a specialist findings artifact (the structured referee-report
 *  contract used by Automatic Paper Review reviewer passes) back to its
 *  readable Markdown form. Returns null when `text` is not one. Mirrors the
 *  backend renderer in auto_review.rs. */
export function renderSpecialistMarkdown(text: string): string | null {
  let value: unknown;
  try {
    value = JSON.parse(text.trim());
  } catch {
    return null;
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const findings = (value as Record<string, unknown>).findings;
  if (!Array.isArray(findings)) return null;
  if (findings.length === 0) return "No material issues identified.";

  const blocks: string[] = [];
  for (let index = 0; index < findings.length; index++) {
    const finding = findings[index];
    if (!finding || typeof finding !== "object" || Array.isArray(finding))
      return null;
    const object = finding as Record<string, unknown>;
    if (!("problem" in object) || !("what_would_help" in object)) return null;
    const field = (key: string) => boundedString(object[key], 50_000) ?? "";
    let block = `**#${index + 1}. ${field("title")}**\n\n`;
    for (const [key, label] of [
      ["in_the_paper", "In the paper"],
      ["problem", "The problem"],
      ["consequence", "Consequence"],
      ["what_would_help", "What would help"],
    ] as const) {
      const value = field(key);
      if (value) block += `- **${label}:** ${value}\n`;
    }
    const citations = Array.isArray(object.evidence)
      ? object.evidence
          .map(specialistCitation)
          .filter((entry): entry is string => !!entry)
      : [];
    if (citations.length) block += `- **Location:** ${citations.join("; ")}\n`;
    blocks.push(block.trimEnd());
  }
  return blocks.join("\n\n");
}

const MAX_TABLE_ROWS = 100;
const MAX_TABLE_COLUMNS = 12;
const MAX_CELL_CHARS = 300;

function tableCell(value: unknown): string {
  const text =
    value === undefined || value === null
      ? ""
      : typeof value === "string"
        ? value
        : JSON.stringify(value);
  const flattened = text.replace(/\s+/g, " ").trim();
  const bounded =
    flattened.length > MAX_CELL_CHARS
      ? `${flattened.slice(0, MAX_CELL_CHARS)}…`
      : flattened;
  return bounded.replace(/\|/g, "\\|");
}

function renderJsonValue(value: unknown, depth: number): string {
  if (depth > 4) return `\`${tableCell(value)}\``;
  if (Array.isArray(value)) {
    if (value.length === 0) return "_(empty)_";
    // Array of objects → table over the union of keys (bounded).
    if (
      value.every(
        (entry) => entry && typeof entry === "object" && !Array.isArray(entry),
      )
    ) {
      const keys: string[] = [];
      for (const entry of value.slice(0, MAX_TABLE_ROWS)) {
        for (const key of Object.keys(entry as Record<string, unknown>)) {
          if (!keys.includes(key) && keys.length < MAX_TABLE_COLUMNS)
            keys.push(key);
        }
      }
      const header = `| ${keys.join(" | ")} |\n| ${keys.map(() => "---").join(" | ")} |`;
      const rows = value
        .slice(0, MAX_TABLE_ROWS)
        .map(
          (entry) =>
            `| ${keys.map((key) => tableCell((entry as Record<string, unknown>)[key])).join(" | ")} |`,
        );
      const overflow =
        value.length > MAX_TABLE_ROWS
          ? `\n\n_(${value.length - MAX_TABLE_ROWS} more rows in the raw artifact)_`
          : "";
      return `${header}\n${rows.join("\n")}${overflow}`;
    }
    return value
      .slice(0, MAX_TABLE_ROWS)
      .map((entry) => `- ${tableCell(entry)}`)
      .join("\n");
  }
  if (value && typeof value === "object") {
    return Object.entries(value as Record<string, unknown>)
      .map(([key, child]) => {
        const rendered = renderJsonValue(child, depth + 1);
        return rendered.includes("\n")
          ? `**${key}**\n\n${rendered}`
          : `**${key}**: ${rendered}`;
      })
      .join("\n\n");
  }
  if (typeof value === "string") return value;
  return `\`${JSON.stringify(value)}\``;
}

/** Generic readable rendering of a structured step artifact: arrays of
 *  objects become tables, plain arrays become lists, objects become labeled
 *  sections. Returns null when `text` is not a JSON object. */
export function renderStructuredMarkdown(text: string): string | null {
  let value: unknown;
  try {
    value = JSON.parse(text.trim());
  } catch {
    return null;
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  return renderJsonValue(value, 0);
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

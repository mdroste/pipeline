// Render an arbitrary survey JSON object (any schema a profile's survey
// prompt produced) as readable markdown. Paper-shaped surveys have their own
// dedicated renderer below; the generic renderer assumes nothing about keys.

import { isPaperOrientation } from "./types";
import type { OrientationMap, PaperMetadata, PipelineReport } from "./types";

function titleCase(key: string): string {
  return key
    .replace(/[_-]+/g, " ")
    .trim()
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

/** Flatten a value into a single markdown-table-safe line. */
function cell(value: unknown): string {
  let text: string;
  if (value === null || value === undefined) text = "";
  else if (typeof value === "string") text = value;
  else if (typeof value === "number" || typeof value === "boolean") text = String(value);
  else text = JSON.stringify(value);
  return text.replace(/\|/g, "\\|").replace(/\s*\n\s*/g, " ");
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** Array of objects → table with the union of keys, in first-seen order. */
function renderObjectArray(rows: Record<string, unknown>[]): string {
  const columns: string[] = [];
  for (const row of rows) {
    for (const key of Object.keys(row)) {
      if (!columns.includes(key)) columns.push(key);
    }
  }
  let md = `| ${columns.map(titleCase).join(" | ")} |\n`;
  md += `|${columns.map(() => "---").join("|")}|\n`;
  for (const row of rows) {
    md += `| ${columns.map((c) => cell(row[c])).join(" | ")} |\n`;
  }
  return md + "\n";
}

function renderValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value.trim() ? `${value.trim()}\n\n` : "";
  if (typeof value === "number" || typeof value === "boolean") return `${value}\n\n`;
  if (Array.isArray(value)) {
    if (value.length === 0) return "";
    if (value.every(isPlainObject)) {
      return renderObjectArray(value as Record<string, unknown>[]);
    }
    return value.map((v) => `- ${cell(v)}`).join("\n") + "\n\n";
  }
  if (isPlainObject(value)) {
    const entries = Object.entries(value).filter(([, v]) => v !== null && v !== undefined);
    if (entries.length === 0) return "";
    return entries.map(([k, v]) => `- **${titleCase(k)}**: ${cell(v)}`).join("\n") + "\n\n";
  }
  return "";
}

/**
 * Render any survey JSON as markdown: one section per top-level key, tables
 * for arrays of objects, bullets for lists, paragraphs for strings. Falls
 * back to a JSON code fence for non-object surveys.
 */
export function renderGenericSurvey(survey: unknown): string {
  if (!isPlainObject(survey)) {
    return `# Survey\n\n\`\`\`json\n${JSON.stringify(survey, null, 2)}\n\`\`\`\n`;
  }
  let md = `# Survey\n\n`;
  for (const [key, value] of Object.entries(survey)) {
    const body = renderValue(value);
    if (!body) continue;
    md += `## ${titleCase(key)}\n\n${body}`;
  }
  return md;
}

function renderOrientationMap(orientation: OrientationMap): string {
  // Defensive: paper-shaped surveys may omit fields the schema defaults.
  const metadata = (orientation.metadata ?? {}) as Partial<PaperMetadata>;
  let md = `# Orientation Map\n\n`;
  md += `**Title**: ${metadata.title ?? ""}  \n`;
  if (metadata.authors?.length) md += `**Authors**: ${metadata.authors.join(", ")}  \n`;
  md += `**Type**: ${metadata.paper_type ?? "unknown"}`;
  if (metadata.page_count) md += ` · **Pages**: ${metadata.page_count}`;
  md += `  \n`;
  if (metadata.has_appendix) md += `**Appendix**: yes  \n`;
  if (metadata.has_online_appendix) md += `**Online appendix**: yes  \n`;
  md += `\n`;

  if (orientation.stated_contribution) {
    md += `## Stated Contribution\n\n${orientation.stated_contribution}\n\n`;
  }
  if (orientation.sections?.length) {
    md += `## Sections\n\n| # | Title | Pages |\n|---|-------|-------|\n`;
    for (const section of orientation.sections) {
      const pages = section.page_start
        ? (section.page_end ? `${section.page_start}–${section.page_end}` : `${section.page_start}`)
        : "";
      md += `| ${section.number} | ${section.title} | ${pages} |\n`;
    }
    md += `\n`;
  }
  if (orientation.formal_results?.length) {
    md += `## Formal Results\n\n`;
    for (const result of orientation.formal_results) {
      md += `- **${result.kind} ${result.number}**${result.page ? ` (p. ${result.page})` : ""}: ${result.summary}`;
      if (result.proof_location) md += ` — *Proof: ${result.proof_location}*`;
      md += `\n`;
    }
    md += `\n`;
  }
  if (orientation.tables_figures?.length) {
    md += `## Tables & Figures\n\n`;
    for (const item of orientation.tables_figures) {
      md += `- **${item.kind} ${item.number}**${item.page ? ` (p. ${item.page})` : ""}: ${item.caption_summary}`;
      if (item.what_it_shows) md += ` — ${item.what_it_shows}`;
      md += `\n`;
    }
    md += `\n`;
  }
  if (orientation.notation?.length) {
    md += `## Notation\n\n| Symbol | Definition | Introduced |\n|--------|------------|------------|\n`;
    for (const notation of orientation.notation) {
      md += `| ${notation.symbol} | ${notation.definition} | ${notation.page_introduced ? `p. ${notation.page_introduced}` : ""} |\n`;
    }
    md += `\n`;
  }
  if (orientation.key_references?.length) {
    md += `## Key References\n\n`;
    for (const reference of orientation.key_references) md += `- ${reference}\n`;
    md += `\n`;
  }
  if (orientation.extraction_quality_notes?.length) {
    md += `## Extraction Quality Notes\n\n`;
    for (const note of orientation.extraction_quality_notes) {
      md += `- **${note.page_range}**: ${note.description}\n`;
    }
    md += `\n`;
  }
  return md;
}

/** Render any survey JSON: paper view when paper-shaped, generic sections otherwise. */
export function renderSurvey(orientation: PipelineReport["orientation"]): string {
  return isPaperOrientation(orientation)
    ? renderOrientationMap(orientation)
    : renderGenericSurvey(orientation);
}

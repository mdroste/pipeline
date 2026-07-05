// Render an arbitrary survey JSON object (any schema a profile's survey
// prompt produced) as readable markdown. Paper-shaped surveys have their own
// dedicated renderer in App.tsx; this one assumes nothing about the keys.

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

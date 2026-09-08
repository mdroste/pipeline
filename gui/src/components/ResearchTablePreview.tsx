import { useMemo } from "react";

// RFC-style quoted fields, including doubled quotes and embedded newlines.
// Preserve text values: never infer numeric units, dates, or missing values.
export function parseResearchTable(text: string, delimiter: string, limit = 200) {
  const rows: string[][] = [];
  let row: string[] = [], field = "", quoted = false, afterQuote = false;
  const cell = () => { row.push(field); field = ""; afterQuote = false; };
  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    if (quoted) {
      if (char === '"') { if (text[i + 1] === '"') { field += '"'; i++; } else { quoted = false; afterQuote = true; } }
      else field += char;
    } else if (char === delimiter) { cell(); }
    else if (char === "\n" || char === "\r") {
      if (char === "\r" && text[i + 1] === "\n") i++;
      cell(); rows.push(row); row = [];
      if (rows.length >= limit) return { rows, truncated: i < text.length - 1, error: null };
    } else if (char === '"' && !field && !afterQuote) { quoted = true; }
    else if (afterQuote || char === '"') return { rows: [], truncated: false, error: "Malformed quoted field; inspect the source text." };
    else field += char;
    if (row.length >= 60 || field.length > 64 * 1024) return { rows: [], truncated: true, error: "Table exceeds 60 columns or a 64 KiB cell; inspect the source text." };
  }
  if (quoted) return { rows: [], truncated: false, error: "Incomplete quoted field; inspect the source text or next segment." };
  if (field || row.length || afterQuote) { cell(); rows.push(row); }
  return { rows, truncated: false, error: null };
}
export default function ResearchTablePreview({ text, path }: { text: string; path: string }) {
  const table = useMemo(() => parseResearchTable(text, path.toLowerCase().endsWith(".tsv") ? "\t" : ","), [text, path]);
  return <div className="overflow-auto">{table.error ? <p className="text-xs text-amber-700">{table.error}</p> : <table aria-label="Research data preview" className="border-collapse text-xs"><tbody>{table.rows.map((row, i) => <tr key={i}>{row.map((value, j) => <td key={j} className="whitespace-pre-wrap border px-3 py-2">{value}</td>)}</tr>)}</tbody></table>}{table.truncated && <p className="mt-2 text-xs text-gray-500">Showing up to 200 rows.</p>}</div>;
}

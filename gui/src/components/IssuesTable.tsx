import { useState, useMemo, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Issue } from "../lib/issues";
import { severityRank } from "../lib/issues";

interface Props {
  issues: Issue[];
  runId: string | null;
}

type Verdict = "accept" | "reject" | "done" | "";
interface Annotation {
  status: Verdict;
  note: string;
}
type Annotations = Record<string, Annotation>;

const SEVERITY_STYLES: Record<string, string> = {
  high: "bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300",
  medium: "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300",
  low: "bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300",
};

function sevStyle(sev: string): string {
  return SEVERITY_STYLES[sev] || "bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400";
}

const VERDICT_STYLES: Record<Verdict, string> = {
  accept: "bg-green-600 text-white",
  reject: "bg-red-600 text-white",
  done: "bg-gray-500 text-white",
  "": "",
};

export default function IssuesTable({ issues, runId }: Props) {
  const [annotations, setAnnotations] = useState<Annotations>({});
  const [severityFilter, setSeverityFilter] = useState<string>("all");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const loadedRef = useRef(false);
  const [saved, setSaved] = useState(false);

  // Load saved annotations for this run.
  useEffect(() => {
    if (!runId) {
      loadedRef.current = true;
      return;
    }
    invoke<string>("get_annotations", { runId })
      .then((s) => {
        try {
          setAnnotations(JSON.parse(s) as Annotations);
        } catch {
          /* ignore */
        }
      })
      .catch(() => {})
      .finally(() => {
        loadedRef.current = true;
      });
  }, [runId]);

  // Debounced persist after the initial load.
  useEffect(() => {
    if (!runId || !loadedRef.current) return;
    const t = setTimeout(() => {
      invoke("save_annotations", { runId, content: JSON.stringify(annotations) })
        .then(() => {
          setSaved(true);
          setTimeout(() => setSaved(false), 1200);
        })
        .catch(() => {});
    }, 600);
    return () => clearTimeout(t);
  }, [annotations, runId]);

  const setVerdict = (id: string, status: Verdict) =>
    setAnnotations((prev) => ({ ...prev, [id]: { status: prev[id]?.status === status ? "" : status, note: prev[id]?.note ?? "" } }));
  const setNote = (id: string, note: string) =>
    setAnnotations((prev) => ({ ...prev, [id]: { status: prev[id]?.status ?? "", note } }));

  const toggleExpand = (id: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });

  const sorted = useMemo(
    () => [...issues].sort((a, b) => severityRank(a.severity) - severityRank(b.severity)),
    [issues]
  );
  const visible = useMemo(
    () => (severityFilter === "all" ? sorted : sorted.filter((i) => i.severity === severityFilter)),
    [sorted, severityFilter]
  );

  const counts = useMemo(() => {
    const c: Record<Verdict, number> = { accept: 0, reject: 0, done: 0, "": 0 };
    for (const i of issues) c[annotations[i.id]?.status ?? ""]++;
    return c;
  }, [issues, annotations]);

  const exportAccepted = async () => {
    const accepted = sorted.filter((i) => annotations[i.id]?.status === "accept");
    const md =
      "# Accepted issues\n\n" +
      accepted
        .map((i, n) => {
          const note = annotations[i.id]?.note?.trim();
          return `## ${n + 1}. ${i.title}${i.severity ? ` _(${i.severity})_` : ""}\n\n${i.body}${note ? `\n\n> **Note:** ${note}` : ""}`;
        })
        .join("\n\n");
    try {
      const path = await save({ defaultPath: "accepted-issues.md", filters: [{ name: "Markdown", extensions: ["md"] }] });
      if (path) await invoke("save_text_file", { path, content: md });
    } catch {
      /* cancelled */
    }
  };

  const severities = ["all", "high", "medium", "low"];

  return (
    <div className="max-w-4xl mx-auto p-6">
      <div className="flex items-center gap-2 mb-4 flex-wrap">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">Issues ({issues.length})</h2>
        <div className="flex items-center gap-1 ml-2">
          {severities.map((s) => (
            <button
              key={s}
              onClick={() => setSeverityFilter(s)}
              className={`px-2 py-0.5 rounded text-xs capitalize transition-colors ${
                severityFilter === s
                  ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900"
                  : "text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100"
              }`}
            >
              {s}
            </button>
          ))}
        </div>
        <div className="ml-auto flex items-center gap-3 text-xs text-gray-500 dark:text-gray-400">
          {counts.accept > 0 && <span className="text-green-600 dark:text-green-400">{counts.accept} accepted</span>}
          {counts.reject > 0 && <span className="text-red-600 dark:text-red-400">{counts.reject} rejected</span>}
          {saved && <span className="text-gray-400">saved ✓</span>}
          <button
            onClick={exportAccepted}
            disabled={counts.accept === 0}
            className="px-2 py-1 rounded border border-gray-300 dark:border-gray-600 hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            Export accepted
          </button>
        </div>
      </div>

      <div className="space-y-1.5">
        {visible.map((issue) => {
          const ann = annotations[issue.id];
          const isOpen = expanded.has(issue.id);
          return (
            <div
              key={issue.id}
              className={`border rounded-lg bg-white dark:bg-gray-900 transition-colors ${
                ann?.status === "reject"
                  ? "border-red-200 dark:border-red-900 opacity-60"
                  : ann?.status === "accept"
                    ? "border-green-200 dark:border-green-900"
                    : "border-gray-200 dark:border-gray-800"
              }`}
            >
              <div className="flex items-center gap-3 px-3 py-2">
                <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium uppercase shrink-0 ${sevStyle(issue.severity)}`}>
                  {issue.severity || "—"}
                </span>
                <button onClick={() => toggleExpand(issue.id)} className="flex-1 text-left min-w-0">
                  <span className="text-sm text-gray-900 dark:text-gray-100">{issue.title}</span>
                  {issue.section && <span className="text-xs text-gray-400 ml-2">§{issue.section}</span>}
                </button>
                {ann?.status && (
                  <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${VERDICT_STYLES[ann.status]}`}>
                    {ann.status}
                  </span>
                )}
                <div className="flex items-center gap-1 shrink-0">
                  {(["accept", "reject", "done"] as Verdict[]).map((v) => (
                    <button
                      key={v}
                      onClick={() => setVerdict(issue.id, v)}
                      className={`px-1.5 py-0.5 rounded text-[10px] border transition-colors ${
                        ann?.status === v
                          ? VERDICT_STYLES[v]
                          : "border-gray-300 dark:border-gray-600 text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100"
                      }`}
                      title={v}
                    >
                      {v === "accept" ? "✓" : v === "reject" ? "✕" : "done"}
                    </button>
                  ))}
                </div>
              </div>
              {isOpen && (
                <div className="px-3 pb-3 border-t border-gray-100 dark:border-gray-800 pt-2">
                  <div className="report-content !max-w-none !p-0 text-sm">
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>{issue.body}</ReactMarkdown>
                  </div>
                  <input
                    value={ann?.note ?? ""}
                    onChange={(e) => setNote(issue.id, e.target.value)}
                    placeholder="Add a note…"
                    className="mt-2 w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                  />
                </div>
              )}
            </div>
          );
        })}
        {visible.length === 0 && <p className="text-sm text-gray-400 px-2">No issues at this severity.</p>}
      </div>
      {!runId && (
        <p className="text-[11px] text-gray-400 mt-4">Annotations aren't saved for this view (no run directory).</p>
      )}
    </div>
  );
}

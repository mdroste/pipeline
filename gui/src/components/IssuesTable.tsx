import { useState, useMemo, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Issue, IssueEvidence } from "../lib/issues";
import { severityRank } from "../lib/issues";
import SafeMarkdownLink from "./SafeMarkdownLink";

interface Props {
  issues: Issue[];
  runId: string | null;
  onOpenEvidence?: (evidence: IssueEvidence) => void;
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
  accept: "bg-green-700 text-white",
  reject: "bg-red-600 text-white",
  done: "bg-gray-500 text-white",
  "": "",
};

function evidenceCitation(item: IssueEvidence): string {
  const references = [
    item.page ? `p. ${item.page}` : "",
    item.lineStart
      ? `line ${item.lineStart}${item.lineEnd && item.lineEnd !== item.lineStart ? `–${item.lineEnd}` : ""}`
      : "",
    item.nodeId ? `node ${item.nodeId}` : "",
    item.assetId ? `asset ${item.assetId}` : "",
    item.artifactPath ? `artifact ${item.artifactPath}` : "",
    item.sourcePath ? `source ${item.sourcePath}` : "",
    item.sourceHash ? `source hash ${item.sourceHash}` : "",
  ].filter(Boolean);
  const location = references.join(", ") || item.description || "source";
  const description = item.description && item.description !== location
    ? ` — ${item.description}`
    : "";
  return `${location}${description}${item.quote ? ` (“${item.quote}”)` : ""}`;
}

export default function IssuesTable({ issues, runId, onOpenEvidence }: Props) {
  const [annotations, setAnnotations] = useState<Annotations>({});
  const [severityFilter, setSeverityFilter] = useState<string>("all");
  const [verdictFilter, setVerdictFilter] = useState<string>("all");
  const [issueSearch, setIssueSearch] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [bulkVerdict, setBulkVerdict] = useState<Exclude<Verdict, "">>("accept");
  const [bulkRationale, setBulkRationale] = useState("");
  const [annotationsLoaded, setAnnotationsLoaded] = useState(!runId);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const annotationsRef = useRef<Annotations>({});
  const loadedRunRef = useRef<string | null>(null);
  const dirtyRunRef = useRef<string | null>(null);
  const loadVersionRef = useRef(0);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const savedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const saveChainRef = useRef<Promise<void>>(Promise.resolve());
  const [saved, setSaved] = useState(false);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [persistenceError, setPersistenceError] = useState<{
    operation: "load" | "save" | "export";
    message: string;
  } | null>(null);

  useEffect(() => {
    annotationsRef.current = annotations;
  }, [annotations]);

  const enqueueSave = (targetRun: string, value: Annotations) => {
    const content = JSON.stringify(value);
    const request = saveChainRef.current
      .catch(() => undefined)
      .then(() => invoke<void>("save_annotations", { runId: targetRun, content }));
    saveChainRef.current = request.catch(() => undefined);
    return { request, content };
  };

  // Load saved annotations for this run.
  useEffect(() => {
    const version = ++loadVersionRef.current;
    loadedRunRef.current = null;
    dirtyRunRef.current = null;
    annotationsRef.current = {};
    setAnnotations({});
    setSaved(false);
    setAnnotationsLoaded(false);
    setSelected(new Set());
    if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
    setPersistenceError(null);
    if (!runId) {
      setAnnotationsLoaded(true);
      return;
    }
    invoke<string>("get_annotations", { runId })
      .then((s) => {
        if (loadVersionRef.current !== version) return;
        try {
          const loaded = JSON.parse(s) as Annotations;
          if (!loaded || typeof loaded !== "object" || Array.isArray(loaded)) {
            throw new Error("annotations file is not a JSON object");
          }
          annotationsRef.current = loaded;
          setAnnotations(loaded);
          loadedRunRef.current = runId;
          setAnnotationsLoaded(true);
          setPersistenceError(null);
        } catch (error) {
          setPersistenceError({
            operation: "load",
            message: error instanceof Error ? error.message : String(error),
          });
        }
      })
      .catch((error) => {
        if (loadVersionRef.current !== version) return;
        setPersistenceError({
          operation: "load",
          message: error instanceof Error ? error.message : String(error),
        });
      });
    return () => {
      if (loadVersionRef.current === version) loadVersionRef.current++;
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
      if (dirtyRunRef.current === runId) {
        enqueueSave(runId, annotationsRef.current);
        dirtyRunRef.current = null;
      }
    };
  }, [runId, loadAttempt]);

  // Debounced, serialized persistence after a user edit.
  useEffect(() => {
    if (!runId || loadedRunRef.current !== runId || dirtyRunRef.current !== runId) return;
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    const snapshot = JSON.stringify(annotations);
    saveTimerRef.current = setTimeout(() => {
      const { request } = enqueueSave(runId, annotations);
      request
        .then(() => {
          if (runId !== loadedRunRef.current) return;
          setPersistenceError(null);
          if (JSON.stringify(annotationsRef.current) === snapshot) {
            dirtyRunRef.current = null;
            setSaved(true);
            if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
            savedTimerRef.current = setTimeout(() => setSaved(false), 1200);
          }
        })
        .catch((error) => {
          if (runId === loadedRunRef.current) {
            setSaved(false);
            setPersistenceError({
              operation: "save",
              message: error instanceof Error ? error.message : String(error),
            });
          }
        });
    }, 600);
    return () => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    };
  }, [annotations, runId]);

  useEffect(
    () => () => {
      if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
    },
    [],
  );

  const setVerdict = (id: string, status: Verdict) => {
    if (runId && loadedRunRef.current !== runId) return;
    if (runId) dirtyRunRef.current = runId;
    setAnnotations((prev) => ({ ...prev, [id]: { status: prev[id]?.status === status ? "" : status, note: prev[id]?.note ?? "" } }));
  };
  const setNote = (id: string, note: string) => {
    if (runId && loadedRunRef.current !== runId) return;
    if (runId) dirtyRunRef.current = runId;
    setAnnotations((prev) => ({ ...prev, [id]: { status: prev[id]?.status ?? "", note } }));
  };

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
    () => sorted.filter((issue) => {
      if (severityFilter !== "all" && issue.severity !== severityFilter) return false;
      const verdict = annotations[issue.id]?.status ?? "";
      if (verdictFilter === "unreviewed" && verdict) return false;
      if (verdictFilter !== "all" && verdictFilter !== "unreviewed" && verdict !== verdictFilter) return false;
      const needle = issueSearch.trim().toLowerCase();
      return !needle || [issue.title, issue.body, issue.section, ...(issue.sources ?? [])]
        .filter(Boolean).join(" ").toLowerCase().includes(needle);
    }),
    [annotations, issueSearch, sorted, severityFilter, verdictFilter]
  );

  const counts = useMemo(() => {
    const c: Record<Verdict, number> = { accept: 0, reject: 0, done: 0, "": 0 };
    for (const i of issues) c[annotations[i.id]?.status ?? ""]++;
    return c;
  }, [issues, annotations]);

  const exportDecisions = async () => {
    const md =
      "# Issue decisions\n\n" +
      sorted
        .map((i, n) => {
          const note = annotations[i.id]?.note?.trim();
          const status = annotations[i.id]?.status || "unreviewed";
          const evidence = i.evidence?.length
            ? `\n\n**Evidence:** ${i.evidence.map(evidenceCitation).join("; ")}`
            : "";
          return `## ${n + 1}. ${i.title}${i.severity ? ` _(${i.severity})_` : ""}\n\n**Decision:** ${status}\n\n${i.body}${evidence}${note ? `\n\n> **Rationale:** ${note}` : ""}`;
        })
        .join("\n\n");
    try {
      // The backend runs the native save dialog and writes only to the
      // user-chosen path; the webview never supplies a filesystem path.
      await invoke("save_text_file", { content: md, suggestedName: "issue-decisions.md" });
    } catch (error) {
      setPersistenceError({
        operation: "export",
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const retryPersistence = () => {
    if (!runId || !persistenceError) return;
    if (persistenceError.operation === "load") {
      setLoadAttempt((attempt) => attempt + 1);
      return;
    }
    if (persistenceError.operation === "save") {
      const snapshot = JSON.stringify(annotationsRef.current);
      const { request } = enqueueSave(runId, annotationsRef.current);
      request
        .then(() => {
          if (runId !== loadedRunRef.current) return;
          setPersistenceError(null);
          if (
            JSON.stringify(annotationsRef.current) === snapshot
          ) {
            dirtyRunRef.current = null;
            setSaved(true);
            if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
            savedTimerRef.current = setTimeout(() => setSaved(false), 1200);
          }
        })
        .catch((error) => {
          if (runId === loadedRunRef.current) {
            setPersistenceError({
              operation: "save",
              message: error instanceof Error ? error.message : String(error),
            });
          }
        });
    }
  };

  const severities = ["all", "high", "medium", "low"];

  const applyBulkDecision = () => {
    if (!annotationsLoaded || selected.size === 0 || !bulkRationale.trim()) return;
    if (runId) dirtyRunRef.current = runId;
    setAnnotations((current) => {
      const next = { ...current };
      for (const id of selected) {
        next[id] = { status: bulkVerdict, note: bulkRationale.trim() };
      }
      return next;
    });
    setSelected(new Set());
    setBulkRationale("");
  };

  return (
    <div className="max-w-4xl mx-auto p-6">
      <div className="flex items-center gap-2 mb-4 flex-wrap">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">Issues ({issues.length})</h2>
        <div className="flex items-center gap-1 ml-2">
          {severities.map((s) => (
            <button
              key={s}
              onClick={() => setSeverityFilter(s)}
              aria-pressed={severityFilter === s}
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
        <select aria-label="Filter by decision" value={verdictFilter} onChange={(event) => setVerdictFilter(event.target.value)} className="rounded border border-gray-300 bg-white px-2 py-1 text-xs dark:border-gray-700 dark:bg-gray-900">
          <option value="all">All decisions</option>
          <option value="unreviewed">Unreviewed ({counts[""]})</option>
          <option value="accept">Accepted</option>
          <option value="reject">Rejected</option>
          <option value="done">Done</option>
        </select>
        <input aria-label="Search issues" value={issueSearch} onChange={(event) => setIssueSearch(event.target.value)} placeholder="Search title, text, reviewer…" className="min-w-48 rounded border border-gray-300 bg-white px-2 py-1 text-xs dark:border-gray-700 dark:bg-gray-900" />
        <div className="ml-auto flex items-center gap-3 text-xs text-gray-500 dark:text-gray-400">
          {counts.accept > 0 && <span className="text-green-700 dark:text-green-400">{counts.accept} accepted</span>}
          {counts.reject > 0 && <span className="text-red-600 dark:text-red-400">{counts.reject} rejected</span>}
          {saved && <span className="text-gray-500 dark:text-gray-400">saved ✓</span>}
          {!annotationsLoaded && <span role="status">Loading decisions…</span>}
          <button
            onClick={exportDecisions}
            disabled={!annotationsLoaded || issues.length === 0}
            className="px-2 py-1 rounded border border-gray-300 dark:border-gray-600 hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            Export all decisions
          </button>
        </div>
      </div>

      {selected.size > 0 && (
        <div className="mb-4 flex flex-wrap items-center gap-2 rounded-lg border border-gray-200 bg-gray-50 p-3 text-xs dark:border-gray-800 dark:bg-gray-900/60">
          <span className="font-medium">{selected.size} selected</span>
          <select aria-label="Bulk decision" value={bulkVerdict} onChange={(event) => setBulkVerdict(event.target.value as Exclude<Verdict, "">)} className="rounded border border-gray-300 bg-white px-2 py-1 dark:border-gray-700 dark:bg-gray-900">
            <option value="accept">Accept</option><option value="reject">Reject</option><option value="done">Done</option>
          </select>
          <textarea rows={2} aria-label="Bulk decision rationale" value={bulkRationale} onChange={(event) => setBulkRationale(event.target.value)} placeholder="Required rationale" className="min-w-56 flex-1 rounded border border-gray-300 bg-white px-2 py-1 dark:border-gray-700 dark:bg-gray-900" />
          <button onClick={applyBulkDecision} disabled={!bulkRationale.trim() || !annotationsLoaded} className="rounded bg-gray-900 px-2 py-1 text-white disabled:opacity-40 dark:bg-gray-100 dark:text-gray-900">Apply decision</button>
          <button onClick={() => setSelected(new Set())} className="px-2 py-1 text-gray-500">Clear</button>
        </div>
      )}

      {persistenceError && (
        <div role="alert" className="mb-4 rounded border border-red-200 dark:border-red-900 bg-red-50 dark:bg-red-950/30 p-3 text-xs text-red-700 dark:text-red-300">
          <span className="font-medium">
            {persistenceError.operation === "export"
              ? "Decision export failed:"
              : `Annotation ${persistenceError.operation} failed:`}
          </span>{" "}
          {persistenceError.message}
          {persistenceError.operation !== "export" && (
            <button
              type="button"
              onClick={retryPersistence}
              className="ml-2 rounded border border-red-300 dark:border-red-800 px-2 py-0.5 hover:bg-red-100 dark:hover:bg-red-900/40"
            >
              Retry
            </button>
          )}
        </div>
      )}

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
                <input
                  type="checkbox"
                  aria-label={`Select issue: ${issue.title}`}
                  checked={selected.has(issue.id)}
                  disabled={!annotationsLoaded}
                  onChange={() => setSelected((current) => {
                    const next = new Set(current);
                    next.has(issue.id) ? next.delete(issue.id) : next.add(issue.id);
                    return next;
                  })}
                />
                <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium uppercase shrink-0 ${sevStyle(issue.severity)}`}>
                  {issue.severity || "—"}
                </span>
                <button
                  onClick={() => toggleExpand(issue.id)}
                  aria-expanded={isOpen}
                  className="flex-1 text-left min-w-0"
                >
                  <span className="text-sm text-gray-900 dark:text-gray-100">{issue.title}</span>
                  {issue.section && <span className="text-xs text-gray-500 dark:text-gray-400 ml-2">§{issue.section}</span>}
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
                      disabled={!annotationsLoaded}
                      aria-label={`${v} issue: ${issue.title}`}
                      aria-pressed={ann?.status === v}
                      className={`px-1.5 py-0.5 rounded text-[10px] border transition-colors ${
                        ann?.status === v
                          ? VERDICT_STYLES[v]
                          : "border-gray-300 dark:border-gray-600 text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100 disabled:cursor-wait disabled:opacity-40"
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
                    <ReactMarkdown
                      remarkPlugins={[remarkGfm]}
                      components={{ a: SafeMarkdownLink }}
                    >
                      {issue.body}
                    </ReactMarkdown>
                  </div>
                  {issue.sources && issue.sources.length > 0 && (
                    <p className="mt-2 text-[11px] text-gray-500 dark:text-gray-400">
                      <span className="font-semibold uppercase tracking-[0.1em] text-[10px]">Reviewers:</span>{" "}
                      {issue.sources.join(", ")}
                    </p>
                  )}
                  {issue.evidence && issue.evidence.length > 0 && (
                    <div className="mt-3 rounded-lg border border-gray-200 bg-gray-50 p-3 dark:border-gray-800 dark:bg-gray-900/70">
                      <p className="text-[10px] font-semibold uppercase tracking-[0.1em] text-gray-500 dark:text-gray-400">Evidence</p>
                      <div className="mt-2 space-y-2">
                        {issue.evidence.map((evidence, evidenceIndex) => {
                          const canOpen = !!onOpenEvidence && (!!evidence.page || !!evidence.artifactPath);
                          const lineLabel = evidence.lineStart
                            ? `:${evidence.lineStart}${evidence.lineEnd && evidence.lineEnd !== evidence.lineStart ? `–${evidence.lineEnd}` : ""}`
                            : "";
                          const label = evidence.page
                            ? `Page ${evidence.page}`
                            : evidence.sourcePath
                              ? `${evidence.sourcePath}${lineLabel}`
                              : evidence.artifactPath
                                ? `${evidence.artifactPath}${lineLabel}`
                                : evidence.description || evidence.assetId || evidence.nodeId || (lineLabel ? `Line ${lineLabel.slice(1)}` : `Source ${evidenceIndex + 1}`);
                          const location = (
                            <span className={`inline-flex rounded-md border px-2 py-1 text-[11px] font-medium ${
                              canOpen
                                ? "border-gray-300 bg-white text-gray-700 hover:border-gray-400 hover:text-gray-950 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-300 dark:hover:text-white"
                                : "border-gray-200 text-gray-500 dark:border-gray-800 dark:text-gray-400"
                            }`}>
                              {label}
                            </span>
                          );
                          return (
                            <div key={`${evidence.page ?? ""}-${evidence.sourcePath ?? ""}-${evidence.nodeId ?? ""}-${evidenceIndex}`} className="text-xs text-gray-600 dark:text-gray-400">
                              <div className="flex flex-wrap items-center gap-2">
                                {canOpen ? (
                                  <button
                                    type="button"
                                    aria-label={`View ${label} evidence`}
                                    onClick={() => onOpenEvidence(evidence)}
                                  >
                                    {location}
                                  </button>
                                ) : location}
                                {evidence.description && evidence.description !== label && <span>{evidence.description}</span>}
                                {evidence.nodeId && <span className="font-mono text-[10px] text-gray-400">{evidence.nodeId}</span>}
                              </div>
                              {evidence.quote && (
                                <blockquote className="mt-1.5 border-l-2 border-gray-300 pl-2 text-[11px] italic text-gray-500 dark:border-gray-700 dark:text-gray-400">
                                  {evidence.quote}
                                </blockquote>
                              )}
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  )}
                  <input
                    aria-label={`Note for issue: ${issue.title}`}
                      value={ann?.note ?? ""}
                      onChange={(e) => setNote(issue.id, e.target.value)}
                      disabled={!annotationsLoaded}
                    placeholder="Add a note…"
                    className="mt-2 w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                  />
                </div>
              )}
            </div>
          );
        })}
        {visible.length === 0 && <p className="text-sm text-gray-500 dark:text-gray-400 px-2">No issues at this severity.</p>}
      </div>
      {!runId && (
        <p className="text-[11px] text-gray-500 dark:text-gray-400 mt-4">Annotations aren't saved for this view (no saved report).</p>
      )}
    </div>
  );
}

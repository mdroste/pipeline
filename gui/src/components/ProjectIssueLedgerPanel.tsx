import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type {
  Project,
  ProjectIssue,
  ProjectIssueEvidence,
  ProjectIssueLedger,
  ProjectIssueOccurrence,
  ProjectIssueStatus,
  RunSummary,
} from "../lib/types";
import type { ArtifactSelectionTarget } from "./ArtifactExplorer";

interface Props {
  project: Project;
  runs: RunSummary[];
  onOpenRun: (runId: string, source?: ArtifactSelectionTarget) => void;
}

const STATUS_ORDER: Record<ProjectIssueStatus, number> = {
  regressed: 0,
  open: 1,
  addressed: 2,
  dismissed: 3,
};

const STATUS_STYLES: Record<ProjectIssueStatus, string> = {
  open: "bg-blue-50 text-blue-700 dark:bg-blue-950/40 dark:text-blue-300",
  addressed: "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300",
  dismissed: "bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300",
  regressed: "bg-amber-50 text-amber-800 dark:bg-amber-950/40 dark:text-amber-300",
};

function formatDate(value: string, withTime = false): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString(undefined, withTime
    ? { year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }
    : { year: "numeric", month: "short", day: "numeric" });
}

function severityRank(value: string): number {
  switch (value.toLowerCase()) {
    case "high":
    case "critical":
    case "major":
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

function occurrenceLabel(occurrence: ProjectIssueOccurrence, run?: RunSummary): string {
  return run?.title || run?.input_name || occurrence.input_name || "Saved run";
}

function sourceLabel(evidence: ProjectIssueEvidence): string {
  const lines = evidence.line_start
    ? `:${evidence.line_start}${evidence.line_end && evidence.line_end !== evidence.line_start ? `–${evidence.line_end}` : ""}`
    : "";
  if (evidence.page) return `Page ${evidence.page}`;
  if (evidence.artifact_path) return `${evidence.artifact_path}${lines}`;
  if (evidence.description) return `${evidence.description}${lines}`;
  if (lines) return `Line ${lines.slice(1)}`;
  if (evidence.asset_id) return `Asset ${evidence.asset_id}`;
  if (evidence.node_id) return `Node ${evidence.node_id}`;
  return "Evidence";
}

function evidenceTarget(evidence: ProjectIssueEvidence): ArtifactSelectionTarget | undefined {
  if (!evidence.page && !evidence.artifact_path) return undefined;
  return { page: evidence.page, relPath: evidence.artifact_path };
}

function issueFirstObserved(issue: ProjectIssue): string {
  return issue.occurrences[0]?.observed_at || issue.created;
}

function issueLastObserved(issue: ProjectIssue): string {
  return issue.occurrences[issue.occurrences.length - 1]?.observed_at || issue.updated;
}

function exportEvidence(evidence: ProjectIssueEvidence): string {
  const references = [
    evidence.page ? `p. ${evidence.page}` : "",
    evidence.line_start
      ? `line ${evidence.line_start}${evidence.line_end && evidence.line_end !== evidence.line_start ? `–${evidence.line_end}` : ""}`
      : "",
    evidence.node_id ? `node ${evidence.node_id}` : "",
    evidence.asset_id ? `asset ${evidence.asset_id}` : "",
    evidence.artifact_path ? `artifact ${evidence.artifact_path}` : "",
  ].filter(Boolean);
  const location = references.join(", ") || evidence.description || "source";
  const description = evidence.description && evidence.description !== location
    ? ` — ${evidence.description}`
    : "";
  return `${location}${description}${evidence.quote ? ` (“${evidence.quote}”)` : ""}`;
}

function exportLedger(project: Project, issues: ProjectIssue[]): string {
  const body = issues.map((issue, issueIndex) => {
    const occurrences = issue.occurrences.map((occurrence) => {
      const evidence = occurrence.evidence.length
        ? `\n  - Evidence: ${occurrence.evidence.map(exportEvidence).join("; ")}`
        : "";
      const section = occurrence.section ? ` · ${occurrence.section}` : "";
      return `- ${formatDate(occurrence.observed_at)} · ${occurrence.input_name} · ${occurrence.profile_name}${section}\n  - ${occurrence.body.replaceAll("\n", " ")}${evidence}`;
    }).join("\n");
    return `## ${issueIndex + 1}. ${issue.title}\n\n- Status: ${issue.status}\n- Severity: ${issue.severity || "unspecified"}\n- First observed: ${formatDate(issueFirstObserved(issue))}\n- Last observed: ${formatDate(issueLastObserved(issue))}\n- Occurrences: ${issue.occurrences.length}${issue.section ? `\n- Current section/location: ${issue.section}` : ""}${issue.note ? `\n\n**Project note:** ${issue.note}` : ""}\n\n### Observations\n\n${occurrences}`;
  }).join("\n\n");
  return `# ${project.name} — Issue ledger\n\nExported ${formatDate(new Date().toISOString(), true)}. Lifecycle decisions are project-level; observations retain their original run provenance.\n\n${body || "No issues match the current filters."}\n`;
}

export default function ProjectIssueLedgerPanel({ project, runs, onOpenRun }: Props) {
  const [ledger, setLedger] = useState<ProjectIssueLedger | null>(null);
  const [loading, setLoading] = useState(true);
  const [pendingIssueId, setPendingIssueId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [noteDrafts, setNoteDrafts] = useState<Record<string, string>>({});
  const [mergeTargets, setMergeTargets] = useState<Record<string, string>>({});
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState("all");
  const [severityFilter, setSeverityFilter] = useState("all");
  const [workflowFilter, setWorkflowFilter] = useState("all");
  const [runFilter, setRunFilter] = useState("all");

  const installLedger = (next: ProjectIssueLedger) => {
    setLedger(next);
    setNoteDrafts((current) => Object.fromEntries(
      next.issues.map((issue) => [issue.id, current[issue.id] ?? issue.note]),
    ));
  };

  const refresh = async () => {
    setLoading(true);
    setError(null);
    try {
      installLedger(await invoke<ProjectIssueLedger>("sync_project_issue_ledger", {
        projectId: project.id,
      }));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    setLedger(null);
    setExpanded(new Set());
    setNoteDrafts({});
    setMergeTargets({});
    setWorkflowFilter("all");
    setRunFilter("all");
    void refresh();
  }, [project.id, project.run_ids.join("\0")]);

  const runsById = useMemo(
    () => new Map(runs.map((run) => [run.run_id, run])),
    [runs],
  );
  const workflows = useMemo(() => {
    const values = new Map<string, string>();
    ledger?.issues.forEach((issue) => issue.occurrences.forEach((occurrence) => {
      values.set(occurrence.profile_id, occurrence.profile_name || occurrence.profile_id);
    }));
    return [...values].sort((left, right) => left[1].localeCompare(right[1]));
  }, [ledger]);
  const severities = useMemo(() => [...new Set(
    ledger?.issues.map((issue) => issue.severity).filter(Boolean) ?? [],
  )].sort((left, right) => severityRank(left) - severityRank(right) || left.localeCompare(right)), [ledger]);

  const visibleIssues = useMemo(() => {
    const query = search.trim().toLowerCase();
    return (ledger?.issues ?? [])
      .filter((issue) => statusFilter === "all" || issue.status === statusFilter)
      .filter((issue) => severityFilter === "all" || issue.severity === severityFilter)
      .filter((issue) => workflowFilter === "all" || issue.occurrences.some((occurrence) => occurrence.profile_id === workflowFilter))
      .filter((issue) => runFilter === "all" || issue.occurrences.some((occurrence) => occurrence.run_id === runFilter))
      .filter((issue) => !query || [
        issue.title,
        issue.section,
        issue.note,
        ...issue.occurrences.flatMap((occurrence) => [
          occurrence.body,
          occurrence.step_label,
          occurrence.profile_name,
          occurrence.input_name,
        ]),
      ].some((value) => value.toLowerCase().includes(query)))
      .sort((left, right) =>
        STATUS_ORDER[left.status] - STATUS_ORDER[right.status]
        || severityRank(left.severity) - severityRank(right.severity)
        || issueLastObserved(right).localeCompare(issueLastObserved(left)),
      );
  }, [ledger, runFilter, search, severityFilter, statusFilter, workflowFilter]);

  const counts = useMemo(() => {
    const values: Record<ProjectIssueStatus, number> = {
      open: 0,
      addressed: 0,
      dismissed: 0,
      regressed: 0,
    };
    ledger?.issues.forEach((issue) => { values[issue.status] += 1; });
    return values;
  }, [ledger]);

  const updateIssue = async (issue: ProjectIssue, status: string, note: string) => {
    if (pendingIssueId) return;
    setPendingIssueId(issue.id);
    setError(null);
    try {
      installLedger(await invoke<ProjectIssueLedger>("update_project_issue", {
        projectId: project.id,
        issueId: issue.id,
        status,
        note,
      }));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setPendingIssueId(null);
    }
  };

  const mergeIssue = async (issue: ProjectIssue) => {
    const targetIssueId = mergeTargets[issue.id];
    if (!targetIssueId || pendingIssueId) return;
    setPendingIssueId(issue.id);
    setError(null);
    try {
      installLedger(await invoke<ProjectIssueLedger>("merge_project_issues", {
        projectId: project.id,
        sourceIssueId: issue.id,
        targetIssueId,
      }));
      setExpanded((current) => {
        const next = new Set(current);
        next.delete(issue.id);
        next.add(targetIssueId);
        return next;
      });
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setPendingIssueId(null);
    }
  };

  const exportVisible = async () => {
    setError(null);
    try {
      const path = await save({
        defaultPath: `${project.name.replace(/[^a-z0-9]+/gi, "-").replace(/^-|-$/g, "").toLowerCase() || "project"}-issues.md`,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (path) {
        await invoke("save_text_file", { path, content: exportLedger(project, visibleIssues) });
      }
    } catch (caught) {
      setError(`Issue-ledger export failed: ${caught instanceof Error ? caught.message : String(caught)}`);
    }
  };

  if (loading && !ledger) {
    return <div className="rounded-xl border border-gray-200 px-4 py-10 text-center text-sm text-gray-500 dark:border-gray-800 dark:text-gray-400">Refreshing issue ledger…</div>;
  }

  return (
    <section>
      <div className="flex flex-wrap items-start gap-4">
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-semibold text-gray-900 dark:text-gray-100">Issue ledger</h2>
          <p className="mt-1 max-w-3xl text-xs leading-5 text-gray-500 dark:text-gray-400">
            Findings are matched conservatively across structured reports. Project decisions persist; a missing finding is never treated as addressed automatically. The ledger works the same way for documents, source trees, and input-free reports.
          </p>
        </div>
        <div className="flex gap-2">
          <button type="button" disabled={loading} onClick={() => void refresh()} className="rounded-lg border border-gray-300 px-3 py-1.5 text-xs text-gray-600 disabled:opacity-40 dark:border-gray-700 dark:text-gray-300">
            {loading ? "Refreshing…" : "Refresh"}
          </button>
          <button type="button" disabled={!ledger || visibleIssues.length === 0} onClick={() => void exportVisible()} className="rounded-lg border border-gray-300 px-3 py-1.5 text-xs text-gray-600 disabled:opacity-40 dark:border-gray-700 dark:text-gray-300">
            Export visible
          </button>
        </div>
      </div>

      {error && <div role="alert" className="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300">{error}</div>}
      {ledger && ledger.warnings.length > 0 && (
        <details className="mt-4 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-300">
          <summary>{ledger.warnings.length} run{ledger.warnings.length === 1 ? " was" : "s were"} not fully included</summary>
          <ul className="mt-2 list-disc space-y-1 pl-5">{ledger.warnings.map((warning, index) => <li key={`${index}-${warning}`}>{warning}</li>)}</ul>
        </details>
      )}

      <div className="mt-5 grid gap-2 sm:grid-cols-4">
        {(["open", "regressed", "addressed", "dismissed"] as const).map((status) => (
          <button key={status} type="button" onClick={() => setStatusFilter(statusFilter === status ? "all" : status)} className={`rounded-xl border px-3 py-2.5 text-left transition-colors ${statusFilter === status ? "border-gray-500 ring-1 ring-gray-400" : "border-gray-200 dark:border-gray-800"}`}>
            <span className="block text-[10px] font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">{status}</span>
            <span className="mt-1 block text-xl font-semibold text-gray-950 dark:text-gray-50">{counts[status]}</span>
          </button>
        ))}
      </div>

      <div className="mt-4 grid gap-2 lg:grid-cols-[minmax(12rem,1fr)_auto_auto_auto_auto]">
        <input aria-label="Search project issues" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search titles, locations, notes, or report text…" className="rounded-lg border border-gray-300 bg-white px-3 py-2 text-xs dark:border-gray-700 dark:bg-gray-900" />
        <select aria-label="Issue status filter" value={statusFilter} onChange={(event) => setStatusFilter(event.target.value)} className="rounded-lg border border-gray-300 bg-white px-2 py-2 text-xs dark:border-gray-700 dark:bg-gray-900">
          <option value="all">All statuses</option>
          <option value="open">Open</option><option value="regressed">Regressed</option><option value="addressed">Addressed</option><option value="dismissed">Dismissed</option>
        </select>
        <select aria-label="Issue severity filter" value={severityFilter} onChange={(event) => setSeverityFilter(event.target.value)} className="rounded-lg border border-gray-300 bg-white px-2 py-2 text-xs dark:border-gray-700 dark:bg-gray-900">
          <option value="all">All severities</option>
          {severities.map((severity) => <option key={severity} value={severity}>{severity}</option>)}
        </select>
        <select aria-label="Issue workflow filter" value={workflowFilter} onChange={(event) => setWorkflowFilter(event.target.value)} className="rounded-lg border border-gray-300 bg-white px-2 py-2 text-xs dark:border-gray-700 dark:bg-gray-900">
          <option value="all">All workflows</option>
          {workflows.map(([id, name]) => <option key={id} value={id}>{name}</option>)}
        </select>
        <select aria-label="Issue run filter" value={runFilter} onChange={(event) => setRunFilter(event.target.value)} className="max-w-56 rounded-lg border border-gray-300 bg-white px-2 py-2 text-xs dark:border-gray-700 dark:bg-gray-900">
          <option value="all">All runs</option>
          {project.run_ids.map((runId) => {
            const run = runsById.get(runId);
            return <option key={runId} value={runId}>{run?.title || run?.input_name || runId}</option>;
          })}
        </select>
      </div>

      <div className="mt-4 space-y-3">
        {visibleIssues.map((issue) => {
          const isExpanded = expanded.has(issue.id);
          const latest = issue.occurrences[issue.occurrences.length - 1];
          const decisionValue = issue.decision_updated ? issue.status : "automatic";
          return (
            <article key={issue.id} className="overflow-hidden rounded-xl border border-gray-200 dark:border-gray-800">
              <button type="button" aria-label={`Review ${issue.title}`} aria-expanded={isExpanded} onClick={() => setExpanded((current) => {
                const next = new Set(current);
                if (next.has(issue.id)) next.delete(issue.id); else next.add(issue.id);
                return next;
              })} className="flex w-full items-start gap-4 px-4 py-3 text-left hover:bg-gray-50 dark:hover:bg-gray-900/50">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className={`rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase ${STATUS_STYLES[issue.status]}`}>{issue.status}</span>
                    {issue.severity && <span className="text-[10px] font-medium uppercase text-gray-500 dark:text-gray-400">{issue.severity}</span>}
                    {issue.section && <span className="truncate text-[11px] text-gray-500 dark:text-gray-400">{issue.section}</span>}
                  </div>
                  <h3 className="mt-1.5 text-sm font-medium text-gray-950 dark:text-gray-50">{issue.title}</h3>
                  <p className="mt-1 text-[11px] text-gray-500 dark:text-gray-400">
                    {issue.occurrences.length} observation{issue.occurrences.length === 1 ? "" : "s"} · first {formatDate(issueFirstObserved(issue))} · last {formatDate(issueLastObserved(issue))}{latest ? ` · ${latest.profile_name}` : ""}
                  </p>
                </div>
                <span className="mt-1 text-xs text-gray-400">{isExpanded ? "Hide" : "Review"}</span>
              </button>

              {isExpanded && (
                <div className="border-t border-gray-200 bg-gray-50/60 px-4 py-4 dark:border-gray-800 dark:bg-gray-900/30">
                  <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_15rem]">
                    <div>
                      <label htmlFor={`project-issue-note-${issue.id}`} className="text-[11px] font-medium text-gray-700 dark:text-gray-300">Project note or decision rationale</label>
                      <textarea id={`project-issue-note-${issue.id}`} value={noteDrafts[issue.id] ?? issue.note} onChange={(event) => setNoteDrafts((current) => ({ ...current, [issue.id]: event.target.value }))} rows={3} className="mt-1 w-full resize-y rounded-lg border border-gray-300 bg-white px-3 py-2 text-xs dark:border-gray-700 dark:bg-gray-950" placeholder="This note persists across future runs." />
                      <button type="button" disabled={pendingIssueId !== null || (noteDrafts[issue.id] ?? issue.note) === issue.note} onClick={() => void updateIssue(issue, decisionValue, noteDrafts[issue.id] ?? issue.note)} className="mt-2 rounded-lg border border-gray-300 px-3 py-1.5 text-xs disabled:opacity-40 dark:border-gray-700">Save note</button>
                    </div>
                    <div className="space-y-3">
                      <label className="block text-[11px] font-medium text-gray-700 dark:text-gray-300">
                        Lifecycle
                        <select aria-label={`Lifecycle for ${issue.title}`} value={decisionValue} disabled={pendingIssueId !== null} onChange={(event) => void updateIssue(issue, event.target.value, noteDrafts[issue.id] ?? issue.note)} className="mt-1 block w-full rounded-lg border border-gray-300 bg-white px-2 py-2 text-xs dark:border-gray-700 dark:bg-gray-950">
                          <option value="automatic">Follow run annotations</option>
                          <option value="open">Open</option><option value="addressed">Addressed</option><option value="dismissed">Dismissed</option><option value="regressed">Regressed</option>
                        </select>
                      </label>
                      <p className="text-[10px] leading-4 text-gray-500 dark:text-gray-400">{issue.decision_updated ? `Project decision updated ${formatDate(issue.decision_updated, true)}.` : "Derived from accept/reject/done annotations on its run occurrences."}</p>
                      {ledger && ledger.issues.length > 1 && (
                        <div>
                          <label className="text-[11px] font-medium text-gray-700 dark:text-gray-300" htmlFor={`merge-${issue.id}`}>Combine duplicate into</label>
                          <select id={`merge-${issue.id}`} value={mergeTargets[issue.id] ?? ""} onChange={(event) => setMergeTargets((current) => ({ ...current, [issue.id]: event.target.value }))} className="mt-1 block w-full rounded-lg border border-gray-300 bg-white px-2 py-2 text-xs dark:border-gray-700 dark:bg-gray-950">
                            <option value="">Choose issue…</option>
                            {ledger.issues.filter((candidate) => candidate.id !== issue.id).map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.title}</option>)}
                          </select>
                          <button type="button" disabled={!mergeTargets[issue.id] || pendingIssueId !== null} onClick={() => void mergeIssue(issue)} className="mt-2 rounded-lg border border-gray-300 px-3 py-1.5 text-xs disabled:opacity-40 dark:border-gray-700">Combine</button>
                        </div>
                      )}
                    </div>
                  </div>

                  <div className="mt-5 space-y-3">
                    <h4 className="text-[11px] font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">Run observations</h4>
                    {[...issue.occurrences].reverse().map((occurrence) => {
                      const run = runsById.get(occurrence.run_id);
                      return (
                        <div key={occurrence.key} className="rounded-lg border border-gray-200 bg-white p-3 dark:border-gray-800 dark:bg-gray-950">
                          <div className="flex flex-wrap items-start gap-2">
                            <div className="min-w-0 flex-1">
                              <p className="truncate text-xs font-medium text-gray-900 dark:text-gray-100">{occurrenceLabel(occurrence, run)}</p>
                              <p className="mt-0.5 text-[10px] text-gray-500 dark:text-gray-400">{formatDate(occurrence.observed_at, true)} · {occurrence.profile_name} · {occurrence.step_label}{occurrence.section ? ` · ${occurrence.section}` : ""}</p>
                            </div>
                            <button type="button" onClick={() => onOpenRun(occurrence.run_id)} className="rounded-md border border-gray-300 px-2 py-1 text-[10px] dark:border-gray-700">Open run</button>
                          </div>
                          {occurrence.body && <p className="mt-2 whitespace-pre-wrap text-xs leading-5 text-gray-600 dark:text-gray-400">{occurrence.body}</p>}
                          {occurrence.annotation_status && <p className="mt-2 text-[10px] text-gray-500 dark:text-gray-400">Run annotation: {occurrence.annotation_status}{occurrence.annotation_note ? ` — ${occurrence.annotation_note}` : ""}</p>}
                          {occurrence.evidence.length > 0 && (
                            <div className="mt-2 flex flex-wrap gap-1.5">
                              {occurrence.evidence.map((evidence, index) => {
                                const target = evidenceTarget(evidence);
                                const label = sourceLabel(evidence);
                                return target ? (
                                  <button key={`${label}-${index}`} type="button" aria-label={`Open ${label} from ${occurrenceLabel(occurrence, run)}`} onClick={() => onOpenRun(occurrence.run_id, target)} className="rounded-md bg-gray-100 px-2 py-1 text-[10px] text-gray-700 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700">{label}</button>
                                ) : (
                                  <span key={`${label}-${index}`} title={evidence.description || evidence.quote} className="rounded-md bg-gray-100 px-2 py-1 text-[10px] text-gray-500 dark:bg-gray-800 dark:text-gray-400">{label}</span>
                                );
                              })}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}
            </article>
          );
        })}
        {!loading && visibleIssues.length === 0 && (
          <div className="rounded-xl border border-dashed border-gray-300 px-5 py-10 text-center dark:border-gray-700">
            <p className="text-sm text-gray-600 dark:text-gray-300">{ledger?.issues.length ? "No issues match these filters." : "No structured issues were found in this project's runs."}</p>
            <p className="mx-auto mt-1 max-w-xl text-xs leading-5 text-gray-500 dark:text-gray-400">Any workflow can contribute: its saved report only needs an issues-shaped JSON output. Narrative reports remain available as runs but are not converted into findings implicitly.</p>
          </div>
        )}
      </div>
    </section>
  );
}

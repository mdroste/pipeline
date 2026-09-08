import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  studioClient,
  type FindingPreview,
  type ResponseRecord,
  type ResponseDecision,
  type ReportComment,
  type ResponseExport,
} from "../../lib/studioClient";
import type { ProjectRecord } from "../../lib/projectClient";
import { workbenchClient } from "../../lib/workbenchClient";
import type { RunSummary } from "../../lib/types";
import {
  button,
  input,
  panel,
  Text,
  Select,
  Field,
  ErrorNotice,
  Inspect,
  exportText,
  useAction,
  type StudioProps,
} from "./shared";
const categories = [
  "observed_error",
  "unresolved_objection",
  "missing_robustness",
  "omitted_source",
  "unclear_exposition",
  "optional_extension",
];
export default function Responses({
  workspaceId,
  data,
  onRefresh,
  onDocument,
  onReviewHandoff,
}: StudioProps) {
  const [records, setRecords] = useState<ProjectRecord<ResponseRecord>[]>([]);
  const [selected, setSelected] = useState("");
  const [decision, setDecision] = useState<ResponseDecision | null>(null);
  const [checked, setChecked] = useState<string[]>([]);
  const [history, setHistory] = useState<unknown[]>([]);
  const [letter, setLetter] = useState<ResponseExport | null>(null);
  const [format, setFormat] = useState("markdown");
  const { error, busy, run, setError } = useAction();
  const [bridge, setBridge] = useState(false);
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [runId, setRunId] = useState("");
  const [findingPreview, setFindingPreview] = useState<FindingPreview | null>(
    null,
  );
  const [findingIds, setFindingIds] = useState<string[]>([]);
  const [report, setReport] = useState("");
  const [comments, setComments] = useState<ReportComment[]>([]);
  const [commentIds, setCommentIds] = useState<number[]>([]);
  const [changedAnchors, setChangedAnchors] = useState<string[]>([]);
  const [dependencies, setDependencies] = useState<string[]>([]);
  const refresh = async () =>
    setRecords(await studioClient.records(workspaceId, "response"));
  useEffect(() => {
    let disposed = false;
    void studioClient
      .records<ResponseRecord>(workspaceId, "response")
      .then((v) => {
        if (!disposed) setRecords(v);
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
    };
  }, [workspaceId, setError]);
  const current = records.find((r) => r.id === selected);
  const set = (
    key: keyof ResponseDecision,
    value: ResponseDecision[keyof ResponseDecision],
  ) => setDecision((d) => (d ? { ...d, [key]: value } : null));
  const loadPreview = (p: FindingPreview) => {
    setFindingPreview(p);
    setFindingIds(p.package.findings.map((f) => f.id));
  };
  const toggle = (list: string[], id: string) =>
    list.includes(id) ? list.filter((i) => i !== id) : [...list, id];
  return (
    <div className="space-y-5">
      <ErrorNotice error={error} />
      <div className="grid gap-4 lg:grid-cols-2">
        <section className={panel}>
          <h2 className="font-semibold">Import referee or editor comments</h2>
          <button
            className={button}
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const path = await open({
                  multiple: false,
                  directory: false,
                  filters: [
                    {
                      name: "Reports",
                      extensions: ["pdf", "docx", "md", "txt"],
                    },
                  ],
                });
                if (typeof path !== "string") return;
                const p = await workbenchClient.importPaper({
                  workspaceId,
                  paperId: null,
                  title: path.split(/[\\/]/).pop() ?? "Referee report",
                  role: "other",
                  path,
                  operationId: `report-${crypto.randomUUID()}`,
                });
                setReport(p.revision!.id);
                await onRefresh();
              })
            }
          >
            Import local report
          </button>
          <Select
            label="Report revision"
            value={report}
            onChange={(v) => {
              setReport(v);
              setComments([]);
            }}
            options={data.papers.flatMap((p) =>
              p.revision
                ? [{ value: p.revision.id, label: p.paper.title }]
                : [],
            )}
          />
          <button
            className={button}
            disabled={!report || busy}
            onClick={() =>
              void run(async () => {
                const c = await studioClient.report(workspaceId, report);
                setComments(c);
                setCommentIds(c.map((_, i) => i));
              })
            }
          >
            Preview paragraph split
          </button>
          <p className="text-xs text-gray-500">
            Correct numbering and select the comments to retain. Original text
            and byte locators stay attached.
          </p>
          <div className="max-h-72 space-y-2 overflow-auto">
            {comments.map((c, i) => (
              <div
                key={`${c.start}-${c.end}`}
                className="rounded border p-2 text-xs"
              >
                <div className="flex gap-2">
                  <input
                    aria-label={`Import comment ${i + 1}`}
                    type="checkbox"
                    checked={commentIds.includes(i)}
                    onChange={() =>
                      setCommentIds((old) =>
                        old.includes(i)
                          ? old.filter((v) => v !== i)
                          : [...old, i],
                      )
                    }
                  />
                  <Field label="Comment number">
                    <input
                      className={input}
                      value={c.number}
                      onChange={(e) =>
                        setComments((old) =>
                          old.map((v, j) =>
                            i === j ? { ...v, number: e.target.value } : v,
                          ),
                        )
                      }
                    />
                  </Field>
                </div>
                <p className="whitespace-pre-wrap">{c.text}</p>
              </div>
            ))}
          </div>
          {comments.length > 0 && (
            <button
              className={button}
              disabled={!commentIds.length || busy}
              onClick={() =>
                void run(async () => {
                  await studioClient.mutate(workspaceId, {
                    action: "importReport",
                    revisionId: report,
                    comments: comments.filter((_, i) => commentIds.includes(i)),
                  });
                  await refresh();
                  setComments([]);
                })
              }
            >
              Import selected comments
            </button>
          )}
        </section>
        <section className={panel}>
          <label className="flex gap-2 text-sm font-semibold">
            <input
              type="checkbox"
              checked={bridge}
              onChange={(e) => {
                setBridge(e.target.checked);
                if (!e.target.checked) {
                  setRuns([]);
                  setFindingPreview(null);
                }
              }}
            />
            Enable importing Workflow findings
          </label>
          {bridge && (
            <>
              <button
                className={button}
                disabled={busy}
                onClick={() =>
                  void run(async () =>
                    setRuns(await invoke<RunSummary[]>("list_runs")),
                  )
                }
              >
                Choose a completed Workflow run
              </button>
              <Select
                label="Workflow run"
                value={runId}
                onChange={setRunId}
                options={runs.map((r) => ({
                  value: r.run_id,
                  label: `${r.input_name} · ${r.run_id}`,
                }))}
              />
              <button
                className={button}
                disabled={!runId || busy}
                onClick={() =>
                  void run(async () =>
                    loadPreview(await studioClient.workflowFindings(runId)),
                  )
                }
              >
                Preview findings
              </button>
              <Field label="Or import a findings exchange JSON file">
                <input
                  type="file"
                  accept=".json"
                  onChange={(e) => {
                    const file = e.target.files?.[0];
                    if (file)
                      void run(async () => {
                        if (file.size > 4 * 1024 * 1024)
                          throw new Error("Package exceeds 4 MiB");
                        loadPreview(
                          await studioClient.findingPackage(
                            JSON.parse(await file.text()),
                          ),
                        );
                      });
                  }}
                />
              </Field>
              {findingPreview && (
                <>
                  <p className="text-xs">{findingPreview.notice}</p>
                  <div className="max-h-64 overflow-auto">
                    {findingPreview.package.findings.map((f) => (
                      <label
                        key={f.id}
                        className="flex gap-2 border-b py-2 text-xs"
                      >
                        <input
                          type="checkbox"
                          checked={findingIds.includes(f.id)}
                          onChange={() => setFindingIds((v) => toggle(v, f.id))}
                        />
                        <span>
                          {f.title}
                          <br />
                          {f.body}
                        </span>
                      </label>
                    ))}
                  </div>
                  <button
                    className={button}
                    disabled={!findingIds.length || busy}
                    onClick={() =>
                      void run(async () => {
                        await studioClient.mutate(workspaceId, {
                          action: "importFindings",
                          package: findingPreview.package,
                          selected: findingIds,
                        });
                        await refresh();
                      })
                    }
                  >
                    Import selected findings
                  </button>
                  <button
                    className={button}
                    onClick={() =>
                      void run(() =>
                        exportText(
                          JSON.stringify(
                            {
                              ...findingPreview.package,
                              findings: findingPreview.package.findings.filter(
                                (f) => findingIds.includes(f.id),
                              ),
                            },
                            null,
                            2,
                          ),
                          "pipeline-findings-v1.json",
                        ),
                      )
                    }
                  >
                    Export selected exchange package
                  </button>
                </>
              )}
            </>
          )}
          <p className="text-xs text-gray-500">
            Imported findings link to the original reports. Your existing decisions
            are preserved; edits here do not change the Workflow reports.
          </p>
        </section>
      </div>
      <section className={panel}>
        <h2 className="font-semibold">Response matrix</h2>
        <div className="overflow-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr>
                <th>Select</th>
                <th>Comment</th>
                <th>Category / severity</th>
                <th>Disposition</th>
                <th>Evidence</th>
              </tr>
            </thead>
            <tbody>
              {records.map((r) => (
                <tr key={r.id} className="border-t">
                  <td>
                    <input
                      aria-label={`Select response ${r.body.decision.number}`}
                      type="checkbox"
                      checked={checked.includes(r.id)}
                      onChange={() => setChecked((v) => toggle(v, r.id))}
                    />
                  </td>
                  <td className="max-w-sm p-2">
                    <button
                      className="text-left underline"
                      onClick={() => {
                        setSelected(r.id);
                        setDecision(r.body.decision);
                        setHistory([]);
                      }}
                    >
                      {r.body.decision.number}.{" "}
                      {r.body.source.finding?.title ??
                        r.body.source.comment?.text.slice(0, 150)}
                    </button>
                  </td>
                  <td>
                    {r.body.decision.category.replaceAll("_", " ")} ·{" "}
                    {r.body.decision.severity}
                  </td>
                  <td>{r.body.decision.disposition}</td>
                  <td>
                    {r.body.flags.length
                      ? `${r.body.flags.length} export flag(s)`
                      : "Links recorded"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {current && decision && (
          <div className="grid gap-4 lg:grid-cols-2">
            <div className={panel}>
              <Text
                label="Stable comment number"
                value={decision.number}
                onChange={(s) => set("number", s)}
              />
              <Select
                label="Category"
                value={decision.category}
                onChange={(s) => set("category", s)}
                options={categories.map((v) => ({
                  value: v,
                  label: v.replaceAll("_", " "),
                }))}
              />
              <Select
                label="Severity"
                value={decision.severity}
                onChange={(s) => set("severity", s)}
                options={["unspecified", "high", "medium", "low"].map((v) => ({
                  value: v,
                  label: v,
                }))}
              />
              <Select
                label="Disposition"
                value={decision.disposition}
                onChange={(s) => set("disposition", s)}
                options={[
                  "open",
                  "investigating",
                  "deferred",
                  "rejected",
                  "addressed",
                ].map((v) => ({ value: v, label: v }))}
              />
              <Text
                label="Intended response / interpretation"
                multiline
                value={decision.intendedResponse}
                onChange={(s) => set("intendedResponse", s)}
              />
              <Text
                label="Draft response"
                multiline
                value={decision.draft}
                onChange={(s) => set("draft", s)}
              />
              <Text
                label="Decision rationale"
                multiline
                value={decision.rationale}
                onChange={(s) => set("rationale", s)}
              />
              <label className="flex gap-2 text-xs">
                <input
                  type="checkbox"
                  checked={decision.reportsAnalysisAdded}
                  onChange={(e) =>
                    set("reportsAnalysisAdded", e.target.checked)
                  }
                />
                This response says an analysis was added
              </label>
              <button
                className={button}
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await studioClient.mutate(workspaceId, {
                      action: "saveResponse",
                      id: current.id,
                      expectedRevision: current.revision,
                      response: decision,
                    });
                    await refresh();
                  })
                }
              >
                Save response decision
              </button>
            </div>
            <div className={panel}>
              <Select
                label="Revision task"
                value={decision.taskId ?? ""}
                onChange={(s) => set("taskId", s || null)}
                options={data.tasks.map((t) => ({
                  value: t.id,
                  label: t.body.objective,
                }))}
              />
              <Select
                label="Exact manuscript revision"
                value={decision.manuscriptRevisionId ?? ""}
                onChange={(s) => set("manuscriptRevisionId", s || null)}
                options={data.papers.flatMap((p) =>
                  p.revision
                    ? [
                        {
                          value: p.revision.id,
                          label: `${p.paper.title} · ${p.revision.contentHash.slice(0, 8)}`,
                        },
                      ]
                    : [],
                )}
              />
              {decision.manuscriptRevisionId && (
                <button
                  className={button}
                  onClick={() => onDocument(decision.manuscriptRevisionId!)}
                >
                  Open linked manuscript
                </button>
              )}
              <Select
                label="Accepted manuscript change"
                value={decision.applicationId ?? ""}
                onChange={(s) => set("applicationId", s || null)}
                options={data.applications
                  .filter((a) => a.body.state === "applied")
                  .map((a) => ({ value: a.id, label: a.id }))}
              />
              <Select
                label="Completed computation / check"
                value={decision.executionId ?? ""}
                onChange={(s) => set("executionId", s || null)}
                options={data.executions
                  .filter((e) => e.outcome === "completed")
                  .map((e) => ({ value: e.id, label: e.command.join(" ") }))}
              />
              <Field label="Supporting or rejecting evidence">
                <div className="max-h-40 overflow-auto">
                  {data.anchors.map((a) => (
                    <label key={a.id} className="flex gap-2">
                      <input
                        type="checkbox"
                        checked={decision.evidenceAnchorIds.includes(a.id)}
                        onChange={() =>
                          set(
                            "evidenceAnchorIds",
                            toggle(decision.evidenceAnchorIds, a.id),
                          )
                        }
                      />
                      {a.body.selection.quote.slice(0, 130) || a.body.body}
                    </label>
                  ))}
                </div>
              </Field>
              <Text
                label="Disputed premise"
                multiline
                value={decision.disputedPremise}
                onChange={(s) => set("disputedPremise", s)}
              />
              <Text
                label="Counterargument"
                multiline
                value={decision.counterargument}
                onChange={(s) => set("counterargument", s)}
              />
              <Text
                label="Possible resolving check"
                multiline
                value={decision.resolvingCheck}
                onChange={(s) => set("resolvingCheck", s)}
              />
              <Inspect
                value={current.body.source}
                label="Imported source"
              />
              <button
                className={button}
                onClick={() =>
                  void run(async () =>
                    setHistory(
                      await studioClient.history(workspaceId, current.id),
                    ),
                  )
                }
              >
                Load decision history
              </button>
              {history.length > 0 && (
                <Inspect
                  value={history}
                  label="Prior source versions and decisions"
                />
              )}
            </div>
          </div>
        )}
      </section>
      <section className={panel}>
        <h2 className="font-semibold">Response-letter export</h2>
        <Select
          label="Letter format"
          value={format}
          onChange={setFormat}
          options={[
            { value: "markdown", label: "Markdown" },
            { value: "latex", label: "LaTeX" },
          ]}
        />
        <button
          className={button}
          disabled={!checked.length || busy}
          onClick={() =>
            void run(async () =>
              setLetter(
                await studioClient.exportResponses(
                  workspaceId,
                  checked,
                  format,
                ),
              ),
            )
          }
        >
          Preview selected responses and check links
        </button>
        {letter && (
          <>
            {letter.warnings.map((w, i) => (
              <p key={i} role="status" className="text-xs text-amber-700">
                {w}
              </p>
            ))}
            <p className="text-xs">{letter.assessment}</p>
            <pre className="max-h-64 overflow-auto whitespace-pre-wrap text-xs">
              {letter.text}
            </pre>
            <button
              className={button}
              onClick={() =>
                void run(() =>
                  exportText(
                    `${letter.draft ? (letter.format === "latex" ? "% DRAFT: unresolved export flags\n" : "DRAFT — unresolved export flags\n\n") : ""}${letter.text}`,
                    `response-letter${letter.draft ? "-DRAFT" : ""}.${letter.format === "latex" ? "tex" : "md"}`,
                  ),
                )
              }
            >
              Export {letter.draft ? "flagged draft" : "letter"}
            </button>
          </>
        )}
      </section>
      {bridge && onReviewHandoff && (
        <section className={panel}>
          <h2 className="font-semibold">Focused re-review</h2>
          <p className="text-xs">
            Choose changed passages and their dependencies. The coverage
            manifest goes to the normal Workflow preview; this does not launch a
            review.
          </p>
          <div className="max-h-48 overflow-auto">
            {data.anchors.map((a) => (
              <div key={a.id} className="flex gap-3 py-1 text-xs">
                <label>
                  <input
                    type="checkbox"
                    checked={changedAnchors.includes(a.id)}
                    onChange={() => setChangedAnchors((v) => toggle(v, a.id))}
                  />{" "}
                  Changed
                </label>
                <label>
                  <input
                    type="checkbox"
                    checked={dependencies.includes(a.id)}
                    onChange={() => setDependencies((v) => toggle(v, a.id))}
                  />{" "}
                  Dependency
                </label>
                <span>
                  {a.body.selection.quote.slice(0, 180) || a.body.body}
                </span>
              </div>
            ))}
          </div>
          <button
            className={button}
            disabled={!changedAnchors.length || busy}
            onClick={() =>
              void run(async () =>
                onReviewHandoff(
                  await studioClient.focusReview(
                    workspaceId,
                    changedAnchors,
                    dependencies,
                    checked,
                  ),
                ),
              )
            }
          >
            Prepare covered passages for Workflow preview
          </button>
        </section>
      )}
    </div>
  );
}

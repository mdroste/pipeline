import { useCallback, useEffect, useState } from "react";
import {
  CHECK_METHODS,
  DIRECTION_STATUSES,
  THEORY_KINDS,
  THEORY_STATUSES,
  checkCannotClaimGenerality,
  studioClient,
  type ResearchDirection,
  type TheoryCheck,
  type TheoryNote,
  type TheoryOverview,
} from "../../lib/studioClient";
import type { ProjectRecord } from "../../lib/projectClient";
import {
  button,
  input,
  panel,
  lines,
  Text,
  Select,
  Field,
  ErrorNotice,
  Inspect,
  useAction,
  type StudioProps,
} from "./shared";

const emptyNote: TheoryNote = {
  kind: "derivation",
  title: "",
  statement: "",
  body: "",
  assumptions: [],
  assumptionIds: [],
  anchorIds: [],
  relatedIds: [],
  unresolvedSteps: [],
  status: "open",
  rejectionReason: "",
  origin: "manual",
  promotions: [],
};
const emptyCheck = (theoryId: string): TheoryCheck => ({
  theoryId,
  method: "analytical_argument",
  outcome: "passed",
  summary: "",
  domain: "",
  tolerance: null,
  precision: null,
  executionId: null,
  recipeRunId: null,
  anchorIds: [],
  claimsGenerality: false,
  origin: "manual",
});
const emptyDirection: ResearchDirection = {
  question: "",
  mechanism: "",
  closestKnownWork: "",
  minimalModelOrData: "",
  firstDiscriminatingTest: "",
  likelyFailureMode: "",
  nextAction: "",
  status: "idea",
  taskId: null,
  theoryIds: [],
  dropReason: "",
};
const label = (s: string) => s.replaceAll("_", " ");
const opts = (values: readonly string[]) =>
  values.map((v) => ({ value: v, label: label(v) }));

function IdPicker({
  title,
  selected,
  choices,
  onChange,
}: {
  title: string;
  selected: string[];
  choices: { id: string; text: string }[];
  onChange: (ids: string[]) => void;
}) {
  if (!choices.length) return null;
  return (
    <fieldset className="max-h-40 space-y-1 overflow-auto rounded border p-2 text-xs dark:border-neutral-700">
      <legend className="px-1 text-gray-600 dark:text-gray-400">{title}</legend>
      {choices.map((c) => (
        <label key={c.id} className="flex gap-2">
          <input
            type="checkbox"
            checked={selected.includes(c.id)}
            onChange={(e) =>
              onChange(
                e.target.checked
                  ? [...selected, c.id]
                  : selected.filter((id) => id !== c.id),
              )
            }
          />
          <span className="min-w-0 flex-1 truncate">{c.text}</span>
        </label>
      ))}
    </fieldset>
  );
}

export default function Theory({
  workspaceId,
  data,
  onRefresh,
  onAnchors,
}: StudioProps) {
  const [overview, setOverview] = useState<TheoryOverview | null>(null);
  const [kindFilter, setKindFilter] = useState("");
  const [noteId, setNoteId] = useState("");
  const [note, setNote] = useState(emptyNote);
  const [checkId, setCheckId] = useState("");
  const [check, setCheck] = useState(emptyCheck(""));
  const [directionId, setDirectionId] = useState("");
  const [direction, setDirection] = useState(emptyDirection);
  const [task, setTask] = useState("");
  const [path, setPath] = useState("appendix.tex");
  const [afterLine, setAfterLine] = useState("");
  const [notice, setNotice] = useState("");
  const { error, busy, run, setError } = useAction();
  const load = useCallback(async () => {
    setOverview(await studioClient.theory(workspaceId));
  }, [workspaceId]);
  useEffect(() => {
    let disposed = false;
    void studioClient
      .theory(workspaceId)
      .then((o) => {
        if (!disposed) setOverview(o);
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
    };
  }, [workspaceId, setError]);
  const notes = overview?.notes ?? [];
  const current = notes.find((n) => n.id === noteId);
  const currentCheck = overview?.checks.find((c) => c.id === checkId);
  const currentDirection = overview?.directions.find(
    (d) => d.id === directionId,
  );
  const set = <K extends keyof TheoryNote>(key: K, value: TheoryNote[K]) =>
    setNote((n) => ({ ...n, [key]: value }));
  const setC = <K extends keyof TheoryCheck>(key: K, value: TheoryCheck[K]) =>
    setCheck((c) => ({ ...c, [key]: value }));
  const setD = <K extends keyof ResearchDirection>(
    key: K,
    value: ResearchDirection[K],
  ) => setDirection((d) => ({ ...d, [key]: value }));
  const anchorText = (id: string) =>
    data.anchors.find((a) => a.id === id)?.body.selection.quote.slice(0, 160) ??
    id;
  const selectNote = (n: ProjectRecord<TheoryNote>) => {
    setNoteId(n.id);
    setNote(n.body);
    setCheckId("");
    setCheck(emptyCheck(n.id));
  };
  const proofPartner = (n: ProjectRecord<TheoryNote>) =>
    n.body.relatedIds
      .map((id) => notes.find((x) => x.id === id))
      .find((x) => x && x.body.anchorIds.length > 0);
  const numericalOnly = checkCannotClaimGenerality(check.method);
  const needsReason =
    note.status === "abandoned" || note.kind === "rejected_approach";
  return (
    <div className="space-y-5">
      <ErrorNotice error={error} />
      <section className={panel}>
        <h2 className="font-semibold">Theory notes</h2>
        <p className="text-xs text-gray-500">
          Working records for assumptions, conjectures, derivations, proof
          sketches, counterexamples and rejected approaches. Status is yours;
          the evidence label is computed from recorded checks. A numerical check
          covers tested instances only and is never a proof.
        </p>
        <div className="flex flex-wrap gap-2">
          <Select
            label="Filter by kind"
            value={kindFilter}
            onChange={setKindFilter}
            options={opts(THEORY_KINDS)}
            placeholder="All kinds"
          />
          <button
            className={button}
            onClick={() => {
              setNoteId("");
              setNote(emptyNote);
              setCheckId("");
              setCheck(emptyCheck(""));
            }}
          >
            New note
          </button>
        </div>
        <div className="overflow-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr>
                <th>Kind / title</th>
                <th>Status</th>
                <th>Evidence</th>
                <th>Action</th>
              </tr>
            </thead>
            <tbody>
              {notes
                .filter((n) => !kindFilter || n.body.kind === kindFilter)
                .map((n) => {
                  const partner = proofPartner(n);
                  return (
                    <tr className="border-t align-top" key={n.id}>
                      <td className="max-w-md p-2">
                        <span className="text-gray-500">
                          {label(n.body.kind)}
                        </span>{" "}
                        {n.body.title}
                        {n.body.unresolvedSteps.length > 0 && (
                          <p className="text-amber-700">
                            {n.body.unresolvedSteps.length} unresolved step(s)
                          </p>
                        )}
                        {n.body.status === "abandoned" && (
                          <p className="text-gray-500">
                            Reason: {n.body.rejectionReason.slice(0, 200)}
                          </p>
                        )}
                      </td>
                      <td className="p-2">
                        {n.body.status}
                        {n.body.origin === "proposed" && " · proposed"}
                      </td>
                      <td className="max-w-xs p-2">
                        {overview?.evidence[n.id]?.label}
                      </td>
                      <td className="space-x-1 p-2">
                        <button
                          className={button}
                          onClick={() => selectNote(n)}
                        >
                          Edit / checks
                        </button>
                        {onAnchors && n.body.anchorIds.length > 0 && (
                          <button
                            className={button}
                            onClick={() =>
                              onAnchors(
                                n.body.anchorIds[0],
                                partner?.body.anchorIds[0] ??
                                  n.body.anchorIds[1] ??
                                  null,
                              )
                            }
                          >
                            {partner
                              ? "Open statement and proof side by side"
                              : n.body.anchorIds.length > 1
                                ? "Open linked passages side by side"
                                : "Open linked passage"}
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
            </tbody>
          </table>
        </div>
        {!notes.length && (
          <p className="text-sm">
            No theory notes yet. Record an assumption or derivation below and
            link it to selected passages from Documents.
          </p>
        )}
      </section>
      <div className="grid gap-4 lg:grid-cols-2">
        <section className={panel}>
          <h2 className="font-semibold">
            {current ? "Edit theory note" : "New theory note"}
          </h2>
          <div className="grid grid-cols-2 gap-3">
            <Select
              label="Kind"
              value={note.kind}
              onChange={(v) => set("kind", v)}
              options={opts(THEORY_KINDS)}
            />
            <Select
              label="Status (your disposition)"
              value={note.status}
              onChange={(v) => set("status", v)}
              options={opts(THEORY_STATUSES)}
            />
          </div>
          <Text
            label="Title"
            value={note.title}
            onChange={(v) => set("title", v)}
          />
          <Text
            label="Statement"
            value={note.statement}
            onChange={(v) => set("statement", v)}
            multiline
          />
          <Field label="Derivation or argument (promotable prose)">
            <textarea
              aria-label="Derivation prose"
              className={input}
              rows={6}
              value={note.body}
              onChange={(e) => set("body", e.target.value)}
            />
          </Field>
          <Field label="Assumptions, one per line">
            <textarea
              aria-label="Assumptions"
              className={input}
              rows={2}
              value={note.assumptions.join("\n")}
              onChange={(e) => set("assumptions", lines(e.target.value))}
            />
          </Field>
          <IdPicker
            title="Linked assumption notes"
            selected={note.assumptionIds}
            choices={notes
              .filter((n) => n.body.kind === "assumption" && n.id !== noteId)
              .map((n) => ({ id: n.id, text: n.body.title }))}
            onChange={(ids) => set("assumptionIds", ids)}
          />
          <IdPicker
            title="Linked passages and equations (saved selections)"
            selected={note.anchorIds}
            choices={data.anchors.map((a) => ({
              id: a.id,
              text: a.body.selection.quote.slice(0, 160) || a.body.body,
            }))}
            onChange={(ids) => set("anchorIds", ids)}
          />
          <IdPicker
            title="Related notes (for a proposition, its proof or counterexample)"
            selected={note.relatedIds}
            choices={notes
              .filter((n) => n.id !== noteId)
              .map((n) => ({
                id: n.id,
                text: `${label(n.body.kind)}: ${n.body.title}`,
              }))}
            onChange={(ids) => set("relatedIds", ids)}
          />
          <Field label="Unresolved steps, one per line (retained; blocks “supported”)">
            <textarea
              aria-label="Unresolved steps"
              className={input}
              rows={2}
              value={note.unresolvedSteps.join("\n")}
              onChange={(e) => set("unresolvedSteps", lines(e.target.value))}
            />
          </Field>
          {needsReason && (
            <Text
              label="Why this approach was rejected or abandoned"
              value={note.rejectionReason}
              onChange={(v) => set("rejectionReason", v)}
              multiline
            />
          )}
          <Select
            label="Origin"
            value={note.origin}
            onChange={(v) => set("origin", v)}
            options={[
              { value: "manual", label: "Researcher-authored or reviewed" },
              {
                value: "proposed",
                label: "Proposed; awaiting review (stays open)",
              },
            ]}
          />
          <button
            className={button}
            disabled={busy || !note.title.trim() || !note.statement.trim()}
            onClick={() =>
              void run(async () => {
                const saved = await studioClient.mutate<
                  ProjectRecord<TheoryNote>
                >(workspaceId, {
                  action: "saveTheory",
                  id: noteId || null,
                  expectedRevision: current?.revision ?? 0,
                  note,
                });
                await load();
                setNoteId(saved.id);
                setNote({ ...emptyNote, ...saved.body });
                setCheck((c) => ({ ...c, theoryId: saved.id }));
                setNotice("Theory note saved.");
              })
            }
          >
            Save theory note
          </button>
          {current && (
            <Inspect label="Promotions and revision" value={current} />
          )}
        </section>
        <section className={panel}>
          <h2 className="font-semibold">Checks for the selected note</h2>
          {!current && (
            <p className="text-sm text-gray-500">
              Select a note to record analytical, symbolic, numerical, heuristic
              or model-assessment checks.
            </p>
          )}
          {current && (
            <>
              <ul className="space-y-1 text-xs">
                {overview?.checks
                  .filter((c) => c.body.check.theoryId === current.id)
                  .map((c) => (
                    <li
                      key={c.id}
                      className="flex flex-wrap gap-2 border-t pt-1"
                    >
                      <span className="min-w-0 flex-1">
                        {label(c.body.check.method)} · {c.body.check.outcome} ·{" "}
                        <span className="text-gray-500">{c.body.label}</span>
                      </span>
                      <button
                        className={button}
                        onClick={() => {
                          setCheckId(c.id);
                          setCheck(c.body.check);
                        }}
                      >
                        Edit
                      </button>
                    </li>
                  ))}
              </ul>
              <button
                className={button}
                onClick={() => {
                  setCheckId("");
                  setCheck(emptyCheck(current.id));
                }}
              >
                New check
              </button>
              <div className="grid grid-cols-2 gap-3">
                <Select
                  label="Method"
                  value={check.method}
                  onChange={(v) =>
                    setCheck((c) => ({
                      ...c,
                      method: v,
                      claimsGenerality: checkCannotClaimGenerality(v)
                        ? false
                        : c.claimsGenerality,
                    }))
                  }
                  options={opts(CHECK_METHODS)}
                />
                <Select
                  label="Outcome (did the check's own assertion hold?)"
                  value={check.outcome}
                  onChange={(v) => setC("outcome", v)}
                  options={opts(["passed", "failed", "inconclusive"])}
                />
              </div>
              <Text
                label="Summary"
                value={check.summary}
                onChange={(v) => setC("summary", v)}
                multiline
              />
              <Text
                label={
                  numericalOnly
                    ? "Domain of tested instances (required for numerical checks)"
                    : "Domain (optional)"
                }
                value={check.domain}
                onChange={(v) => setC("domain", v)}
              />
              <div className="grid grid-cols-2 gap-3">
                <Field label="Tolerance">
                  <input
                    aria-label="Tolerance"
                    className={input}
                    type="number"
                    step="any"
                    min={0}
                    value={check.tolerance ?? ""}
                    onChange={(e) =>
                      setC(
                        "tolerance",
                        e.target.value === "" ? null : Number(e.target.value),
                      )
                    }
                  />
                </Field>
                <Field label="Precision (digits)">
                  <input
                    aria-label="Precision"
                    className={input}
                    type="number"
                    min={0}
                    max={30}
                    value={check.precision ?? ""}
                    onChange={(e) =>
                      setC(
                        "precision",
                        e.target.value === "" ? null : Number(e.target.value),
                      )
                    }
                  />
                </Field>
              </div>
              <Select
                label="Completed execution (numerical evidence)"
                value={check.executionId ?? ""}
                onChange={(v) => setC("executionId", v || null)}
                options={data.executions
                  .filter((e) => e.outcome === "completed")
                  .map((e) => ({ value: e.id, label: e.id.slice(-12) }))}
                placeholder="None"
              />
              <IdPicker
                title="Passages this check refers to"
                selected={check.anchorIds}
                choices={data.anchors.map((a) => ({
                  id: a.id,
                  text: anchorText(a.id),
                }))}
                onChange={(ids) => setC("anchorIds", ids)}
              />
              <label className="flex gap-2 text-xs">
                <input
                  type="checkbox"
                  aria-label="Claims generality"
                  disabled={numericalOnly}
                  checked={check.claimsGenerality}
                  onChange={(e) => setC("claimsGenerality", e.target.checked)}
                />
                This check establishes the statement in general
                {numericalOnly && (
                  <span className="text-gray-500">
                    (unavailable: {label(check.method)} covers tested instances
                    or assessment only)
                  </span>
                )}
              </label>
              <button
                className={button}
                disabled={busy || !check.summary.trim()}
                onClick={() =>
                  void run(async () => {
                    await studioClient.mutate(workspaceId, {
                      action: "saveCheck",
                      id: checkId || null,
                      expectedRevision: currentCheck?.revision ?? 0,
                      check,
                    });
                    await load();
                    setNotice("Check recorded.");
                  })
                }
              >
                Record check
              </button>
              <h3 className="pt-4 text-sm font-semibold">
                Promote derivation prose into the manuscript
              </h3>
              <p className="text-xs">
                Inserts this note’s derivation text with a comment header that
                lists its assumptions and unresolved steps. The file is staged
                in a task copy for review in Changes; nothing else changes.
              </p>
              <Select
                label="Task change set"
                value={task}
                onChange={setTask}
                options={data.changes
                  .filter((c) => ["isolated", "review"].includes(c.body.state))
                  .map((c) => ({
                    value: c.id,
                    label:
                      data.tasks.find((t) => t.id === c.body.taskId)?.body
                        .objective ?? c.id,
                  }))}
              />
              <div className="grid grid-cols-2 gap-3">
                <Text
                  label="Manuscript file (.tex or .md)"
                  value={path}
                  onChange={setPath}
                />
                <Text
                  label="Insert after line (blank appends)"
                  value={afterLine}
                  onChange={setAfterLine}
                />
              </div>
              <button
                className={button}
                disabled={busy || !task || !current.body.body.trim()}
                onClick={() =>
                  void run(async () => {
                    const parsed = afterLine.trim()
                      ? Number.parseInt(afterLine, 10)
                      : null;
                    if (
                      parsed !== null &&
                      (!Number.isInteger(parsed) || parsed < 0)
                    )
                      throw new Error(
                        "Insert line must be a non-negative integer",
                      );
                    await studioClient.mutate(workspaceId, {
                      action: "promoteTheory",
                      checkpointId: task,
                      path,
                      theoryId: current.id,
                      afterLine: parsed,
                    });
                    await load();
                    await onRefresh();
                    setNotice(
                      "Derivation staged. Review and accept its captured task in Changes.",
                    );
                  })
                }
              >
                Stage derivation into manuscript
              </button>
            </>
          )}
          <p role="status" className="text-xs">
            {notice}
          </p>
        </section>
      </div>
      <section className={panel}>
        <h2 className="font-semibold">Research directions</h2>
        <p className="text-xs text-gray-500">
          A structured idea note: question, mechanism, closest known work,
          minimal model or data, first discriminating test, likely failure mode
          and next action. Compare ideas side by side and convert one into a
          task. There are no generated novelty or tractability scores.
        </p>
        <div className="overflow-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr>
                <th>Question</th>
                <th>Mechanism</th>
                <th>First discriminating test</th>
                <th>Likely failure mode</th>
                <th>Next action</th>
                <th>Status</th>
                <th>Action</th>
              </tr>
            </thead>
            <tbody>
              {overview?.directions.map((d) => (
                <tr className="border-t align-top" key={d.id}>
                  <td className="max-w-xs p-2">{d.body.question}</td>
                  <td className="max-w-xs p-2">{d.body.mechanism}</td>
                  <td className="max-w-xs p-2">
                    {d.body.firstDiscriminatingTest || (
                      <span className="text-amber-700">not stated</span>
                    )}
                  </td>
                  <td className="max-w-xs p-2">{d.body.likelyFailureMode}</td>
                  <td className="max-w-xs p-2">{d.body.nextAction}</td>
                  <td className="p-2">
                    {d.body.status}
                    {d.body.taskId && (
                      <p className="text-gray-500">
                        task{" "}
                        {data.tasks.find((t) => t.id === d.body.taskId)?.body
                          .objective ?? d.body.taskId}
                      </p>
                    )}
                  </td>
                  <td className="space-x-1 p-2">
                    <button
                      className={button}
                      onClick={() => {
                        setDirectionId(d.id);
                        setDirection(d.body);
                      }}
                    >
                      Edit
                    </button>
                    <button
                      className={button}
                      disabled={
                        busy ||
                        !d.body.firstDiscriminatingTest.trim() ||
                        ["converted", "dropped"].includes(d.body.status)
                      }
                      onClick={() =>
                        void run(async () => {
                          await studioClient.mutate(workspaceId, {
                            action: "convertDirection",
                            id: d.id,
                            expectedRevision: d.revision,
                          });
                          await load();
                          await onRefresh();
                          setNotice("Direction converted into an open task.");
                        })
                      }
                    >
                      Convert to task
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <button
          className={button}
          onClick={() => {
            setDirectionId("");
            setDirection(emptyDirection);
          }}
        >
          New direction
        </button>
        <div className="grid gap-3 lg:grid-cols-2">
          <Text
            label="Research question"
            value={direction.question}
            onChange={(v) => setD("question", v)}
            multiline
          />
          <Text
            label="Mechanism"
            value={direction.mechanism}
            onChange={(v) => setD("mechanism", v)}
            multiline
          />
          <Text
            label="Closest known work"
            value={direction.closestKnownWork}
            onChange={(v) => setD("closestKnownWork", v)}
            multiline
          />
          <Text
            label="Minimal model or data"
            value={direction.minimalModelOrData}
            onChange={(v) => setD("minimalModelOrData", v)}
            multiline
          />
          <Text
            label="First discriminating test"
            value={direction.firstDiscriminatingTest}
            onChange={(v) => setD("firstDiscriminatingTest", v)}
            multiline
          />
          <Text
            label="Likely failure mode"
            value={direction.likelyFailureMode}
            onChange={(v) => setD("likelyFailureMode", v)}
            multiline
          />
          <Text
            label="Next action"
            value={direction.nextAction}
            onChange={(v) => setD("nextAction", v)}
          />
          <Select
            label="Status"
            value={direction.status}
            onChange={(v) => setD("status", v)}
            options={opts(DIRECTION_STATUSES.filter((s) => s !== "converted"))}
          />
          {direction.status === "dropped" && (
            <Text
              label="Why this direction was dropped"
              value={direction.dropReason}
              onChange={(v) => setD("dropReason", v)}
              multiline
            />
          )}
        </div>
        <IdPicker
          title="Related theory notes"
          selected={direction.theoryIds}
          choices={notes.map((n) => ({
            id: n.id,
            text: `${label(n.body.kind)}: ${n.body.title}`,
          }))}
          onChange={(ids) => setD("theoryIds", ids)}
        />
        <button
          className={button}
          disabled={busy || !direction.question.trim()}
          onClick={() =>
            void run(async () => {
              const saved = await studioClient.mutate<
                ProjectRecord<ResearchDirection>
              >(workspaceId, {
                action: "saveDirection",
                id: directionId || null,
                expectedRevision: currentDirection?.revision ?? 0,
                direction,
              });
              await load();
              setDirectionId(saved.id);
              setDirection({ ...emptyDirection, ...saved.body });
              setNotice("Research direction saved.");
            })
          }
        >
          Save direction
        </button>
      </section>
    </div>
  );
}

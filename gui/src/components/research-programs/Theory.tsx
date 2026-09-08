import { useState } from "react";
import type { DeskRecord, OpenResearchObject } from "../../lib/deskClient";
import type { SymbolRecord } from "../../lib/programClient";
import {
  button,
  input,
  card,
  muted,
  Field,
  Inspect,
  ObjectSelect,
  Sources,
  operation,
  reference,
  exactRecord,
  lines,
  useProgram,
  type DeskProps,
} from "./shared";
const initial: SymbolRecord = {
  notation: "",
  scope: "global",
  definition: "",
  domain: "",
  units: "",
  aliases: [],
  sources: [],
};
export default function Theory(props: DeskProps) {
  const p = useProgram(props, ["symbol", "assumption_branch"]);
  const { workspaceId, onOpen } = props;
  const [symbol, setSymbol] = useState(initial),
    [prior, setPrior] = useState<string | null>(null),
    [collisions, setCollisions] = useState<unknown>(null),
    [candidateSource, setCandidateSource] = useState<OpenResearchObject | null>(
      null,
    ),
    [candidates, setCandidates] = useState<{
      candidates: { notation: string; start: number; end: number }[];
      notice: string;
    } | null>(null);
  const [theoryId, setTheoryId] = useState(""),
    [branchTitle, setBranchTitle] = useState("Alternative assumptions"),
    [assumptions, setAssumptions] = useState(""),
    [question, setQuestion] = useState("");
  const [method, setMethod] = useState("numerical_verification"),
    [outcome, setOutcome] = useState("passed"),
    [expression, setExpression] = useState(""),
    [domain, setDomain] = useState(""),
    [boundary, setBoundary] = useState(""),
    [tolerance, setTolerance] = useState("1e-8"),
    [summary, setSummary] = useState(""),
    [execution, setExecution] = useState("");
  const selected = p.choices?.theory.find((n) => n.id === theoryId);
  const symbols = (p.records.symbol ?? []) as DeskRecord<SymbolRecord>[];
  return (
    <div className="space-y-5">
      <section className={card}>
        <h2 className="font-semibold">Symbols and assumptions</h2>
        <p className={muted}>
          Definitions have a scope. Collision suggestions compare overlapping
          scopes; original source remains authoritative.
        </p>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Notation">
            <input
              className={input}
              value={symbol.notation}
              onChange={(e) =>
                setSymbol((s) => ({ ...s, notation: e.target.value }))
              }
            />
          </Field>
          <Field label="Scope">
            <input
              className={input}
              value={symbol.scope}
              onChange={(e) =>
                setSymbol((s) => ({ ...s, scope: e.target.value }))
              }
              placeholder="global or lemma / section name"
            />
          </Field>
          <Field label="Definition">
            <textarea
              className={input}
              value={symbol.definition}
              onChange={(e) =>
                setSymbol((s) => ({ ...s, definition: e.target.value }))
              }
            />
          </Field>
          <Field label="Domain">
            <input
              className={input}
              value={symbol.domain}
              onChange={(e) =>
                setSymbol((s) => ({ ...s, domain: e.target.value }))
              }
            />
          </Field>
          <Field label="Units">
            <input
              className={input}
              value={symbol.units}
              onChange={(e) =>
                setSymbol((s) => ({ ...s, units: e.target.value }))
              }
            />
          </Field>
          <Field label="Aliases (one per line)">
            <textarea
              className={input}
              value={symbol.aliases.join("\n")}
              onChange={(e) =>
                setSymbol((s) => ({
                  ...s,
                  aliases: e.target.value.split("\n"),
                }))
              }
            />
          </Field>
        </div>
        <Sources
          sources={symbol.sources}
          choices={p.choices?.objects ?? []}
          onChange={(sources) => setSymbol((s) => ({ ...s, sources }))}
          onOpen={onOpen}
        />
        <button
          className={button}
          disabled={p.busy || !symbol.notation || !symbol.definition}
          onClick={() =>
            void p.run(async () => {
              await p.call({
                action: "saveSymbol",
                symbol: { ...symbol, aliases: symbol.aliases.filter(Boolean) },
                supersedes: prior,
                operationId: operation(),
              });
              setPrior(null);
              setSymbol(initial);
            })
          }
        >
          Save notation revision
        </button>
        <button
          className={button}
          onClick={() =>
            void p.run(async () =>
              setCollisions(await p.call({ action: "symbols" })),
            )
          }
        >
          Find possible collisions
        </button>
        {collisions !== null && (
          <Inspect value={collisions} label="Potential notation conflicts" />
        )}
      </section>
      <section className={card}>
        <h3 className="font-semibold">Extract notation candidates</h3>
        <ObjectSelect
          choices={p.choices?.objects ?? []}
          value={candidateSource}
          onChange={setCandidateSource}
          label="Exact source to inspect"
        />
        <button
          className={button}
          disabled={!candidateSource || p.busy}
          onClick={() =>
            void p.run(async () =>
              setCandidates(
                await p.call({
                  action: "symbolCandidates",
                  source: candidateSource,
                }),
              ),
            )
          }
        >
          Inspect lexical candidates
        </button>
        {candidates && (
          <>
            <p className={muted}>{candidates.notice}</p>
            <div className="flex flex-wrap gap-1">
              {candidates.candidates.map((c) => (
                <button
                  key={c.start}
                  className={button}
                  onClick={() => {
                    setSymbol((s) => ({
                      ...s,
                      notation: c.notation,
                      sources: candidateSource ? [candidateSource] : [],
                    }));
                  }}
                >
                  {c.notation}
                </button>
              ))}
            </div>
          </>
        )}
      </section>
      {symbols.map((s) => (
        <section className={card} key={s.id}>
          <h3>
            {s.body.notation} · {s.body.scope}
          </h3>
          <p className="text-sm">{s.body.definition}</p>
          <p className={muted}>
            {s.body.domain} · {s.body.units}
          </p>
          <button className={button} onClick={() => onOpen(reference(s))}>
            Exact record and sources
          </button>
          <button
            className={button}
            onClick={() => {
              setSymbol(s.body);
              setPrior(s.id);
            }}
          >
            Revise definition
          </button>
        </section>
      ))}
      <section className={card}>
        <h3 className="font-semibold">Try a different assumption</h3>
        <Field label="Theory note">
          <select
            className={input}
            value={theoryId}
            onChange={(e) => {
              setTheoryId(e.target.value);
              const n = p.choices?.theory.find((t) => t.id === e.target.value);
              setAssumptions(n?.body.assumptions.join("\n") ?? "");
            }}
          >
            <option value="">Choose an existing theory note</option>
            {p.choices?.theory.map((n) => (
              <option key={n.id} value={n.id}>
                {n.body.title} · revision {n.revision}
              </option>
            ))}
          </select>
        </Field>
        {selected && (
          <>
            <button
              className={button}
              onClick={() => onOpen(exactRecord(selected))}
            >
              Original argument
            </button>
            <Inspect
              value={{
                assumptions: selected.body.assumptions,
                assumptionIds: selected.body.assumptionIds,
                relatedNotes: selected.body.relatedIds,
                unresolvedSteps: selected.body.unresolvedSteps,
              }}
              label="Argument dependencies"
            />
          </>
        )}
        <Field label="Branch title">
          <input
            className={input}
            value={branchTitle}
            onChange={(e) => setBranchTitle(e.target.value)}
          />
        </Field>
        <Field label="Alternative assumptions (one per line)">
          <textarea
            className={input}
            value={assumptions}
            onChange={(e) => setAssumptions(e.target.value)}
          />
        </Field>
        <Field label="Question for this branch">
          <textarea
            className={input}
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
          />
        </Field>
        <button
          className={button}
          disabled={p.busy || !selected || !question.trim()}
          onClick={() =>
            void p.run(async () => {
              if (selected)
                await p.call({
                  action: "assumptionBranch",
                  request: {
                    workspaceId,
                    source: exactRecord(selected),
                    title: branchTitle,
                    assumptions: lines(assumptions),
                    question,
                    operationId: operation(),
                  },
                });
            })
          }
        >
          Create an open theory branch
        </button>
      </section>
      <section className={card}>
        <h3 className="font-semibold">Record an executable assumption check</h3>
        <p className={muted}>
          Run a plan in Captured execution first, then link it here. The record
          includes the expression, assumptions, parameters, and results.
          Numerical checks apply only to the cases tested.
        </p>
        <div className="flex flex-wrap gap-2">
          {[
            "Limiting case",
            "Accounting identity",
            "Comparative statics",
            "Dimensional consistency",
            "Counterexample search",
          ].map((t) => (
            <button
              key={t}
              className={button}
              onClick={() => {
                setExpression(`${t}: `);
                setMethod(
                  t === "Counterexample search"
                    ? "numerical_counterexample"
                    : "numerical_verification",
                );
              }}
            >
              {t}
            </button>
          ))}
        </div>
        <Field label="Expression or property checked">
          <textarea
            className={input}
            value={expression}
            onChange={(e) => setExpression(e.target.value)}
          />
        </Field>
        <Field label="Domain and assumptions">
          <textarea
            className={input}
            value={domain}
            onChange={(e) => setDomain(e.target.value)}
          />
        </Field>
        <Field label="Boundary conditions">
          <textarea
            className={input}
            value={boundary}
            onChange={(e) => setBoundary(e.target.value)}
          />
        </Field>
        <div className="grid grid-cols-3 gap-2">
          <Field label="Method">
            <select
              className={input}
              value={method}
              onChange={(e) => setMethod(e.target.value)}
            >
              <option value="numerical_verification">
                Numerical verification
              </option>
              <option value="numerical_counterexample">Counterexample</option>
              <option value="symbolic_identity">Symbolic identity</option>
            </select>
          </Field>
          <Field label="Outcome">
            <select
              className={input}
              value={outcome}
              onChange={(e) => setOutcome(e.target.value)}
            >
              {["passed", "failed", "inconclusive"].map((v) => (
                <option key={v}>{v}</option>
              ))}
            </select>
          </Field>
          <Field label="Tolerance">
            <input
              className={input}
              value={tolerance}
              onChange={(e) => setTolerance(e.target.value)}
            />
          </Field>
        </div>
        <Field label="Completed captured execution">
          <select
            className={input}
            value={execution}
            onChange={(e) => setExecution(e.target.value)}
          >
            <option value="">Choose check execution</option>
            {p.choices?.executions
              .filter(
                (e) => e.outcome === "completed" && e.inputManifest.planId,
              )
              .map((e) => (
                <option key={e.id} value={e.id}>
                  {e.adapter} · {new Date(e.createdAt).toLocaleString()}
                </option>
              ))}
          </select>
        </Field>
        <Field label="Check interpretation and limitations">
          <textarea
            className={input}
            value={summary}
            onChange={(e) => setSummary(e.target.value)}
          />
        </Field>
        <button
          className={button}
          disabled={
            p.busy ||
            !selected ||
            !execution ||
            !summary ||
            !expression ||
            !boundary
          }
          onClick={() =>
            void p.run(async () => {
              if (selected)
                await p.call({
                  action: "theoryCheck",
                  request: {
                    workspaceId,
                    theory: exactRecord(selected),
                    executionId: execution,
                    expression,
                    boundaryConditions: boundary,
                    check: {
                      theoryId: selected.id,
                      method,
                      outcome,
                      summary,
                      domain,
                      tolerance: tolerance ? Number(tolerance) : null,
                      precision: null,
                      executionId: execution,
                      recipeRunId: null,
                      anchorIds: [],
                      claimsGenerality: false,
                      origin: "manual",
                    },
                    operationId: operation(),
                  },
                });
            })
          }
        >
          Retain check with its tested scope
        </button>
      </section>
      {p.records.assumption_branch?.map((r) => (
        <section className={card} key={r.id}>
          <h3>{r.title}</h3>
          <Inspect value={r.body} />
          <button className={button} onClick={() => onOpen(reference(r))}>
            Open branch evidence
          </button>
        </section>
      ))}
    </div>
  );
}

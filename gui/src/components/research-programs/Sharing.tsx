import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { DeskRecord, OpenResearchObject } from "../../lib/deskClient";
import type {
  Artifact,
  CapsuleManifest,
  CapsulePreview,
  CapsuleRequest,
  CapsuleSelection,
  Expected,
} from "../../lib/programClient";
import WorkspaceExchangePanel from "../WorkspaceExchangePanel";
import {
  button,
  input,
  card,
  muted,
  Field,
  Inspect,
  ObjectSelect,
  Sources,
  Artifacts,
  operation,
  lines,
  useProgram,
  type DeskProps,
} from "./shared";
export default function Sharing(props: DeskProps) {
  const p = useProgram(props, ["coauthor_review", "capsule_import"]);
  const { workspaceId } = props;
  const [title, setTitle] = useState("Coauthor review"),
    [questions, setQuestions] = useState(""),
    [changes, setChanges] = useState(""),
    [excluded, setExcluded] = useState(""),
    [sources, setSources] = useState<OpenResearchObject[]>([]),
    [commentSource, setCommentSource] = useState<OpenResearchObject | null>(
      null,
    ),
    [comment, setComment] = useState(""),
    [decision, setDecision] = useState(""),
    [comments, setComments] = useState<
      { source: OpenResearchObject; comment: string; decision: string }[]
    >([]);
  const [capsuleTitle, setCapsuleTitle] = useState("Replication materials"),
    [selections, setSelections] = useState<CapsuleSelection[]>([]),
    [expected, setExpected] = useState<Record<string, string>>({}),
    [preview, setPreview] = useState<CapsulePreview | null>(null),
    [inspected, setInspected] = useState<{
      path: string;
      preview: CapsulePreview;
    } | null>(null),
    [bound, setBound] = useState<string>(""),
    [capsule, setCapsule] = useState(""),
    [ordinal, setOrdinal] = useState(0),
    [profile, setProfile] = useState(""),
    [external, setExternal] = useState<Record<string, string>>({}),
    [result, setResult] = useState<unknown>(null),
    [execution, setExecution] = useState("");
  const imports = (p.records.capsule_import ?? []) as DeskRecord<{
    manifest?: CapsuleManifest;
    planId?: string;
  }>[];
  const selectedCapsule = imports.find((c) => c.id === capsule)?.body.manifest;
  const request = (): CapsuleRequest => ({
    workspaceId,
    title: capsuleTitle,
    selections: selections.map((s) => ({
      ...s,
      expected: JSON.parse(expected[s.planId] || "[]") as Expected[],
    })),
    path: "",
  });
  const update = (id: string, patch: Partial<CapsuleSelection>) => {
    setSelections((s) =>
      s.map((v) => (v.planId === id ? { ...v, ...patch } : v)),
    );
    setPreview(null);
  };
  return (
    <div className="space-y-5">
      <section className={card}>
        <h2 className="font-semibold">Prepare for coauthor review</h2>
        <p className={muted}>
          Write questions, select exact evidence, and record revision-specific
          comments. The readable brief becomes an ordinary handoff note for the
          existing .pwex exchange preview below.
        </p>
        <Field label="Brief title">
          <input
            className={input}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </Field>
        <Field label="Questions for the coauthor (one per line)">
          <textarea
            className={input}
            value={questions}
            onChange={(e) => setQuestions(e.target.value)}
          />
        </Field>
        <Field label="Change summary">
          <textarea
            className={input}
            value={changes}
            onChange={(e) => setChanges(e.target.value)}
          />
        </Field>
        <Field label="Excluded material and unresolved dependencies">
          <textarea
            className={input}
            value={excluded}
            onChange={(e) => setExcluded(e.target.value)}
          />
        </Field>
        <Sources
          sources={sources}
          choices={p.choices?.objects ?? []}
          onChange={setSources}
          onOpen={props.onOpen}
        />
        <details className="space-y-2">
          <summary>Comment on an exact revision</summary>
          <ObjectSelect
            choices={p.choices?.objects ?? []}
            value={commentSource}
            onChange={setCommentSource}
          />
          <Field label="Coauthor comment">
            <textarea
              className={input}
              value={comment}
              onChange={(e) => setComment(e.target.value)}
            />
          </Field>
          <Field label="Recorded decision or open question">
            <input
              className={input}
              value={decision}
              onChange={(e) => setDecision(e.target.value)}
            />
          </Field>
          <button
            className={button}
            disabled={!commentSource || !comment}
            onClick={() => {
              if (commentSource)
                setComments((old) => [
                  ...old,
                  { source: commentSource, comment, decision },
                ]);
              setComment("");
            }}
          >
            Add comment to brief
          </button>
          <Inspect value={comments} />
        </details>
        <button
          className={button}
          disabled={p.busy || !changes || !excluded}
          onClick={() =>
            void p.run(async () => {
              await p.call({
                action: "coauthor",
                title,
                brief: {
                  questions: lines(questions),
                  changeSummary: changes,
                  excludedMaterial: excluded,
                  sources,
                  comments,
                },
                supersedes: null,
                operationId: operation(),
              });
            })
          }
        >
          Create review brief
        </button>
      </section>
      {p.records.coauthor_review?.map((r) => (
        <section key={r.id} className={card}>
          <h3>{r.title}</h3>
          <p className={muted}>
            Select “Notes” in the exchange preview to carry this brief and its
            comments. Add the referenced source objects using the ordinary
            package selections.
          </p>
          <Artifacts
            workspaceId={workspaceId}
            recordId={r.id}
            artifacts={(r.body as { artifacts: Artifact[] }).artifacts}
            choices={p.choices}
            onError={props.onError}
          />
        </section>
      ))}
      <details className={card}>
        <summary className="font-semibold">
          Selective .pwex export, return import, and conflicts
        </summary>
        <WorkspaceExchangePanel
          workspaceId={workspaceId}
          sessionId={props.sessionId ?? null}
          busy={p.busy}
          onAction={(fn) => void p.run(fn)}
          onError={props.onError}
        />
      </details>
      <section className={card}>
        <h2 className="font-semibold">Replication capsule</h2>
        <p className={muted}>
          A separate research-capsule-v1 archive holds declared inputs, ordered
          plans, environment requirements, and optional numeric tolerances.
          Importing it grants no execution authority.
        </p>
        <Field label="Capsule title">
          <input
            className={input}
            value={capsuleTitle}
            onChange={(e) => {
              setCapsuleTitle(e.target.value);
              setPreview(null);
            }}
          />
        </Field>
        <Field label="Add captured plan in execution order">
          <select
            className={input}
            value=""
            onChange={(e) => {
              setSelections((old) => [
                ...old,
                {
                  planId: e.target.value,
                  includedPaths: [],
                  externalRequirements: {},
                  expected: [],
                },
              ]);
              setPreview(null);
            }}
          >
            <option value="">Choose a captured plan</option>
            {p.choices?.plans
              .filter((c) => !selections.some((s) => s.planId === c.id))
              .map((c) => (
                <option key={c.id} value={c.id}>
                  {c.title}
                </option>
              ))}
          </select>
        </Field>
        {selections.map((s, index) => {
          const plan = p.choices?.plans.find((c) => c.id === s.planId);
          return (
            <div className="space-y-3 rounded border p-3" key={s.planId}>
              <h3>
                {index + 1}. {plan?.title}
              </h3>
              <Inspect
                value={plan?.body}
                label="Review command, parameters, inputs, and toolchain"
              />
              {plan?.body.capturedFiles.map((f) => (
                <div key={f.path} className="space-y-1 text-xs">
                  <label>
                    <input
                      type="checkbox"
                      checked={s.includedPaths.includes(f.path)}
                      onChange={(e) =>
                        update(s.planId, {
                          includedPaths: e.target.checked
                            ? [...s.includedPaths, f.path]
                            : s.includedPaths.filter((p) => p !== f.path),
                        })
                      }
                    />{" "}
                    Include {f.path} · {f.hash.slice(0, 12)}
                  </label>
                  {!s.includedPaths.includes(f.path) && (
                    <input
                      className={input}
                      aria-label={`External requirement for ${f.path}`}
                      placeholder="Required: acquisition instructions or license restriction"
                      value={s.externalRequirements[f.path] ?? ""}
                      onChange={(e) =>
                        update(s.planId, {
                          externalRequirements: {
                            ...s.externalRequirements,
                            [f.path]: e.target.value,
                          },
                        })
                      }
                    />
                  )}
                </div>
              ))}
              <p className={muted}>
                Non-script data requires the project's package-data permission
                in Data & samples. Omitted required inputs produce an incomplete
                verification.
              </p>
              <details>
                <summary className="text-xs">
                  Expected numeric outputs and tolerances
                </summary>
                <p className={muted}>
                  Use JSON output paths and JSON pointers. Example:{" "}
                  {`[{"output":"results.json","pointer":"/estimate","value":1.25,"absoluteTolerance":1e-8,"relativeTolerance":1e-6}]`}
                </p>
                <textarea
                  className={input}
                  aria-label={`Numeric expectations for plan ${index + 1}`}
                  value={expected[s.planId] ?? "[]"}
                  onChange={(e) => {
                    setExpected((old) => ({
                      ...old,
                      [s.planId]: e.target.value,
                    }));
                    setPreview(null);
                  }}
                />
              </details>
              <button
                className={button}
                onClick={() => {
                  setSelections((old) =>
                    old.filter((v) => v.planId !== s.planId),
                  );
                  setPreview(null);
                }}
              >
                Remove plan
              </button>
            </div>
          );
        })}
        <button
          className={button}
          disabled={p.busy || !selections.length}
          onClick={() =>
            void p.run(async () =>
              setPreview(
                await p.call<CapsulePreview>({
                  action: "capsulePreview",
                  request: request(),
                }),
              ),
            )
          }
        >
          Preview replication materials
        </button>
        {preview && (
          <>
            <p className="text-sm">
              {(preview.includedBytes / 1048576).toFixed(2)} MiB of included
              inputs
            </p>
            <Inspect value={preview.manifest} label="Exact capsule manifest" />
            <button
              className={button}
              disabled={p.busy}
              onClick={() =>
                void p.run(async () => {
                  const path = await save({ defaultPath: "replication.pwrc" });
                  if (path)
                    await p.call({
                      action: "capsuleExport",
                      request: { ...request(), path },
                      fingerprint: preview.fingerprint,
                    });
                })
              }
            >
              Export this capsule
            </button>
          </>
        )}
      </section>
      <section className={card}>
        <h3 className="font-semibold">
          Inspect and import replication materials
        </h3>
        <button
          className={button}
          disabled={p.busy}
          onClick={() =>
            void p.run(async () => {
              const path = await open({
                multiple: false,
                filters: [
                  { name: "Research capsule", extensions: ["pwrc", "zip"] },
                ],
              });
              if (typeof path === "string")
                setInspected({
                  path,
                  preview: await p.call<CapsulePreview>({
                    action: "capsuleInspect",
                    path,
                  }),
                });
            })
          }
        >
          Inspect capsule archive
        </button>
        {inspected && (
          <>
            <Inspect
              value={inspected.preview.manifest}
              label="Incoming inert material"
            />
            <button
              className={button}
              disabled={p.busy}
              onClick={() =>
                void p.run(async () => {
                  const r = await p.call<DeskRecord>({
                    action: "capsuleImport",
                    path: inspected.path,
                    fingerprint: inspected.preview.fingerprint,
                    operationId: operation(),
                  });
                  setCapsule(r.id);
                })
              }
            >
              Import reviewed capsule as inert material
            </button>
          </>
        )}
        <Field label="Imported capsule">
          <select
            className={input}
            value={capsule}
            onChange={(e) => {
              setCapsule(e.target.value);
              setOrdinal(0);
              setExternal({});
            }}
          >
            <option value="">Choose imported material</option>
            {imports
              .filter((c) => c.body.manifest)
              .map((c) => (
                <option key={c.id} value={c.id}>
                  {c.title}
                </option>
              ))}
          </select>
        </Field>
        <Field label="Plan to bind">
          <select
            className={input}
            value={ordinal}
            onChange={(e) => {
              setOrdinal(Number(e.target.value));
              setExternal({});
            }}
          >
            {selectedCapsule?.plans.map((c, i) => (
              <option key={i} value={i}>
                {i + 1}. {c.label}
              </option>
            ))}
          </select>
        </Field>
        {selectedCapsule?.plans[ordinal] && (
          <Inspect
            value={selectedCapsule.plans[ordinal].environment}
            label="Required environment and package coverage"
          />
        )}
        <Field label="Local configured toolchain">
          <select
            className={input}
            value={profile}
            onChange={(e) => setProfile(e.target.value)}
          >
            <option value="">Choose recipient command profile</option>
            {p.choices?.profiles
              .filter((pr) => pr.adapter === "command")
              .map((pr) => (
                <option key={pr.id} value={pr.id}>
                  {pr.name}
                </option>
              ))}
          </select>
        </Field>
        {selectedCapsule?.plans[ordinal]?.inputs
          .filter((i) => !i.included)
          .map((i) => (
            <Field
              key={i.path}
              label={`Supply ${i.path} from a local captured input (exact hash required)`}
            >
              <select
                className={input}
                value={external[i.path] ?? ""}
                onChange={(e) =>
                  setExternal((old) => ({ ...old, [i.path]: e.target.value }))
                }
              >
                <option value="">Unavailable — report incomplete</option>
                {p.choices?.plans.flatMap((pr) =>
                  pr.body.capturedFiles
                    .filter((f) => f.hash === i.hash)
                    .map((f) => (
                      <option key={`${pr.id}:${f.path}`} value={f.artifactId}>
                        {pr.title}: {f.path}
                      </option>
                    )),
                )}
              </select>
            </Field>
          ))}
        <button
          className={button}
          disabled={p.busy || !capsule || !profile}
          onClick={() =>
            void p.run(async () => {
              const v = await p.call<{
                status: string;
                binding?: { id: string };
              }>({
                action: "capsuleBind",
                request: {
                  workspaceId,
                  capsuleId: capsule,
                  ordinal,
                  localProfileId: profile,
                  externalInputs: Object.fromEntries(
                    Object.entries(external).filter(([, v]) => v),
                  ),
                  operationId: operation(),
                },
              });
              setResult(v);
              if (v.binding) setBound(v.binding.id);
            })
          }
        >
          Prepare local verification plan
        </button>
        <p className={muted}>
          Open Captured execution to inspect, authorize, and test the prepared
          local plan. No imported plan is authorized automatically.
        </p>
        <Field label="Locally bound capsule">
          <select
            className={input}
            value={bound}
            onChange={(e) => setBound(e.target.value)}
          >
            <option value="">Choose local binding</option>
            {imports
              .filter((c) => c.body.planId)
              .map((c) => (
                <option key={c.id} value={c.id}>
                  {c.title}
                </option>
              ))}
          </select>
        </Field>
        <Field label="Verification execution">
          <select
            className={input}
            value={execution}
            onChange={(e) => setExecution(e.target.value)}
          >
            <option value="">Choose execution</option>
            {p.choices?.executions
              .filter(
                (e) =>
                  e.inputManifest.planId ===
                  imports.find((c) => c.id === bound)?.body.planId,
              )
              .map((e) => (
                <option key={e.id} value={e.id}>
                  {e.outcome} · {new Date(e.createdAt).toLocaleString()}
                </option>
              ))}
          </select>
        </Field>
        <button
          className={button}
          disabled={p.busy || !bound || !execution}
          onClick={() =>
            void p.run(async () =>
              setResult(
                await p.call({
                  action: "capsuleVerify",
                  bindingId: bound,
                  executionId: execution,
                }),
              ),
            )
          }
        >
          Check selected outputs and tolerances
        </button>
        {result !== null && (
          <Inspect
            value={result}
            label="Replication preparation / verification report"
          />
        )}
      </section>
    </div>
  );
}

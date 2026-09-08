import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import type { DeskRecord, OpenResearchObject } from "../../lib/deskClient";
import type {
  Deliverable,
  Kit,
  Outline,
  Publication,
  Section,
} from "../../lib/programClient";
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
  reference,
  useProgram,
  type DeskProps,
} from "./shared";
export default function Delivery(props: DeskProps) {
  const p = useProgram(props, [
    "kit_install",
    "deliverable",
    "publication_asset",
    "deliverable_role",
  ]);
  const [kits, setKits] = useState<Kit[]>([]),
    [kit, setKit] = useState<Kit | null>(null),
    [instructions, setInstructions] = useState(""),
    [outline, setOutline] = useState<Outline>({
      title: "Research memo",
      template: "memo",
      sections: [{ heading: "Question", text: "", sources: [], assets: [] }],
    }),
    [supersedes, setSupersedes] = useState<string | null>(null),
    [role, setRole] = useState("main"),
    [roleObject, setRoleObject] = useState<OpenResearchObject | null>(null);
  useEffect(() => {
    void p
      .call<Kit[]>({ action: "kits" })
      .then(setKits)
      .catch(() => undefined);
  }, [props.workspaceId]);
  const change = (i: number, v: Partial<Section>) =>
    setOutline((o) => ({
      ...o,
      sections: o.sections.map((s, j) => (i === j ? { ...s, ...v } : s)),
    }));
  const assets = (p.records.publication_asset ??
    []) as DeskRecord<Publication>[];
  const drafts = (p.records.deliverable ?? []) as DeskRecord<Deliverable>[];
  return (
    <div className="space-y-5">
      <section className={card}>
        <h2 className="font-semibold">Research starter kits</h2>
        <p className={muted}>
          Start with an editable outline and two concrete tasks. Kits work
          before a manuscript is imported and make no model call.
        </p>
        <div className="flex flex-wrap gap-2">
          {kits.map((k) => (
            <button
              className={button}
              key={k.id}
              onClick={() => {
                setKit(k);
                setInstructions(k.instructions);
              }}
            >
              {k.title}
            </button>
          ))}
        </div>
        {kit && (
          <div className="space-y-3 border-t pt-3">
            <h3>
              {kit.title} · version {kit.version}
            </h3>
            <Field label="Edit the kit instructions">
              <textarea
                className={input}
                value={instructions}
                onChange={(e) => setInstructions(e.target.value)}
              />
            </Field>
            <p className={muted}>
              Creates one ordinary outline note with these sections:
            </p>
            <ul className="list-inside list-disc text-sm">
              {kit.sections.map((s) => (
                <li key={s}>{s}</li>
              ))}
            </ul>
            <p className={muted}>Creates these research tasks:</p>
            <ul className="list-inside list-disc text-sm">
              {kit.tasks.map((t) => (
                <li key={t}>{t}</li>
              ))}
            </ul>
            {kit.recipeSuggestions.length > 0 && (
              <p className={muted}>
                Optional later actions: {kit.recipeSuggestions.join(", ")}
              </p>
            )}
            <button
              className={button}
              disabled={p.busy || !instructions.trim()}
              onClick={() =>
                void p.run(async () => {
                  await p.call({
                    action: "installKit",
                    kitId: kit.id,
                    instructions,
                    operationId: operation(),
                  });
                  setKit(null);
                })
              }
            >
              Install the previewed structure
            </button>
          </div>
        )}
      </section>
      <section className={card}>
        <h2 className="font-semibold">Deliverable assembly</h2>
        <p className={muted}>
          Arrange prose, exact source excerpts, and retained publication assets.
          Each generation creates a new Markdown and TeX draft for comparison
          and acceptance.
        </p>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Deliverable title">
            <input
              className={input}
              value={outline.title}
              onChange={(e) =>
                setOutline((o) => ({ ...o, title: e.target.value }))
              }
            />
          </Field>
          <Field label="Template">
            <select
              className={input}
              value={outline.template}
              onChange={(e) =>
                setOutline((o) => ({ ...o, template: e.target.value }))
              }
            >
              {["manuscript", "memo", "seminar", "grant", "teaching"].map(
                (v) => (
                  <option key={v}>{v}</option>
                ),
              )}
            </select>
          </Field>
        </div>
        {outline.sections.map((s, i) => (
          <div className="space-y-3 rounded border p-3" key={i}>
            <Field label={`Section ${i + 1} heading`}>
              <input
                className={input}
                value={s.heading}
                onChange={(e) => change(i, { heading: e.target.value })}
              />
            </Field>
            <Field label="Your section prose">
              <textarea
                className={input}
                rows={4}
                value={s.text}
                onChange={(e) => change(i, { text: e.target.value })}
              />
            </Field>
            <Sources
              choices={p.choices?.objects ?? []}
              sources={s.sources}
              onChange={(sources) => change(i, { sources })}
              onOpen={props.onOpen}
            />
            <Field label="Add publication asset">
              <select
                className={input}
                value=""
                onChange={(e) =>
                  change(i, { assets: [...s.assets, e.target.value] })
                }
              >
                <option value="">Choose table or figure</option>
                {assets
                  .filter((a) => !s.assets.includes(a.id))
                  .map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.title} · {a.body.spec.kind}
                    </option>
                  ))}
              </select>
            </Field>
            {s.assets.map((a) => (
              <div className="flex gap-2 text-xs" key={a}>
                <span>{assets.find((v) => v.id === a)?.title ?? a}</span>
                <button
                  className={button}
                  onClick={() =>
                    change(i, { assets: s.assets.filter((id) => id !== a) })
                  }
                >
                  Remove asset
                </button>
              </div>
            ))}
            <div className="flex gap-2">
              <button
                className={button}
                disabled={i === 0}
                onClick={() =>
                  setOutline((o) => {
                    const sections = [...o.sections];
                    [sections[i - 1], sections[i]] = [
                      sections[i],
                      sections[i - 1],
                    ];
                    return { ...o, sections };
                  })
                }
              >
                Move section up
              </button>
              <button
                className={button}
                disabled={outline.sections.length === 1}
                onClick={() =>
                  setOutline((o) => ({
                    ...o,
                    sections: o.sections.filter((_, j) => j !== i),
                  }))
                }
              >
                Remove section
              </button>
            </div>
          </div>
        ))}
        <button
          className={button}
          disabled={outline.sections.length >= 30}
          onClick={() =>
            setOutline((o) => ({
              ...o,
              sections: [
                ...o.sections,
                { heading: "New section", text: "", sources: [], assets: [] },
              ],
            }))
          }
        >
          Add section
        </button>
        <button
          className={button}
          disabled={p.busy || !outline.title || !outline.sections.length}
          onClick={() =>
            void p.run(async () => {
              const r = await p.call<DeskRecord<Deliverable>>({
                action: "assemble",
                outline,
                supersedes,
                operationId: operation(),
              });
              setSupersedes(r.id);
            })
          }
        >
          {supersedes
            ? "Generate a new draft version"
            : "Generate deliverable draft"}
        </button>
      </section>
      {drafts.map((d) => (
        <section className={card} key={d.id}>
          <h3 className="font-semibold">{d.title}</h3>
          <p className={muted}>
            {new Date(d.createdAt).toLocaleString()} · {d.body.outline.template}{" "}
            · {d.contentHash.slice(0, 12)}
          </p>
          <button
            className={button}
            onClick={() => {
              setOutline(d.body.outline);
              setSupersedes(d.id);
            }}
          >
            Revise as a new draft
          </button>
          <button className={button} onClick={() => props.onOpen(reference(d))}>
            Exact outline and source links
          </button>
          <button
            className={button}
            disabled={p.busy}
            onClick={() =>
              void p.run(async () => {
                const path = await save({
                  defaultPath: "deliverable-with-assets.zip",
                });
                if (path)
                  await p.call({ action: "exportDeliverable", id: d.id, path });
              })
            }
          >
            Export draft with all asset files
          </button>
          <Artifacts
            workspaceId={props.workspaceId}
            recordId={d.id}
            artifacts={d.body.artifacts}
            choices={p.choices}
            onError={props.onError}
          />
          {d.body.attachments.length > 0 && (
            <Inspect
              value={d.body.attachments}
              label="Included asset files and relative paths"
            />
          )}
        </section>
      ))}
      <section className={card}>
        <h3 className="font-semibold">Project deliverable roles</h3>
        <p className={muted}>
          A main deliverable is optional. Existing manuscript selection remains
          available in Project brief.
        </p>
        <Field label="Deliverable role">
          <select
            className={input}
            value={role}
            onChange={(e) => setRole(e.target.value)}
          >
            {["main", "grant", "seminar", "teaching", "memo", "supporting"].map(
              (r) => (
                <option key={r}>{r}</option>
              ),
            )}
          </select>
        </Field>
        <ObjectSelect
          choices={p.choices?.objects ?? []}
          value={roleObject}
          onChange={setRoleObject}
          label="Object to assign"
        />
        <button
          className={button}
          disabled={p.busy || !roleObject}
          onClick={() =>
            void p.run(async () => {
              const previous = p.records.deliverable_role?.find(
                (r) => r.title === role,
              );
              await p.call({
                action: "role",
                role,
                object: roleObject,
                supersedes: previous?.id ?? null,
                operationId: operation(),
              });
            })
          }
        >
          Save role assignment
        </button>
        {p.records.deliverable_role?.map((r) => (
          <div key={r.id}>
            <span className="text-sm">{r.title}</span>
            <Inspect value={r.body} />
          </div>
        ))}
      </section>
      <p className={muted}>
        For external editing, open Writing → Edits & acceptance: inspect an
        exact working file, open it externally, refresh the project inventory,
        and compare incoming changes before acceptance.
      </p>
    </div>
  );
}

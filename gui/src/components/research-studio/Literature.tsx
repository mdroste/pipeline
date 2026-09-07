import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  studioClient,
  type BibEntry,
  type BibliographyRecord,
  type LiteratureRecord,
  type LiteratureNote,
  type ZoteroPreview,
} from "../../lib/studioClient";
import type { ProjectRecord } from "../../lib/projectClient";
import type { SourceRecord } from "../../lib/workbenchTypes";
import { workbenchClient } from "../../lib/workbenchClient";
import {
  button,
  panel,
  Text,
  Select,
  Field,
  ErrorNotice,
  Inspect,
  useAction,
  type StudioProps,
} from "./shared";
const blank: LiteratureNote = {
  question: "",
  statement: "",
  citationKey: "",
  sourceVersionId: null,
  start: null,
  end: null,
  quote: "",
  identityChecked: false,
  support: "unsupported",
  method: "manual",
  relatedVersionIds: [],
};
export default function Literature({
  workspaceId,
  data,
  onRefresh,
  onDocument,
}: StudioProps) {
  const [bibliography, setBibliography] = useState<
    ProjectRecord<BibliographyRecord>[]
  >([]);
  const [sources, setSources] = useState<SourceRecord[]>([]);
  const [notes, setNotes] = useState<ProjectRecord<LiteratureRecord>[]>([]);
  const [revision, setRevision] = useState("");
  const [entries, setEntries] = useState<BibEntry[]>([]);
  const [selectedEntries, setSelectedEntries] = useState<string[]>([]);
  const [bibNotice, setBibNotice] = useState("");
  const [manuscript, setManuscript] = useState(
    data.settings.body.manuscriptRevisionId ?? "",
  );
  const [citations, setCitations] = useState<Awaited<
    ReturnType<typeof studioClient.citations>
  > | null>(null);
  const [noteId, setNoteId] = useState("");
  const [note, setNote] = useState(blank);
  const [filter, setFilter] = useState("");
  const [sourceText, setSourceText] = useState<{
    text: string;
    start: number;
    end: number;
    accessState: string;
  } | null>(null);
  const sourceElement = useRef<HTMLPreElement>(null);
  const [sourceTitle, setSourceTitle] = useState("");
  const [versionLabel, setVersionLabel] = useState("");
  const [access, setAccess] = useState<"full" | "partial" | "abstract">("full");
  const [sourceKey, setSourceKey] = useState("");
  const { error, busy, run, setError } = useAction();
  const [zotero, setZotero] = useState<ZoteroPreview | null>(null);
  const [collections, setCollections] = useState<ZoteroPreview | null>(null);
  const [collection, setCollection] = useState("");
  const [selectedItems, setSelectedItems] = useState<string[]>([]);
  const refresh = async () => {
    const [b, n, s] = await Promise.all([
      studioClient.records<BibliographyRecord>(workspaceId, "bibliography"),
      studioClient.records<LiteratureRecord>(workspaceId, "literature"),
      workbenchClient.listSources(workspaceId),
    ]);
    setBibliography(b);
    setNotes(n);
    setSources(s);
  };
  useEffect(() => {
    let disposed = false;
    void Promise.all([
      studioClient.records<BibliographyRecord>(workspaceId, "bibliography"),
      studioClient.records<LiteratureRecord>(workspaceId, "literature"),
      workbenchClient.listSources(workspaceId),
    ])
      .then(([b, n, s]) => {
        if (!disposed) {
          setBibliography(b);
          setNotes(n);
          setSources(s);
        }
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
    };
  }, [workspaceId, setError]);
  const toggle = (old: string[], id: string) =>
    old.includes(id) ? old.filter((i) => i !== id) : [...old, id];
  const chosen = notes.find((n) => n.id === noteId);
  const useSelection = () => {
    const selection = window.getSelection();
    if (!sourceText || !sourceElement.current || !selection?.rangeCount) return;
    const range = selection.getRangeAt(0);
    if (
      !sourceElement.current.contains(range.startContainer) ||
      !sourceElement.current.contains(range.endContainer) ||
      range.collapsed
    )
      return;
    const prefix = range.cloneRange();
    prefix.selectNodeContents(sourceElement.current);
    prefix.setEnd(range.startContainer, range.startOffset);
    const quote = range.toString();
    const start =
      sourceText.start + new TextEncoder().encode(prefix.toString()).length;
    setNote((n) => ({
      ...n,
      quote,
      start,
      end: start + new TextEncoder().encode(quote).length,
    }));
  };
  return (
    <div className="space-y-5">
      <ErrorNotice error={error} />
      <div className="grid gap-4 lg:grid-cols-2">
        <section className={panel}>
          <h2 className="font-semibold">Local bibliography</h2>
          <button
            className={button}
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const path = await open({
                  multiple: false,
                  directory: false,
                  filters: [{ name: "BibTeX", extensions: ["bib"] }],
                });
                if (typeof path !== "string") return;
                const p = await workbenchClient.importPaper({
                  workspaceId,
                  paperId: null,
                  title: path.split(/[\\/]/).pop() ?? "Bibliography",
                  role: "other",
                  path,
                  operationId: `bib-${crypto.randomUUID()}`,
                });
                setRevision(p.revision!.id);
                await onRefresh();
              })
            }
          >
            Import BibTeX file
          </button>
          <Select
            label="Bibliography revision"
            value={revision}
            onChange={(v) => {
              setRevision(v);
              setEntries([]);
            }}
            options={data.papers.flatMap((p) =>
              p.revision
                ? [{ value: p.revision.id, label: p.paper.title }]
                : [],
            )}
          />
          <button
            className={button}
            disabled={!revision || busy}
            onClick={() =>
              void run(async () => {
                const p = await studioClient.bibliography(
                  workspaceId,
                  revision,
                );
                setEntries(p.entries);
                setSelectedEntries(p.entries.map((e) => e.id));
                setBibNotice(
                  `${p.limitations}${p.existingKeys.length ? ` Existing keys: ${p.existingKeys.join(", ")}` : ""}`,
                );
              })
            }
          >
            Preview entries and duplicates
          </button>
          <p className="text-xs text-gray-500">
            {bibNotice ||
              "Citation keys, unknown fields and original file bytes remain intact. Reimport preserves project notes."}
          </p>
          <div className="max-h-64 space-y-2 overflow-auto">
            {entries.map((e) => (
              <label
                key={`${e.id}-${e.start}`}
                className="flex gap-2 rounded border p-2 text-xs"
              >
                <input
                  type="checkbox"
                  checked={selectedEntries.includes(e.id)}
                  onChange={() => setSelectedEntries((v) => toggle(v, e.id))}
                />
                <span>
                  <strong>{e.key}</strong> · {e.fields.title ?? e.entryType}
                  <br />
                  {e.warnings.join("; ")}
                </span>
              </label>
            ))}
          </div>
          {entries.length > 0 && (
            <button
              className={button}
              disabled={!selectedEntries.length || busy}
              onClick={() =>
                void run(async () => {
                  await studioClient.mutate(workspaceId, {
                    action: "importBibliography",
                    revisionId: revision,
                    selected: selectedEntries,
                  });
                  setEntries([]);
                  await refresh();
                })
              }
            >
              Import selected source records
            </button>
          )}
          <div className="max-h-52 space-y-2 overflow-auto">
            {bibliography.map((b) => (
              <div key={b.id} className="border-t py-2 text-xs">
                <button
                  className="text-left underline"
                  onClick={() => {
                    setNoteId("");
                    setNote({
                      ...blank,
                      citationKey: b.body.entry?.key ?? "",
                      sourceVersionId: b.body.source.versionId,
                    });
                    setSourceText(null);
                  }}
                >
                  {b.body.entry?.key ?? b.body.item?.key ?? "Source"} ·{" "}
                  {b.body.source.title}
                </button>
                <p>
                  {b.body.source.accessState} ·{" "}
                  {b.body.source.versionLabel ?? "Version unspecified"}
                </p>
                <Inspect value={b.body} label="Metadata and original entry" />
              </div>
            ))}
          </div>
        </section>
        <section className={panel}>
          <h2 className="font-semibold">Zotero collection preview</h2>
          <p className="text-xs text-gray-500">
            Read-only local API. Open Zotero and enable local application
            communication. Collection items enter Pipeline as metadata;
            attachments are not downloaded.
          </p>
          <button
            className={button}
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const p = await studioClient.zotero(null);
                setCollections(p);
                setZotero(null);
              })
            }
          >
            Connect locally and list collections
          </button>
          {collections && (
            <>
              <Select
                label="Zotero collection"
                value={collection}
                onChange={(v) => {
                  setCollection(v);
                  setZotero(null);
                }}
                options={collections.items.map((i) => ({
                  value: i.key,
                  label: i.data.name ?? i.key,
                }))}
              />
              <button
                className={button}
                disabled={!collection || busy}
                onClick={() =>
                  void run(async () => {
                    const p = await studioClient.zotero(
                      collection,
                      0,
                      collections.serverId,
                    );
                    setZotero(p);
                    setSelectedItems(p.items.map((i) => i.key));
                  })
                }
              >
                Preview selected collection
              </button>
              {collections.hasMore && (
                <button
                  className={button}
                  disabled={busy}
                  onClick={() =>
                    void run(async () =>
                      setCollections(
                        await studioClient.zotero(
                          null,
                          collections.start + 100,
                          collections.serverId,
                        ),
                      ),
                    )
                  }
                >
                  Next collections page
                </button>
              )}
            </>
          )}
          {zotero && (
            <>
              <p className="text-xs">
                Instance:{" "}
                {zotero.serverId ??
                  "Legacy instance identity unavailable; content hashes distinguish imports"}
              </p>
              <div className="max-h-64 overflow-auto">
                {zotero.items.map((i) => (
                  <label key={i.key} className="flex gap-2 py-1 text-xs">
                    <input
                      type="checkbox"
                      checked={selectedItems.includes(i.key)}
                      onChange={() => setSelectedItems((v) => toggle(v, i.key))}
                    />
                    {i.data.title ?? i.key} · v{i.version}
                  </label>
                ))}
              </div>
              <button
                className={button}
                disabled={!selectedItems.length || busy}
                onClick={() =>
                  void run(async () => {
                    await studioClient.importZotero(
                      workspaceId,
                      zotero,
                      selectedItems,
                    );
                    await refresh();
                  })
                }
              >
                Import selected metadata
              </button>
              {zotero.hasMore && (
                <button
                  className={button}
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      const p = await studioClient.zotero(
                        collection,
                        zotero.start + 100,
                        zotero.serverId,
                      );
                      setZotero(p);
                      setSelectedItems(p.items.map((i) => i.key));
                    })
                  }
                >
                  Preview next page
                </button>
              )}
            </>
          )}
        </section>
      </div>
      <section className={panel}>
        <h2 className="font-semibold">Citation-key navigation</h2>
        <Select
          label="Manuscript revision"
          value={manuscript}
          onChange={setManuscript}
          options={data.papers.flatMap((p) =>
            p.revision ? [{ value: p.revision.id, label: p.paper.title }] : [],
          )}
        />
        <button
          className={button}
          disabled={!manuscript || busy}
          onClick={() =>
            void run(async () =>
              setCitations(
                await studioClient.citations(workspaceId, manuscript),
              ),
            )
          }
        >
          Find literal TeX citations
        </button>
        {citations && (
          <div className="max-h-64 overflow-auto">
            {citations.citations.map((c, i) => (
              <div
                className="flex flex-wrap gap-3 border-t py-2 text-xs"
                key={`${c.key}-${i}`}
              >
                <button
                  className="underline"
                  onClick={() => onDocument(manuscript)}
                >
                  {c.key} · bytes {c.start}–{c.end}
                </button>
                <span>
                  {c.sources.length === 0
                    ? "Source identity unavailable"
                    : c.sources.length > 1
                      ? `${c.sources.length} source entries — choose a version`
                      : `${c.sources[0].body.source.accessState} access`}
                </span>
                {c.sources.map((s) => (
                  <button
                    className={button}
                    key={s.id}
                    onClick={() => {
                      setNoteId("");
                      setNote({
                        ...blank,
                        citationKey: c.key,
                        sourceVersionId: s.body.source.versionId,
                      });
                      setSourceText(null);
                    }}
                  >
                    {s.body.source.title}
                  </button>
                ))}
                <span>{c.notes.length} passage assessment(s)</span>
              </div>
            ))}
          </div>
        )}
      </section>
      <section className={panel}>
        <h2 className="font-semibold">Add a local source version</h2>
        <div className="grid gap-3 md:grid-cols-2">
          <Text
            label="Source title"
            value={sourceTitle}
            onChange={setSourceTitle}
          />
          <Text
            label="Citation key (optional)"
            value={sourceKey}
            onChange={setSourceKey}
          />
          <Text
            label="Version label (working paper, published version… )"
            value={versionLabel}
            onChange={setVersionLabel}
          />
          <Select
            label="Access supplied by this file"
            value={access}
            onChange={(s) => setAccess(s as "full" | "partial" | "abstract")}
            options={["full", "partial", "abstract"].map((v) => ({
              value: v,
              label: v,
            }))}
          />
        </div>
        <button
          className={button}
          disabled={!sourceTitle || busy}
          onClick={() =>
            void run(async () => {
              const path = await open({
                multiple: false,
                directory: false,
                filters: [
                  {
                    name: "Source document",
                    extensions: ["pdf", "txt", "md", "docx"],
                  },
                ],
              });
              if (typeof path !== "string") return;
              const imported = await workbenchClient.importSource({
                workspaceId,
                title: sourceTitle,
                citationKey: sourceKey || null,
                identifiers: {},
                versionLabel: versionLabel || null,
                path,
                locator: null,
                accessState: access,
                acquiredVia: path.endsWith(".pdf") ? "local_pdf" : "local_file",
                operationId: `source-${crypto.randomUUID()}`,
              });
              setNote((n) => ({
                ...n,
                sourceVersionId: imported.source.versionId,
                citationKey: sourceKey,
              }));
              setSourceText(null);
              await refresh();
            })
          }
        >
          Choose and retain local source
        </button>
      </section>
      <div className="grid gap-4 xl:grid-cols-2">
        <section className={panel}>
          <h2 className="font-semibold">
            Literature note and source-support checklist
          </h2>
          <button
            className={button}
            onClick={() => {
              setNoteId("");
              setNote(blank);
              setSourceText(null);
            }}
          >
            New literature note
          </button>
          <Text
            label="Research question"
            value={note.question}
            onChange={(question) => setNote((n) => ({ ...n, question }))}
          />
          <Text
            label="Substantive comparison or claim"
            multiline
            value={note.statement}
            onChange={(statement) => setNote((n) => ({ ...n, statement }))}
          />
          <Text
            label="Citation key"
            value={note.citationKey}
            onChange={(citationKey) => setNote((n) => ({ ...n, citationKey }))}
          />
          <Select
            label="Exact source version"
            value={note.sourceVersionId ?? ""}
            onChange={(sourceVersionId) => {
              setNote((n) => ({
                ...n,
                sourceVersionId: sourceVersionId || null,
                start: null,
                end: null,
                quote: "",
                support: "unsupported",
              }));
              setSourceText(null);
            }}
            options={sources.map((s) => ({
              value: s.versionId,
              label: `${s.title} · ${s.versionLabel ?? s.versionId.slice(-8)} · ${s.accessState}`,
            }))}
          />
          <Select
            label="Passage assessment"
            value={note.support}
            onChange={(support) => setNote((n) => ({ ...n, support }))}
            options={["unsupported", "relevant", "supports", "contradicts"].map(
              (v) => ({ value: v, label: v }),
            )}
          />
          <Select
            label="Assessment method"
            value={note.method}
            onChange={(method) => setNote((n) => ({ ...n, method }))}
            options={[
              "manual",
              "model_assessment",
              "deterministic_identity",
            ].map((v) => ({ value: v, label: v.replaceAll("_", " ") }))}
          />
          <label className="flex gap-2 text-xs">
            <input
              type="checkbox"
              checked={note.identityChecked}
              onChange={(e) =>
                setNote((n) => ({ ...n, identityChecked: e.target.checked }))
              }
            />
            I checked the source identity
          </label>
          <Field label="Related working-paper or published versions">
            <div className="max-h-28 overflow-auto">
              {sources
                .filter((s) => s.versionId !== note.sourceVersionId)
                .map((s) => (
                  <label className="flex gap-2" key={s.versionId}>
                    <input
                      type="checkbox"
                      checked={note.relatedVersionIds.includes(s.versionId)}
                      onChange={() =>
                        setNote((n) => ({
                          ...n,
                          relatedVersionIds: toggle(
                            n.relatedVersionIds,
                            s.versionId,
                          ),
                        }))
                      }
                    />
                    {s.title} · {s.versionLabel ?? "Unspecified version"}
                  </label>
                ))}
            </div>
          </Field>
          <p className="whitespace-pre-wrap text-xs">
            Exact passage:{" "}
            {note.quote || "None — substantive support remains unverified"}
          </p>
          <button
            className={button}
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const saved = await studioClient.mutate<
                  ProjectRecord<LiteratureRecord>
                >(workspaceId, {
                  action: "saveLiterature",
                  id: noteId || null,
                  expectedRevision: chosen?.revision ?? 0,
                  note,
                });
                setNoteId(saved.id);
                await refresh();
              })
            }
          >
            Save passage assessment
          </button>
        </section>
        <section className={panel}>
          <h2 className="font-semibold">Source passage</h2>
          <button
            className={button}
            disabled={!note.sourceVersionId || busy}
            onClick={() =>
              void run(async () =>
                setSourceText(
                  await studioClient.source(workspaceId, note.sourceVersionId!),
                ),
              )
            }
          >
            Read retained source text
          </button>
          {sourceText && (
            <>
              <p className="text-xs">
                Access: {sourceText.accessState}. Bytes {sourceText.start}–
                {sourceText.end}. Select supporting or contradicting text below.
              </p>
              <pre
                ref={sourceElement}
                onMouseUp={useSelection}
                onKeyUp={useSelection}
                tabIndex={0}
                className="h-80 overflow-auto whitespace-pre-wrap rounded border p-3 font-sans text-sm leading-6"
              >
                {sourceText.text}
              </pre>
              <button
                className={button}
                disabled={busy}
                onClick={() =>
                  void run(async () =>
                    setSourceText(
                      await studioClient.source(
                        workspaceId,
                        note.sourceVersionId!,
                        sourceText.end,
                      ),
                    ),
                  )
                }
              >
                Next source segment
              </button>
            </>
          )}
          {chosen && (
            <Inspect
              label="Saved source-support checklist"
              value={chosen.body.checklist}
            />
          )}
        </section>
      </div>
      <section className={panel}>
        <h2 className="font-semibold">Literature comparison table</h2>
        <Text
          label="Filter by research question"
          value={filter}
          onChange={setFilter}
        />
        <div className="overflow-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr>
                <th>Question</th>
                <th>Substantive entry</th>
                <th>Source access</th>
                <th>Passage / method</th>
              </tr>
            </thead>
            <tbody>
              {notes
                .filter((n) =>
                  n.body.note.question
                    .toLowerCase()
                    .includes(filter.toLowerCase()),
                )
                .map((n) => (
                  <tr key={n.id} className="border-t">
                    <td className="p-2">
                      <button
                        className="underline"
                        onClick={() => {
                          setNoteId(n.id);
                          setNote(n.body.note);
                          setSourceText(null);
                        }}
                      >
                        {n.body.note.question}
                      </button>
                    </td>
                    <td className="max-w-lg p-2">{n.body.note.statement}</td>
                    <td>{n.body.checklist.access}</td>
                    <td>
                      {n.body.checklist.passageRelevance} ·{" "}
                      {n.body.checklist.assessmentMethod}
                      <br />
                      {n.body.note.quote.slice(0, 150) ||
                        "Unsupported: no linked passage"}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

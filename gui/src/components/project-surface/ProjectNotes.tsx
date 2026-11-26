import { useProjectEnvironment } from "../../lib/projectEnvironment";
import { useEffect, useRef, useState } from "react";
import type { ResearchNote } from "../../lib/workbench/researchTypes";
import {
  NOTE_KINDS,
  button,
  input,
  muted,
  formatDate,
  noteKindLabel,
  op,
  type SurfaceApi,
} from "./shared";
export function NoteCard({
  note,
  api,
}: {
  note: ResearchNote;
  api: SurfaceApi;
}) {
  const { researchClient, projectClient } = useProjectEnvironment();
  const { data, run, saveSettings } = api;
  const brief = data.settings.body.briefNoteIds;
  const excluded = data.settings.body.excludedNoteIds;
  const [editing, setEditing] = useState(false);
  const [body, setBody] = useState(note.body);
  const [saving, setSaving] = useState(false);
  const saveBody = async () => {
    if (saving || !body.trim()) return;
    setSaving(true);
    await run(async () => {
      await researchClient.updateNote({
        noteId: note.id,
        expectedRevision: note.revision,
        body: body.trim(),
        operationId: op(),
      });
      setEditing(false);
    });
    setSaving(false);
  };
  const update = (patch: { state?: "accepted" | "rejected" }) =>
    run(() =>
      researchClient.updateNote({
        noteId: note.id,
        expectedRevision: note.revision,
        operationId: op(),
        ...patch,
      }),
    );
  return (
    <div
      data-note-id={note.id}
      className="rounded-lg border border-gray-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-950"
    >
      <div className={`flex flex-wrap items-center gap-x-2 ${muted}`}>
        <span className="font-medium">{noteKindLabel(note.kind)}</span>
        {note.state === "proposed" && (
          <span>
            {note.importedStatus
              ? "· Imported: the sender accepted this, you haven't yet"
              : `· Suggested by ${note.origin === "user" ? "you" : "the assistant"}, not yet accepted`}
          </span>
        )}
        {note.state === "rejected" && <span>· Rejected</span>}
        {note.state === "retired" && <span>· No longer current</span>}
        <span className="ml-auto">{formatDate(note.updatedAt)}</span>
      </div>
      {editing ? (
        <form
          className="my-3 space-y-3"
          onSubmit={(e) => {
            e.preventDefault();
            void saveBody();
          }}
        >
          <label className="block text-xs">
            Edit note
            <textarea
              className={`${input} mt-2`}
              rows={4}
              value={body}
              onChange={(e) => setBody(e.target.value)}
              autoFocus
            />
          </label>
          <div className="flex gap-2">
            <button className={button} disabled={saving || !body.trim()}>
              {saving ? "Saving…" : "Save changes"}
            </button>
            <button
              type="button"
              className={button}
              disabled={saving}
              onClick={() => setEditing(false)}
            >
              Cancel
            </button>
          </div>
        </form>
      ) : (
        <p className="my-2 whitespace-pre-wrap text-sm">{note.body}</p>
      )}
      {note.state === "proposed" && (
        <div className="flex gap-2">
          <button
            className={button}
            onClick={() => void update({ state: "accepted" })}
          >
            Accept
          </button>
          <button
            className={button}
            onClick={() => void update({ state: "rejected" })}
          >
            Reject
          </button>
        </div>
      )}
      {note.state === "accepted" && (
        <div className={`flex flex-wrap gap-4 ${muted}`}>
          <button
            type="button"
            className="underline underline-offset-2"
            onClick={() => {
              setBody(note.body);
              setEditing(true);
            }}
          >
            Edit note
          </button>
          <label className="flex items-center gap-1.5">
            <input
              type="checkbox"
              checked={brief.includes(note.id) || note.pinned}
              onChange={(e) => {
                const checked = e.target.checked;
                void run(async () => {
                  if (note.pinned && !checked)
                    await researchClient.updateNote({
                      noteId: note.id,
                      expectedRevision: note.revision,
                      pinned: false,
                      operationId: op(),
                    });
                  await projectClient.mutate(api.workspaceId, {
                    action: "saveHome",
                    expectedRevision: data.settings.revision,
                    settings: {
                      ...data.settings.body,
                      briefNoteIds: checked
                        ? [...new Set([...brief, note.id])]
                        : brief.filter((id) => id !== note.id),
                    },
                  });
                });
              }}
            />
            Pin in project brief
          </label>
          <label className="flex items-center gap-1.5">
            <input
              type="checkbox"
              checked={excluded.includes(note.id)}
              onChange={(e) =>
                void saveSettings({
                  excludedNoteIds: e.target.checked
                    ? [...excluded, note.id]
                    : excluded.filter((id) => id !== note.id),
                })
              }
            />
            Hide from the assistant
          </label>
        </div>
      )}
    </div>
  );
}

function RetainedNote({
  note,
  api,
  visible,
}: {
  note: ResearchNote;
  api: SurfaceApi;
  visible: boolean;
}) {
  const visited = useRef(false);
  if (visible) visited.current = true;
  return visited.current ? (
    <div hidden={!visible}>
      <NoteCard note={note} api={api} />
    </div>
  ) : null;
}

export default function ProjectNotes(
  props: SurfaceApi & {
    compact?: boolean;
    context?: boolean;
    selectedNoteId?: string;
  },
) {
  const { researchClient } = useProjectEnvironment();
  const { data, workspaceId, run } = props;
  const [noteBody, setNoteBody] = useState("");
  const [noteKind, setNoteKind] = useState<ResearchNote["kind"]>("question");
  const [selected, setSelected] = useState("");
  const [composing, setComposing] = useState(false);
  const [filter, setFilter] = useState("brief");
  const notesRoot = useRef<HTMLElement>(null);
  useEffect(() => {
    if (props.selectedNoteId) {
      setSelected(props.selectedNoteId);
      setFilter("all");
    }
  }, [props.selectedNoteId]);
  const visibleNotes = props.context
    ? data.notes.filter(
        (item) =>
          filter === "all" ||
          (filter === "suggested"
            ? item.state === "proposed"
            : item.state === "accepted" &&
              (data.settings.body.briefNoteIds.includes(item.id) ||
                item.pinned)),
      )
    : data.notes;
  const note = data.notes.find((item) => item.id === selected) ?? data.notes[0];
  useEffect(() => {
    if (!props.selectedNoteId || filter !== "all") return;
    const target = Array.from(
      notesRoot.current?.querySelectorAll<HTMLElement>("[data-note-id]") ?? [],
    ).find((element) => element.dataset.noteId === props.selectedNoteId);
    target?.scrollIntoView?.({ block: "nearest" });
  }, [props.selectedNoteId, filter]);
  return (
    <section
      ref={notesRoot}
      className={`space-y-3 ${props.compact ? "overview-notes" : ""}`}
    >
      {!props.compact && <h2 className="font-semibold">Notes</h2>}
      {!props.context && (
        <p className={muted}>
          Save decisions, assumptions, questions, and next steps to use in
          future conversations.
        </p>
      )}
      {props.context && (
        <div className="overview-sections" aria-label="Project context views">
          {[
            ["brief", "Brief"],
            ["all", "All notes"],
            ["suggested", "Suggested"],
          ].map(([id, label]) => (
            <button
              key={id}
              type="button"
              aria-pressed={filter === id}
              onClick={() => setFilter(id)}
            >
              {label}
            </button>
          ))}
          <button type="button" onClick={() => setComposing(!composing)}>
            {composing ? "Close new note" : "Add a note"}
          </button>
        </div>
      )}
      {props.compact && !props.context && (
        <div className="overview-actions">
          {!!data.notes.length && (
            <label className="overview-note-picker">
              Saved note
              <select
                className="ui-input"
                aria-label="Saved note"
                value={note?.id ?? ""}
                onChange={(event) => setSelected(event.target.value)}
              >
                {data.notes.map((item) => (
                  <option key={item.id} value={item.id}>
                    {noteKindLabel(item.kind)} · {item.state} ·{" "}
                    {item.body.slice(0, 80)}
                  </option>
                ))}
              </select>
            </label>
          )}
          <button
            type="button"
            className="ui-button"
            aria-expanded={composing}
            onClick={() => setComposing(!composing)}
          >
            {composing ? "Close new note" : "Add a note"}
          </button>
        </div>
      )}
      <div
        hidden={props.compact && !composing}
        className="flex flex-wrap gap-2 sm:flex-nowrap"
      >
        <select
          aria-label="Note type"
          className="rounded-md border border-gray-300 bg-transparent px-2 text-xs dark:border-neutral-700"
          value={noteKind}
          onChange={(e) => setNoteKind(e.target.value as ResearchNote["kind"])}
        >
          {NOTE_KINDS.map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
        <textarea
          aria-label="New note"
          value={noteBody}
          onChange={(e) => setNoteBody(e.target.value)}
          rows={2}
          className={input}
          placeholder="Write a note…"
        />
        <button
          className={button}
          disabled={!noteBody.trim()}
          onClick={() =>
            void run(async () => {
              await researchClient.createNote({
                workspaceId,
                paperId: null,
                kind: noteKind,
                body: noteBody,
                state: "accepted",
                origin: "user",
                pinned: Boolean(props.context && filter === "brief"),
                operationId: op(),
              });
              setNoteBody("");
              setComposing(false);
            })
          }
        >
          Save note
        </button>
      </div>
      {!data.notes.length && <p className={muted}>No notes yet.</p>}
      {props.context && data.notes.length > 0 && !visibleNotes.length && (
        <p className={muted}>
          {filter === "brief"
            ? "Pin accepted notes to compose your brief."
            : "No suggested notes."}
        </p>
      )}
      <div
        className={
          props.compact ? "overview-note-detail" : "grid gap-3 lg:grid-cols-2"
        }
      >
        {data.notes.map((n) =>
          props.compact ? (
            <RetainedNote
              key={n.id}
              note={n}
              api={props}
              visible={
                props.context ? visibleNotes.includes(n) : n.id === note?.id
              }
            />
          ) : (
            <NoteCard key={n.id} note={n} api={props} />
          ),
        )}
      </div>
      {data.noteHistory?.length ? (
        <details
          className={`rounded-md border border-gray-200 p-3 dark:border-neutral-800 ${muted}`}
        >
          <summary className="cursor-pointer">
            Earlier wording of edited notes
          </summary>
          {data.noteHistory.map((item, index) => (
            <div key={`${item.noteId}-${index}`} className="mt-3">
              <p>{formatDate(item.createdAt)}</p>
              {item.details.previous ? (
                <p className="whitespace-pre-wrap text-gray-700 dark:text-neutral-300">
                  {item.details.previous.body}
                </p>
              ) : (
                <p>
                  Only the note’s settings changed; the earlier text was not
                  kept.
                </p>
              )}
            </div>
          ))}
        </details>
      ) : null}
    </section>
  );
}

import { useState } from "react";
import { workbenchClient } from "../../lib/workbenchClient";
import { projectClient } from "../../lib/projectClient";
import type { ResearchNote } from "../../lib/workbenchTypes";
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
function NoteCard({ note, api }: { note: ResearchNote; api: SurfaceApi }) {
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
      await workbenchClient.updateNote({
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
      workbenchClient.updateNote({
        noteId: note.id,
        expectedRevision: note.revision,
        operationId: op(),
        ...patch,
      }),
    );
  return (
    <div className="rounded-lg border border-gray-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-950">
      <div className={`flex flex-wrap items-center gap-x-2 ${muted}`}>
        <span className="font-medium">{noteKindLabel(note.kind)}</span>
        {note.state === "proposed" && (
          <span>
            · Suggested by {note.origin === "user" ? "you" : "the assistant"},
            not yet accepted
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
                    await workbenchClient.updateNote({
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

export default function ProjectNotes(props: SurfaceApi) {
  const { data, workspaceId, run } = props;
  const [noteBody, setNoteBody] = useState("");
  const [noteKind, setNoteKind] = useState<ResearchNote["kind"]>("question");
  return (
    <section className="space-y-3">
      <h2 className="font-semibold">Notes</h2>
      <p className={muted}>
        Record decisions, assumptions, questions, and next steps so they survive
        across conversations.
      </p>
      <div className="flex flex-wrap gap-2 sm:flex-nowrap">
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
              await workbenchClient.createNote({
                workspaceId,
                paperId: null,
                kind: noteKind,
                body: noteBody,
                state: "accepted",
                origin: "user",
                pinned: false,
                operationId: op(),
              });
              setNoteBody("");
            })
          }
        >
          Save note
        </button>
      </div>
      {!data.notes.length && <p className={muted}>No notes yet.</p>}
      <div className="grid gap-3 lg:grid-cols-2">
        {data.notes.map((n) => (
          <NoteCard key={n.id} note={n} api={props} />
        ))}
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

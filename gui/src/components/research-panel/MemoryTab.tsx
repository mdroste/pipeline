import { primaryButton } from "../../ui/classes";
import { researchClient } from "../../lib/workbench/researchClient";
import { NOTE_KINDS, noteKindLabel } from "../project-surface/shared";
import { importedLabel, plainLabel } from "./format";
import { operation } from "./useResearchPanelController";
import type { useResearchPanelController } from "./useResearchPanelController";
import type {
  ConversationSnapshot,
  ResearchNote,
} from "../../lib/workbenchTypes";

type Panel = ReturnType<typeof useResearchPanelController>;
type Props = Pick<
  Panel,
  | "busy"
  | "act"
  | "noteKind"
  | "setNoteKind"
  | "noteBody"
  | "setNoteBody"
  | "notes"
> & {
  workspaceId: string;
  snapshot: ConversationSnapshot | null;
};

export default function MemoryTab({
  busy,
  act,
  noteKind,
  setNoteKind,
  noteBody,
  setNoteBody,
  notes,
  workspaceId,
  snapshot,
}: Props) {
  return (
    <div className="space-y-4">
      <div>
        <div className="flex gap-2">
          <select
            aria-label="Note type"
            value={noteKind}
            onChange={(event) =>
              setNoteKind(event.target.value as ResearchNote["kind"])
            }
            className="rounded border bg-white p-2 text-xs dark:bg-neutral-950"
          >
            {NOTE_KINDS.map(([kind, label]) => (
              <option key={kind} value={kind}>
                {label}
              </option>
            ))}
          </select>
          <button
            type="button"
            disabled={!noteBody.trim() || busy}
            onClick={() =>
              void act(async () => {
                await researchClient.createNote({
                  workspaceId,
                  paperId: snapshot?.session.paperId ?? null,
                  kind: noteKind,
                  body: noteBody,
                  state: "accepted",
                  origin: "user",
                  pinned: noteKind === "handoff",
                  operationId: operation("note"),
                });
                setNoteBody("");
              })
            }
            className={primaryButton}
          >
            Add reviewed note
          </button>
        </div>
        <textarea
          aria-label="Note text"
          value={noteBody}
          onChange={(event) => setNoteBody(event.target.value)}
          rows={3}
          placeholder="Question, assumption, decision, next step, notation, or handoff…"
          className="mt-2 w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
        />
      </div>
      <div className="space-y-2">
        {notes.map((note) => (
          <div
            key={note.id}
            className="rounded border bg-white p-3 dark:bg-neutral-950"
          >
            <div className="flex items-center gap-2 text-[11px] text-gray-500">
              <span>{noteKindLabel(note.kind)}</span>
              <span>· {plainLabel(note.state)}</span>
              <span>
                · {note.importedStatus ? "Imported" : plainLabel(note.origin)}
              </span>
              {note.pinned && <span>· pinned</span>}
            </div>
            {note.importedStatus && (
              <p className="text-[11px] text-amber-700 dark:text-amber-300">
                {importedLabel(note.importedStatus)}
              </p>
            )}
            <p className="mt-1 whitespace-pre-wrap">{note.body}</p>
            <div className="mt-2 flex gap-1">
              {note.state === "proposed" && (
                <>
                  <button
                    type="button"
                    onClick={() =>
                      void act(async () => {
                        await researchClient.updateNote({
                          noteId: note.id,
                          expectedRevision: note.revision,
                          state: "accepted",
                          operationId: operation("accept-note"),
                        });
                      })
                    }
                    className="rounded border px-2 py-1 text-[11px]"
                  >
                    Accept
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      void act(async () => {
                        await researchClient.updateNote({
                          noteId: note.id,
                          expectedRevision: note.revision,
                          state: "rejected",
                          operationId: operation("reject-note"),
                        });
                      })
                    }
                    className="rounded border px-2 py-1 text-[11px]"
                  >
                    Reject
                  </button>
                </>
              )}
              <button
                type="button"
                onClick={() =>
                  void act(async () => {
                    await researchClient.updateNote({
                      noteId: note.id,
                      expectedRevision: note.revision,
                      pinned: !note.pinned,
                      operationId: operation("pin-note"),
                    });
                  })
                }
                className="rounded border px-2 py-1 text-[11px]"
              >
                {note.pinned ? "Unpin" : "Pin"}
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

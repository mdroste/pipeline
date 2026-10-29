import { researchClient } from "../../lib/workbench/researchClient";
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
            value={noteKind}
            onChange={(event) =>
              setNoteKind(event.target.value as ResearchNote["kind"])
            }
            className="rounded border bg-white p-2 text-xs dark:bg-neutral-950"
          >
            {[
              "question",
              "assumption",
              "decision",
              "next_step",
              "notation",
              "handoff",
            ].map((kind) => (
              <option key={kind}>{kind}</option>
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
            className="rounded bg-gray-900 px-3 text-xs text-white dark:bg-neutral-100 dark:text-neutral-900"
          >
            Add reviewed note
          </button>
        </div>
        <textarea
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
              <span>{note.kind}</span>
              <span>· {note.state}</span>
              <span>· {note.origin}</span>
              {note.pinned && <span>· pinned</span>}
            </div>
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

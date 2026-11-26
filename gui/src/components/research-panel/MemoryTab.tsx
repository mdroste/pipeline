import { primaryButton } from "../../ui/classes";
import { researchClient } from "../../lib/workbench/researchClient";
import { NOTE_KINDS, noteKindLabel } from "../project-surface/shared";
import { importedLabel, plainLabel } from "./format";
import { operation } from "./useResearchPanelController";
import type { useResearchPanelController } from "./useResearchPanelController";
import type { ConversationSnapshot } from "../../lib/workbench/sessionTypes";
import type { ResearchNote } from "../../lib/workbench/researchTypes";
import { useEffect, useRef } from "react";
import useRouteSnapshot from "../../hooks/useRouteSnapshot";

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
  const { route, revision } = useRouteSnapshot();
  const target =
    route.page === "workspace" &&
    route.projectId === workspaceId &&
    route.destination === "memory"
      ? route.noteId
      : undefined;
  const root = useRef<HTMLDivElement>(null);
  const followed = useRef(-1);
  useEffect(() => {
    if (!target || followed.current === revision) return;
    const element = Array.from(
      root.current?.querySelectorAll<HTMLElement>("[data-note-id]") ?? [],
    ).find((item) => item.dataset.noteId === target);
    if (element) {
      followed.current = revision;
      element.scrollIntoView?.({ block: "center" });
      element.focus({ preventScroll: true });
    }
  }, [target, notes, revision]);
  return (
    <div ref={root} className="space-y-4">
      {target && !notes.some((note) => note.id === target) && (
        <p role="status" className="ui-notice ui-notice--warning">
          The linked note is not in the available notes. It may still be loading
          or may have been removed.
        </p>
      )}
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
            data-note-id={note.id}
            tabIndex={-1}
            aria-current={note.id === target ? "location" : undefined}
            className={`rounded-ui-md border border-line bg-surface p-3 ${note.id === target ? "ui-search-target" : ""}`}
          >
            <div className="flex items-center gap-2 text-[0.6875rem] text-gray-500">
              <span>{noteKindLabel(note.kind)}</span>
              <span>· {plainLabel(note.state)}</span>
              <span>
                · {note.importedStatus ? "Imported" : plainLabel(note.origin)}
              </span>
              {note.pinned && <span>· pinned</span>}
            </div>
            {note.importedStatus && (
              <p className="text-[0.6875rem] text-warning">
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
                    className="rounded border px-2 py-1 text-[0.6875rem]"
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
                    className="rounded border px-2 py-1 text-[0.6875rem]"
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
                className="rounded border px-2 py-1 text-[0.6875rem]"
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

import type { ProjectHome } from "../projectClient";
import type { ResearchNote } from "../workbench/researchTypes";
import type {
  TranscriptItem,
  WorkbenchSession,
} from "../workbench/sessionTypes";
import { clip } from "./presentation";

// ---- Where you left off ---------------------------------------------------

export interface LeftOff {
  session: WorkbenchSession;
  /** Text typed but never sent in that conversation. */
  draft: string | null;
  /** The last thing the researcher asked there, when its transcript is loaded. */
  lastAsk: string | null;
  runs: { total: number; failed: number };
  nextSteps: ResearchNote[];
}
export function projectSessions(
  sessions: WorkbenchSession[],
  workspaceId: string,
) {
  return sessions
    .filter(
      (session) => session.workspaceId === workspaceId && !session.archivedAt,
    )
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
}
export function leftOff(
  data: ProjectHome,
  sessions: WorkbenchSession[],
  workspaceId: string,
  transcript: { sessionId: string; items: TranscriptItem[] } | null,
  textOf: (item: TranscriptItem) => string,
): LeftOff | null {
  const session = projectSessions(sessions, workspaceId)[0];
  if (!session) return null;
  const runs = data.executions.filter((run) => run.sessionId === session.id);
  const asked =
    transcript?.sessionId === session.id
      ? [...transcript.items]
          .reverse()
          .find(
            (item) =>
              item.itemKind.toLowerCase().includes("user") && textOf(item),
          )
      : undefined;
  return {
    session,
    draft: session.draft.trim() ? clip(session.draft, 220) : null,
    lastAsk: asked ? clip(textOf(asked), 220) : null,
    runs: {
      total: runs.length,
      failed: runs.filter((run) =>
        ["failed", "timed_out", "interrupted"].includes(run.outcome),
      ).length,
    },
    nextSteps: data.notes
      .filter(
        (note) =>
          note.state === "accepted" &&
          (note.kind === "next_step" || note.kind === "handoff"),
      )
      .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
      .slice(0, 2),
  };
}

/** Curated brief notes, or pinned notes and questions when none were curated. */
export function briefNotes(data: ProjectHome): ResearchNote[] {
  const accepted = data.notes.filter((note) => note.state === "accepted");
  const curated = accepted.filter((note) =>
    data.settings.body.briefNoteIds.includes(note.id),
  );
  if (curated.length) return curated;
  return accepted
    .filter((note) => note.pinned || note.kind === "question")
    .sort(
      (a, b) =>
        Number(b.kind === "question") - Number(a.kind === "question") ||
        b.updatedAt.localeCompare(a.updatedAt),
    )
    .slice(0, 3);
}

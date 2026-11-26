import type { OverviewBlock, OverviewRow } from "./types";
import type { ProjectHome } from "../projectClient";
import { clip } from "./presentation";

/** Publication stage is metadata; real blockers and decisions determine priority. */
export function attentionRows(
  data: ProjectHome,
  blocks: OverviewBlock[],
): OverviewRow[] {
  const rows = blocks
    .flatMap((item) => item.rows)
    .filter(
      (row) =>
        row.lane === "needs" ||
        (row.lane === "next" &&
          !["progress", "uncommitted", "remote"].includes(row.id)) ||
        ["superseded", "working-copy"].includes(row.id),
    )
    .map((row): OverviewRow => ({
      ...row,
      category: ["recovery", "conflicts", "build"].includes(row.id)
        ? "blocker"
        : "suggestion",
    }));
  for (const edit of data.changes.filter(
    (item) => item.body.state === "review",
  ))
    rows.push({
      id: `edit-${edit.id}`,
      version: String(edit.revision),
      text: clip(edit.body.instruction, 140) || "Proposed edit",
      detail: "Edit ready for your decision",
      category: "decision",
      action: {
        kind: "tab",
        label: "Review edit",
        tab: "edits",
        targetId: edit.id,
      },
    });
  for (const note of data.notes.filter((item) => item.state === "proposed"))
    rows.push({
      id: `note-${note.id}`,
      version: String(note.revision),
      text: clip(note.body, 140),
      detail: "Suggested project note",
      category: "decision",
      action: { kind: "notes", label: "Review note", noteId: note.id },
    });
  for (const task of data.tasks.filter(
    (item) => !["completed", "rejected", "deferred"].includes(item.body.status),
  ))
    rows.push({
      id: `task-${task.id}`,
      version: String(task.revision),
      text: task.body.objective,
      detail:
        task.body.status === "investigating" ? "In progress" : "Action item",
      category: "task",
      action: {
        kind: "tab",
        label: "Open task",
        tab: "tasks",
        targetId: task.id,
      },
    });
  const rank = { blocker: 0, decision: 1, task: 2, suggestion: 3 };
  for (const check of data.checks ?? []) {
    if (check.body.outcome === "passed") continue;
    rows.push({
      id: check.id,
      version: check.body.signature,
      text: "Citation check needs attention",
      detail:
        check.body.details.error ??
        `Missing keys: ${(check.body.details.missing ?? []).join(", ") || "none"}. ${(check.body.details.warnings ?? []).join(" ")}`,
      category: "suggestion",
      action: {
        kind: "file",
        label: "Open checked source",
        location: { path: check.body.sourcePath },
      },
    });
  }
  return [...new Map(rows.map((row) => [row.id, row])).values()].sort(
    (a, b) =>
      rank[a.category ?? "suggestion"] - rank[b.category ?? "suggestion"],
  );
}

export const NEXT_LIMIT = 3;
/** Compatibility adapter for callers which inspect the remaining blocks. */
export function arrangeOverview(
  data: ProjectHome,
  blocks: OverviewBlock[],
  _stage?: string | null,
) {
  const rows = attentionRows(data, blocks);
  const moved = new Set(rows.map((row) => row.id));
  return {
    next: null,
    needs: rows.length
      ? { id: "needs" as const, title: "Attention", rows }
      : null,
    blocks: blocks
      .map((item) => ({
        ...item,
        rows: item.rows.filter((row) => !moved.has(row.id)),
      }))
      .filter((item) => item.rows.length),
    moreTasks: 0,
  };
}

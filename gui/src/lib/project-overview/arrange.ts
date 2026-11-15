import type { OverviewBlock, OverviewRow } from "./types";
import type { ProjectHome } from "../projectClient";
import { block, clip, plural } from "./presentation";

/**
 * Which outstanding rows lead the overview at each stage, most pressing
 * first. A project with no stage uses a general order.
 */
const NEXT_BY_STAGE: Record<string, string[]> = {
  idea: [],
  drafting: ["build", "review", "findings"],
  ready: ["review", "findings", "build", "uncommitted"],
  under_review: ["uncommitted", "remote"],
  revising: [
    "progress",
    "undrafted",
    "waiting",
    "flagged",
    "findings",
    "review",
  ],
  accepted: ["uncommitted", "remote", "build"],
};
const NEXT_DEFAULT = [
  "build",
  "progress",
  "undrafted",
  "findings",
  "review",
  "uncommitted",
];
export const NEXT_LIMIT = 3;

export interface ArrangedOverview {
  /** At most three things to do now, chosen by the paper's stage. */
  next: OverviewBlock | null;
  /** Everything waiting on a decision from the researcher. */
  needs: OverviewBlock | null;
  /** The remaining blocks, without the rows that moved up. */
  blocks: OverviewBlock[];
  /** Open action items beyond the ones listed under needs. */
  moreTasks: number;
}

/**
 * Splits the derived blocks into what to do next, what waits on the
 * researcher, and the rest, so no row appears twice.
 */
export function arrangeOverview(
  data: ProjectHome,
  blocks: OverviewBlock[],
  stage: string | null | undefined,
): ArrangedOverview {
  const rows = blocks.flatMap((item) => item.rows);
  const order = (stage && NEXT_BY_STAGE[stage]) || NEXT_DEFAULT;
  const next = order
    .map((id) => rows.find((row) => row.id === id && row.lane === "next"))
    .filter((row): row is OverviewRow => Boolean(row))
    .slice(0, NEXT_LIMIT);

  const needs = rows.filter((row) => row.lane === "needs");
  const pending = data.changes.filter((item) => item.body.state === "review");
  if (pending.length)
    needs.push({
      id: "edits",
      text: `${plural(pending.length, "proposed edit")} ${pending.length === 1 ? "awaits" : "await"} your decision`,
      detail: clip(pending[0].body.instruction, 110) || undefined,
      attention: true,
      action: { kind: "tab", label: "Review edits", tab: "edits" },
    });
  const proposed = data.notes.filter((note) => note.state === "proposed");
  if (proposed.length)
    needs.push({
      id: "notes",
      text: `${plural(proposed.length, "suggested note")} ${proposed.length === 1 ? "awaits" : "await"} your decision`,
      attention: true,
      action: { kind: "notes", label: "Review suggested notes" },
    });
  const tasks = data.tasks
    .filter((task) => !["completed", "rejected"].includes(task.body.status))
    .sort(
      (a, b) =>
        Number(a.body.status === "deferred") -
          Number(b.body.status === "deferred") ||
        b.updatedAt.localeCompare(a.updatedAt),
    );
  for (const task of tasks.slice(0, NEXT_LIMIT))
    needs.push({
      id: `task-${task.id}`,
      text: task.body.objective,
      detail: `Action item · ${
        task.body.status === "deferred"
          ? "Later"
          : task.body.status === "investigating"
            ? "In progress"
            : "Open"
      }`,
      action: { kind: "tab", label: "Open", tab: "tasks" },
    });

  const moved = new Set([...next, ...needs]);
  return {
    next: block("next", "Next", next),
    needs: block("needs", "Needs you", needs),
    blocks: blocks
      .map((item) => ({
        ...item,
        rows: item.rows.filter((row) => !moved.has(row)),
      }))
      .filter((item) => item.rows.length),
    moreTasks: Math.max(0, tasks.length - NEXT_LIMIT),
  };
}

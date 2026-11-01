import type { ConversationTurn } from "./workbenchTypes";

/** Stored turn states are provider words; these are what a researcher reads. */
const TURN_STATE_LABELS: Record<string, string> = {
  inProgress: "Working",
  completed: "Completed",
  interrupted: "Stopped",
  failed: "Failed",
};

export function turnStateLabel(
  turn: Pick<ConversationTurn, "state" | "terminalAt">,
): string {
  return (
    TURN_STATE_LABELS[turn.state] ?? (turn.terminalAt ? "Finished" : "Working")
  );
}

const turnTimeFormat = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

/** Unparseable values are shown as stored. */
export function turnTimeLabel(iso: string): string {
  const parsed = new Date(iso);
  return Number.isNaN(parsed.getTime()) ? iso : turnTimeFormat.format(parsed);
}

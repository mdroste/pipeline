import { payloadText } from "./workspaceTranscript";
import type { ConversationTurn, TranscriptItem } from "./workbenchTypes";

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

/** Providers can send pages of text; one failure line stays readable. */
const MAX_FAILURE_CHARACTERS = 600;

/**
 * Why a turn ended early. ChatGPT sends an object with a message; API and Claude
 * turns save the message itself.
 */
export function turnFailureReason(error: unknown): string | null {
  const message =
    error && typeof error === "object" && "message" in error
      ? error.message
      : error;
  if (typeof message !== "string") return null;
  const reason = message.trim();
  if (!reason) return null;
  return reason.length > MAX_FAILURE_CHARACTERS
    ? `${reason.slice(0, MAX_FAILURE_CHARACTERS)}…`
    : reason;
}

/** One sentence that names the reason, ready for another to follow it. */
export function withReason(opening: string, reason?: string | null) {
  if (!reason) return `${opening}.`;
  return `${opening}: ${reason}${/[.!?…]$/.test(reason) ? "" : "."}`;
}

export interface TurnFailures {
  /** Failure lines to show after these transcript items, by item id. */
  afterItem: Map<string, string>;
  /** The newest failed turn with no saved message of its own, if any. */
  atEnd: string | null;
}

/**
 * Failed turns to show in the transcript. API and Claude turns already saved
 * their failure as a message, so a reason a message repeats is left out.
 */
export function turnFailures(
  turns: ConversationTurn[],
  items: TranscriptItem[],
): TurnFailures {
  const afterItem = new Map<string, string>();
  let atEnd: string | null = null;
  // The store lists the newest turn first.
  for (const turn of turns) {
    const reason = turnFailureReason(turn.error);
    if (!reason || !turn.terminalAt || turn.state === "completed") continue;
    const own = items.filter((item) => item.turnId === turn.id);
    if (own.some((item) => payloadText(item).includes(reason.slice(0, 80))))
      continue;
    const line = withReason(
      turn.state === "failed"
        ? "The response failed"
        : "The response ended early",
      reason,
    );
    const last = own[own.length - 1];
    if (last) afterItem.set(last.id, line);
    else if (
      !atEnd &&
      turn.createdAt >= (items[items.length - 1]?.createdAt ?? "")
    )
      atEnd = line;
  }
  return { afterItem, atEnd };
}

/**
 * The words a researcher should see for a warning from the assistant's
 * connection, or null for housekeeping that repairs itself.
 */
export function conversationWarning(message: string): string | null {
  if (
    message.startsWith("Ignored response for unknown or expired request") ||
    message.startsWith("Workbench projection lagged") ||
    message.startsWith("Workbench reconciled only") ||
    message.startsWith("Workbench skipped a turn")
  )
    return null;
  const failed = message.match(/^Workbench could not [^:]*: (.*)$/s);
  if (failed)
    return withReason(
      "Part of this conversation could not be saved, so messages may be missing from it",
      failed[1],
    );
  return message;
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

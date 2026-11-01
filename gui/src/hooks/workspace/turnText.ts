import { isMessage, payloadText, roleFor } from "../../lib/workspaceTranscript";
import type { ConversationSnapshot } from "../../lib/workbenchTypes";

/**
 * What a running turn has said so far, whichever conversation is on screen. The
 * saved transcript gets a message only once it is whole, so this is what lets a
 * conversation show its partial answer again when the researcher returns. One
 * turn runs at a time, so one record is enough; a new turn replaces it.
 */
export interface TurnText {
  turnId: string;
  messages: { itemId: string | null; text: string }[];
}

export function addTurnText(
  record: TurnText | null,
  turnId: string,
  itemId: string | null,
  delta: string,
): TurnText {
  const turn: TurnText =
    record?.turnId === turnId ? record : { turnId, messages: [] };
  const last = turn.messages.at(-1);
  if (last && (!itemId || !last.itemId || last.itemId === itemId)) {
    last.text += delta;
    last.itemId ??= itemId;
  } else turn.messages.push({ itemId, text: delta });
  return turn;
}

/** The turn's text the transcript does not show, and its newest message. */
export function unsavedTurnText(
  { messages }: TurnText,
  items: ConversationSnapshot["items"],
) {
  // Claude saves a whole message before its turn marks it finished, so any
  // saved text counts. Codex saves an empty message when one starts.
  const saved = new Set(
    items.filter(payloadText).map((item) => item.providerItemId),
  );
  const unsaved = messages.filter(
    (message) => !message.itemId || !saved.has(message.itemId),
  );
  return {
    text: unsaved.map((message) => message.text).join("\n\n"),
    itemId: unsaved.at(-1)?.itemId ?? null,
  };
}

/**
 * Whether the saved transcript already shows a submitted message. The host saves
 * it when the turn starts, so a repeat of an earlier message does not count: the
 * newest message from the researcher has to belong to this turn.
 */
export function transcriptHasPrompt(
  { items, turns }: Pick<ConversationSnapshot, "items" | "turns">,
  submission: { text: string; turnId?: string },
) {
  const newest = [...items]
    .reverse()
    .find((item) => isMessage(item) && roleFor(item) === "You");
  const turn = turns.find((candidate) => candidate.id === newest?.turnId);
  if (!newest || !turn) return false;
  return submission.turnId
    ? turn.providerTurnId === submission.turnId
    : !turn.terminalAt && payloadText(newest).trim() === submission.text;
}

import { expect, it } from "vitest";
import {
  addTurnText,
  transcriptHasPrompt,
  unsavedTurnText,
  type TurnText,
} from "./turnText";
import type { ConversationSnapshot } from "../../lib/workbenchTypes";

type Item = ConversationSnapshot["items"][number];
type Turn = ConversationSnapshot["turns"][number];
const item = (fields: Partial<Item>) =>
  ({ id: "item", isFinal: true, payload: { text: "Hi" }, ...fields }) as Item;
const user = (turnId: string | null, text = "Hi") =>
  item({ turnId, itemKind: "userMessage", payload: { text } });
const turn = (id: string, providerTurnId: string, terminalAt?: string) =>
  ({ id, providerTurnId, terminalAt: terminalAt ?? null }) as Turn;

it("keeps a turn's messages apart and starts over for a new turn", () => {
  let record: TurnText | null = addTurnText(null, "turn", "one", "Fir");
  record = addTurnText(record, "turn", "one", "st");
  record = addTurnText(record, "turn", "two", "Second");
  expect(unsavedTurnText(record, [])).toEqual({
    text: "First\n\nSecond",
    itemId: "two",
  });
  record = addTurnText(record, "next", "one", "Fresh");
  expect(unsavedTurnText(record, [])).toEqual({ text: "Fresh", itemId: "one" });
});

it("leaves out only messages the transcript shows", () => {
  let record = addTurnText(null, "turn", "one", "First");
  record = addTurnText(record, "turn", "two", "Second");
  const saved = (providerItemId: string, fields: Partial<Item> = {}) =>
    item({ providerItemId, itemKind: "agentMessage", ...fields });
  expect(unsavedTurnText(record, [saved("one")]).text).toBe("Second");
  expect(unsavedTurnText(record, [saved("two")])).toEqual({
    text: "First",
    itemId: "one",
  });
  expect(unsavedTurnText(record, [saved("one", { isFinal: false })]).text).toBe(
    "Second",
  );
  expect(
    unsavedTurnText(record, [saved("one", { payload: { text: "" } })]).text,
  ).toBe("First\n\nSecond");
});

it("matches an acknowledged prompt by its turn, whatever the saved text says", () => {
  const snapshot = {
    items: [user("record", "Hi, with attached context")],
    turns: [turn("record", "provider")],
  };
  expect(
    transcriptHasPrompt(snapshot, { text: "Hi", turnId: "provider" }),
  ).toBe(true);
  expect(transcriptHasPrompt(snapshot, { text: "Hi", turnId: "other" })).toBe(
    false,
  );
});

it("does not take an identical earlier message for an unacknowledged prompt", () => {
  const items = [user("record")];
  expect(
    transcriptHasPrompt(
      { items, turns: [turn("record", "provider", "finished")] },
      { text: "Hi" },
    ),
  ).toBe(false);
  expect(
    transcriptHasPrompt(
      { items, turns: [turn("record", "provider")] },
      { text: "Hi" },
    ),
  ).toBe(true);
  expect(transcriptHasPrompt({ items: [], turns: [] }, { text: "Hi" })).toBe(
    false,
  );
});

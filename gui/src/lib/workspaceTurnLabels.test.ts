import { describe, expect, it } from "vitest";
import type { ConversationTurn, TranscriptItem } from "./workbenchTypes";
import {
  conversationWarning,
  turnFailureReason,
  turnFailures,
  turnStateLabel,
  turnTimeLabel,
} from "./workspaceTurnLabels";

it.each([
  ["inProgress", null, "Working"],
  ["completed", "2026-11-01T10:00:00Z", "Completed"],
  ["interrupted", "2026-11-01T10:00:00Z", "Stopped"],
  ["failed", "2026-11-01T10:00:00Z", "Failed"],
  ["somethingNew", "2026-11-01T10:00:00Z", "Finished"],
  ["somethingNew", null, "Working"],
])("labels a %s turn", (state, terminalAt, label) => {
  expect(turnStateLabel({ state, terminalAt })).toBe(label);
});

it("formats a stored time and keeps an unreadable one as stored", () => {
  const stored = "2026-11-01T10:30:00Z";
  expect(turnTimeLabel(stored)).not.toContain("T10:30");
  expect(turnTimeLabel(stored)).toMatch(/2026/);
  expect(turnTimeLabel("not a time")).toBe("not a time");
});

const turn = (over: Partial<ConversationTurn>): ConversationTurn => ({
  id: "turn",
  bindingId: "binding",
  clientSubmissionId: "submission",
  providerTurnId: "provider-turn",
  state: "failed",
  error: null,
  createdAt: "2026-11-01T10:00:00Z",
  updatedAt: "2026-11-01T10:00:00Z",
  terminalAt: "2026-11-01T10:00:01Z",
  ...over,
});
const item = (id: string, text: string, turnId = "turn"): TranscriptItem => ({
  id,
  turnId,
  providerItemId: id,
  itemKind: "agentMessage",
  payload: { text },
  isFinal: true,
  createdAt: "2026-11-01T10:00:00Z",
  updatedAt: "2026-11-01T10:00:00Z",
});

describe("turnFailureReason", () => {
  it("reads each runtime's saved shape", () => {
    expect(
      turnFailureReason({
        message: "You have reached your usage limit.",
        codexErrorInfo: "usageLimitExceeded",
        additionalDetails: null,
      }),
    ).toBe("You have reached your usage limit.");
    expect(turnFailureReason("The API refused the request")).toBe(
      "The API refused the request",
    );
  });

  it("has no reason for an empty or unknown value", () => {
    for (const value of [null, undefined, "", "  ", 4, {}, { message: 4 }])
      expect(turnFailureReason(value)).toBeNull();
  });

  it("shortens a very long reason", () => {
    expect(turnFailureReason("x".repeat(5000))!.length).toBeLessThan(700);
  });
});

describe("turnFailures", () => {
  it("places a ChatGPT failure after the last message of its turn", () => {
    const failures = turnFailures(
      [turn({ error: { message: "Context window exceeded" } })],
      [item("question", "Question"), item("partial", "Partial answer")],
    );
    expect([...failures.afterItem]).toEqual([
      ["partial", "The response failed: Context window exceeded."],
    ]);
    expect(failures.atEnd).toBeNull();
  });

  it("names a turn that recovery closed as ended early", () => {
    const failures = turnFailures(
      [
        turn({
          state: "interrupted",
          error: {
            message: "The connection ended; this turn was not replayed.",
          },
        }),
      ],
      [item("question", "Question")],
    );
    expect(failures.afterItem.get("question")).toBe(
      "The response ended early: The connection ended; this turn was not replayed.",
    );
  });

  it("leaves out a failure the transcript already shows", () => {
    const failures = turnFailures(
      [turn({ error: "The server refused the request" })],
      [
        item(
          "error",
          "The API could not complete this turn: The server refused the request",
        ),
      ],
    );
    expect(failures.afterItem.size).toBe(0);
    expect(failures.atEnd).toBeNull();
  });

  it("ignores turns that finished, are running, or gave no reason", () => {
    const failures = turnFailures(
      [
        turn({ id: "a", state: "completed", error: { message: "x" } }),
        turn({ id: "b", terminalAt: null, error: { message: "x" } }),
        turn({ id: "c", state: "interrupted" }),
      ],
      [item("1", "One", "a"), item("2", "Two", "b"), item("3", "Three", "c")],
    );
    expect(failures.afterItem.size).toBe(0);
  });

  it("shows only the newest turn with no message, and only after the transcript", () => {
    expect(
      turnFailures(
        [
          turn({ id: "new", error: "Newest" }),
          turn({ id: "old", error: "Older" }),
        ],
        [],
      ).atEnd,
    ).toBe("The response failed: Newest.");
    expect(
      turnFailures(
        [turn({ id: "paged-out", error: "Older" })],
        [{ ...item("later", "Later", "x"), createdAt: "2026-11-02T00:00:00Z" }],
      ).atEnd,
    ).toBeNull();
  });
});

describe("conversationWarning", () => {
  it("keeps what the researcher must act on", () => {
    const message =
      "The working copy could not be captured: a file is too large. Its files are retained; refresh the working copy before accepting changes.";
    expect(conversationWarning(message)).toBe(message);
  });

  it("explains a transcript gap in plain words", () => {
    expect(
      conversationWarning(
        "Workbench could not persist a Codex event: disk full",
      ),
    ).toBe(
      "Part of this conversation could not be saved, so messages may be missing from it: disk full.",
    );
  });

  it.each([
    "Ignored response for unknown or expired request 7",
    "Workbench projection lagged by 3 events; reconciling from thread/turns/list",
    "Workbench reconciled only the newest 200 turns of a long conversation",
    "Workbench skipped a turn that moved during reconciliation; a later reconciliation retries it",
  ])("does not show housekeeping: %s", (message) => {
    expect(conversationWarning(message)).toBeNull();
  });
});

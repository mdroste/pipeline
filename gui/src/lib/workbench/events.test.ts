import { expect, it, vi } from "vitest";
import { decodeConversationEvent, subscribeConversationEvents } from "./events";
import type { WorkbenchEvent } from "./runtimeTypes";
import { nativeEventHarness } from "../../test/nativeEventHarness";

it("preserves extensions and malformed known payloads without inventing identity", () => {
  const extended = {
    kind: "futureEvent",
    epoch: 4,
    extra: { untouched: true },
  };
  expect(decodeConversationEvent(extended)).toEqual({
    known: false,
    event: extended,
  });
  expect(
    decodeConversationEvent({ kind: "turnCompleted", epoch: 4 }).known,
  ).toBe(false);
  const event = {
    kind: "turnCompleted",
    epoch: 4,
    threadId: "t",
    turnId: "u",
    status: "completed",
    extra: true,
  };
  const decoded = decodeConversationEvent(event);
  expect(decoded).toEqual({ known: true, event });
  if (decoded.known && decoded.event.kind === "turnCompleted")
    expect(decoded.event.status.toUpperCase()).toBe("COMPLETED");
});

it("disposes late registration once and suppresses late delivery", async () => {
  const native = nativeEventHarness<WorkbenchEvent>();
  const receive = vi.fn(),
    fail = vi.fn();
  const dispose = subscribeConversationEvents(receive, fail, native.register);
  dispose();
  native.emit({ kind: "protocolWarning", epoch: 1, message: "late" });
  native.ready();
  await Promise.resolve();
  dispose();
  expect(native.subscribers).toBe(0);
  expect(receive).not.toHaveBeenCalled();
  expect(fail).not.toHaveBeenCalled();
});

it("reports setup failure only while the owner is live", async () => {
  const fail = vi.fn();
  subscribeConversationEvents(vi.fn(), fail, () =>
    Promise.reject("unavailable"),
  );
  await Promise.resolve();
  expect(fail).toHaveBeenCalledExactlyOnceWith("unavailable");
});

import { expect, it } from "vitest";
import { pendingRequestSessions } from "./workspaceAttention";
it("retains attention until each pending request resolves and distinguishes numeric and string IDs", () => {
  let ids = pendingRequestSessions(new Map(), {
    epoch: 1,
    kind: "serverRequest",
    requestId: 1,
  });
  ids = pendingRequestSessions(ids, {
    epoch: 1,
    kind: "serverRequest",
    requestId: "1",
    sessionId: "conversation-a",
  });
  ids = pendingRequestSessions(ids, {
    epoch: 1,
    kind: "serverRequestResolved",
    requestId: 1,
  });
  expect([...ids]).toEqual([['"1"', "conversation-a"]]);
  ids = pendingRequestSessions(ids, {
    epoch: 1,
    kind: "serverRequestResolved",
    requestId: 1,
  });
  expect(ids.size).toBe(1);
  expect(
    pendingRequestSessions(ids, { epoch: 1, kind: "connectionClosed" }).size,
  ).toBe(0);
});

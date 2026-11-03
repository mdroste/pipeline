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

it("returns the same map for an event that changes nothing", () => {
  const ids = new Map([['"1"', "conversation-a"]]);
  for (const event of [
    { kind: "agentMessageDelta", delta: "word", itemId: "item" },
    { kind: "turnStarted", requestId: 9 },
    { kind: "serverRequestResolved", requestId: "unknown" },
    { kind: "serverRequest", requestId: "1", sessionId: "conversation-a" },
  ])
    expect(pendingRequestSessions(ids, { epoch: 1, ...event })).toBe(ids);
  const none = new Map();
  expect(
    pendingRequestSessions(none, { epoch: 1, kind: "connectionClosed" }),
  ).toBe(none);
});

it("leaves the map it was given unchanged", () => {
  const ids = new Map([['"1"', null]]);
  pendingRequestSessions(ids, {
    epoch: 1,
    kind: "serverRequest",
    requestId: 2,
  });
  pendingRequestSessions(ids, {
    epoch: 1,
    kind: "serverRequestResolved",
    requestId: "1",
  });
  expect([...ids]).toEqual([['"1"', null]]);
});

it("records a later conversation for a request it already holds", () => {
  const ids = pendingRequestSessions(new Map([["1", null]]), {
    epoch: 1,
    kind: "serverRequest",
    requestId: 1,
    sessionId: "conversation-a",
  });
  expect([...ids]).toEqual([["1", "conversation-a"]]);
});

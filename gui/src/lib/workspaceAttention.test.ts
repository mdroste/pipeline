import { expect, it } from "vitest";
import { pendingRequestIds } from "./workspaceAttention";
it("retains attention until each pending request resolves and distinguishes numeric and string IDs", () => {
  let ids = pendingRequestIds(new Set(), {
    epoch: 1,
    kind: "serverRequest",
    requestId: 1,
  });
  ids = pendingRequestIds(ids, {
    epoch: 1,
    kind: "serverRequest",
    requestId: "1",
  });
  ids = pendingRequestIds(ids, {
    epoch: 1,
    kind: "serverRequestResolved",
    requestId: 1,
  });
  expect([...ids]).toEqual(['"1"']);
  ids = pendingRequestIds(ids, {
    epoch: 1,
    kind: "serverRequestResolved",
    requestId: 1,
  });
  expect(ids.size).toBe(1);
  expect(
    pendingRequestIds(ids, { epoch: 1, kind: "connectionClosed" }).size,
  ).toBe(0);
});

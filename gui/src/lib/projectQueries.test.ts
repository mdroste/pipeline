import { beforeEach, expect, it, vi } from "vitest";
import { readProjectHome, invalidatesProjectHome } from "./projectQueries";
import { projectClient } from "./projectClient";
import { workspaceClient } from "./workbench/workspaceClient";
vi.mock("./projectClient", () => ({ projectClient: { home: vi.fn() } }));
vi.mock("./workbench/workspaceClient", () => ({
  workspaceClient: { getWorkspace: vi.fn() },
}));
beforeEach(() => vi.clearAllMocks());
it("shares only in-flight reads within the same project", async () => {
  const first = readProjectHome("a");
  expect(readProjectHome("a")).toBe(first);
  expect(readProjectHome("b")).not.toBe(first);
  await first;
  await readProjectHome("a");
  expect(projectClient.home).toHaveBeenCalledTimes(3);
  expect(workspaceClient.getWorkspace).toHaveBeenCalledTimes(3);
});
it("scopes identified turns and preserves older host invalidations", () => {
  expect(
    invalidatesProjectHome(
      { kind: "turnCompleted", workspaceId: "b", epoch: 1 },
      "a",
    ),
  ).toBe(false);
  expect(invalidatesProjectHome({ kind: "turnCompleted", epoch: 1 }, "a")).toBe(
    true,
  );
});

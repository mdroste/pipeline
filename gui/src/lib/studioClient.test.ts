import { beforeEach, expect, it, vi } from "vitest";
import { resultRef, studioClient } from "./studioClient";
import type { ResearchResultV1 } from "./workbenchTypes";
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
beforeEach(() => invoke.mockReset());
it("never replays an uncertain file mutation", async () => {
  invoke.mockResolvedValue({ outcome: "unknown" });
  await expect(
    studioClient.mutate("workspace", {
      action: "saveText",
      checkpointId: null,
      path: "main.tex",
      expectedHash: "old",
      content: "new",
    }),
  ).rejects.toThrow("could not confirm whether this finished");
  expect(invoke).toHaveBeenCalledTimes(1);
});
it("keeps result identities scoped to the execution", () => {
  expect(
    resultRef({
      sourceExecutionId: "alternative",
      resultId: "coefficient",
    } as ResearchResultV1),
  ).toEqual({ executionId: "alternative", resultId: "coefficient" });
});
it("requests bounded logs with an explicit cursor and project scope", async () => {
  invoke.mockResolvedValue({ text: "chunk", nextOffset: 15, truncated: false });
  await studioClient.log("workspace", "execution", "stderr", 10);
  expect(invoke).toHaveBeenCalledExactlyOnceWith("workbench_job_log", {
    workspaceId: "workspace",
    executionId: "execution",
    stream: "stderr",
    offset: 10,
  });
});
it("does not initiate Zotero write authorization", async () => {
  invoke.mockResolvedValue({ items: [] });
  await studioClient.zotero("ABCDEFGH", 100, "instance");
  expect(invoke).toHaveBeenCalledExactlyOnceWith("workbench_zotero_preview", {
    collection: "ABCDEFGH",
    start: 100,
    expectedServer: "instance",
  });
});

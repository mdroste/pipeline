import { beforeEach, expect, it, vi } from "vitest";
import { studioClient } from "./studioClient";
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

import fixture from "../../tests/fixtures/workbench/studio-contracts-v1.json";
it("preserves the shared native action fixture and unchanged-save result", async () => {
  invoke.mockResolvedValue(fixture.unchanged);
  const action = { ...fixture.saveText, action: "saveText" as const };
  expect(await studioClient.mutate("fixture", action)).toEqual(
    fixture.unchanged,
  );
  expect(invoke).toHaveBeenCalledWith("workbench_studio_mutate", {
    request: expect.objectContaining({
      workspaceId: "fixture",
      ...fixture.saveText,
    }),
  });
});

import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { createReviewSetupReads } from "./reviewSetupReads";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

it("shares config, profile list and active selection for one setup revision", async () => {
  vi.mocked(invoke).mockResolvedValue(null);
  const reads = createReviewSetupReads();
  await Promise.all([reads.workflow(), reads.workflow(), reads.config()]);
  expect(vi.mocked(invoke).mock.calls.map(([command]) => command)).toEqual([
    "get_pipeline_config",
    "list_profiles",
    "get_active_profile",
  ]);
  await createReviewSetupReads().workflow();
  expect(invoke).toHaveBeenCalledTimes(6);
});

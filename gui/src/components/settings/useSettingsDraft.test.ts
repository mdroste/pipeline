import { renderHook, act, waitFor } from "@testing-library/react";
import { it, expect, vi } from "vitest";
import { useSettingsDraft } from "./useSettingsDraft";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./useProviderCatalogs", () => ({
  useProviderCatalogs: () => ({ initialize: () => undefined }),
}));

it("offers to discard a stored key that no longer decrypts and reloads afterwards", async () => {
  let undecryptable = ["google"];
  invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "get_settings")
      return Promise.resolve({
        settings: { active_profile: "auto-review" },
        warnings: undecryptable.length
          ? ["Could not decrypt Google API key: Decryption failed."]
          : [],
        undecryptable_keys: undecryptable,
      });
    if (cmd === "discard_undecryptable_secret") {
      expect(args).toEqual({ provider: "google" });
      undecryptable = [];
      return Promise.resolve();
    }
    return Promise.reject(new Error(`unexpected ${cmd}`));
  });
  const { result } = renderHook(() => useSettingsDraft());
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.undecryptableKeys).toEqual(["google"]);
  await act(async () => {
    await result.current.discardStoredKey("google");
  });
  expect(invoke).toHaveBeenCalledWith("discard_undecryptable_secret", {
    provider: "google",
  });
  expect(result.current.undecryptableKeys).toEqual([]);
  expect(result.current.warnings).toEqual([]);
});

import { renderHook, act, waitFor } from "@testing-library/react";
import { it, expect, vi } from "vitest";
import { useSettingsDraft } from "./useSettingsDraft";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./useProviderCatalogs", () => ({
  useProviderCatalogs: () => ({
    initialize: () => undefined,
    didSave: () => undefined,
  }),
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

it("saves the same edit again after a failed save and a revert", async () => {
  invoke.mockReset();
  const saved: unknown[] = [];
  let failNext = true;
  invoke.mockImplementation((cmd: string, args?: { settings?: unknown }) => {
    if (cmd === "get_settings")
      return Promise.resolve({
        settings: { max_workers: 4 },
        warnings: [],
        undecryptable_keys: [],
      });
    if (cmd === "save_settings") {
      if (failNext) {
        failNext = false;
        return Promise.reject(new Error("Disk full"));
      }
      saved.push(args?.settings);
    }
    return Promise.resolve();
  });
  const { result } = renderHook(() => useSettingsDraft());
  await waitFor(() => expect(result.current.loading).toBe(false));
  const original = result.current.settings!;
  const edited = { ...original, max_workers: 8 };

  act(() => result.current.setSettings(edited));
  await waitFor(() => expect(result.current.saveError).toBe("Disk full"));
  // A failed save keeps the edit on screen and unsaved.
  expect(result.current.settings).toEqual(edited);
  expect(result.current.dirty).toBe(true);

  act(() => result.current.setSettings({ ...original }));
  expect(result.current.dirty).toBe(false);
  act(() => result.current.setSettings({ ...edited }));

  await waitFor(() => expect(saved).toEqual([{ max_workers: 8 }]));
  await waitFor(() => expect(result.current.dirty).toBe(false));
  expect(result.current.saveError).toBeNull();
});

it("keeps retrying a newer edit that follows a failed save", async () => {
  invoke.mockReset();
  const saved: unknown[] = [];
  let failNext = true;
  invoke.mockImplementation((cmd: string, args?: { settings?: unknown }) => {
    if (cmd === "get_settings")
      return Promise.resolve({
        settings: { max_workers: 4 },
        warnings: [],
        undecryptable_keys: [],
      });
    if (cmd === "save_settings") {
      if (failNext) {
        failNext = false;
        return Promise.reject(new Error("Disk full"));
      }
      saved.push(args?.settings);
    }
    return Promise.resolve();
  });
  const { result } = renderHook(() => useSettingsDraft());
  await waitFor(() => expect(result.current.loading).toBe(false));
  const original = result.current.settings!;
  act(() => result.current.setSettings({ ...original, max_workers: 8 }));
  await waitFor(() => expect(result.current.saveError).toBe("Disk full"));
  act(() => result.current.setSettings({ ...original, max_workers: 9 }));
  await waitFor(() => expect(saved).toEqual([{ max_workers: 9 }]));
  await waitFor(() => expect(result.current.dirty).toBe(false));
});

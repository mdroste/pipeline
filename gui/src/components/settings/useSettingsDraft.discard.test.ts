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

it("discarding a stored key preserves unsaved settings", async () => {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_settings")
      return Promise.resolve({
        settings: { max_workers: 4, active_profile: "auto-review" },
        warnings: [],
        undecryptable_keys: ["google"],
      });
    return Promise.resolve();
  });
  const { result, unmount } = renderHook(() => useSettingsDraft());
  await waitFor(() => expect(result.current.loading).toBe(false));
  act(() =>
    result.current.setSettings({ ...result.current.settings!, max_workers: 8 }),
  );
  expect(result.current.dirty).toBe(true);
  await act(async () => {
    await result.current.discardStoredKey("google");
  });
  expect(result.current.settings?.max_workers).toBe(8);
  expect(result.current.dirty).toBe(true);
  expect(invoke).not.toHaveBeenCalledWith("save_settings", expect.anything());
  unmount();
});

it("serializes discard after an in-flight save and preserves newer edits", async () => {
  invoke.mockClear();
  let finishSave!: () => void;
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_settings")
      return Promise.resolve({
        settings: { max_workers: 4 },
        warnings: [],
        undecryptable_keys: [],
      });
    if (cmd === "save_settings")
      return new Promise<void>((resolve) => {
        finishSave = resolve;
      });
    return Promise.resolve();
  });
  const { result, unmount } = renderHook(() => useSettingsDraft());
  await waitFor(() => expect(result.current.loading).toBe(false));
  act(() =>
    result.current.setSettings({ ...result.current.settings!, max_workers: 8 }),
  );
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_settings", expect.anything()),
  );
  let discarding!: Promise<void>;
  act(() => {
    discarding = result.current.discardStoredKey("google");
  });
  expect(invoke).not.toHaveBeenCalledWith(
    "discard_undecryptable_secret",
    expect.anything(),
  );
  act(() =>
    result.current.setSettings({
      ...result.current.settings!,
      max_workers: 12,
    }),
  );
  await act(async () => {
    finishSave();
    await discarding;
  });
  expect(result.current.settings?.max_workers).toBe(12);
  expect(result.current.dirty).toBe(true);
  unmount();
});

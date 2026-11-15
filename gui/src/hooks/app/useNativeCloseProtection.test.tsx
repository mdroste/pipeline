import { useDraftFlush } from "../useDraftFlush";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useNativeCloseProtection } from "./useNativeCloseProtection";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  confirm: vi.fn(),
  notify: vi.fn(),
  destroy: vi.fn(),
  stopClose: vi.fn(),
  stopQuit: vi.fn(),
  native: undefined as undefined | ((event: unknown) => Promise<void>),
  quit: undefined as undefined | (() => Promise<void>),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", async (original) => ({
  ...(await original<typeof import("@tauri-apps/api/event")>()),
  listen: mocks.listen,
}));
vi.mock("../../components/DialogService", () => ({
  confirmDialog: mocks.confirm,
  notify: mocks.notify,
}));
vi.mock("@tauri-apps/api/window", async (original) => {
  const actual = await original<typeof import("@tauri-apps/api/window")>();
  const window = {
    listen: vi.fn(async (_name: string, callback: typeof mocks.native) => {
      mocks.native = callback;
      return mocks.stopClose;
    }),
    destroy: mocks.destroy,
    // Exercise the installed Tauri wrapper, including its implicit destroy.
    onCloseRequested: actual.Window.prototype.onCloseRequested,
  };
  return { ...actual, getCurrentWindow: () => window };
});

beforeEach(() => {
  vi.clearAllMocks();
  mocks.native = undefined;
  mocks.quit = undefined;
  mocks.invoke.mockImplementation(async (command: string) =>
    command === "app_close_hides" ? false : undefined,
  );
  mocks.listen.mockImplementation(
    async (_name: string, callback: typeof mocks.quit) => {
      mocks.quit = callback;
      return mocks.stopQuit;
    },
  );
  mocks.confirm.mockResolvedValue(false);
});

async function setup(dirty = false) {
  const ref = {
    current: {
      workflowDirty: dirty,
      settingsDirty: false,
      projectsDirty: false,
      tasksDirty: false,
    },
  };
  const hook = renderHook(() => useNativeCloseProtection(ref));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "set_app_close_guard",
      expect.objectContaining({ active: true }),
    ),
  );
  return { ...hook, ref };
}
async function closeWindow() {
  await act(async () => {
    await mocks.native?.({
      event: "tauri://close-requested",
      id: 1,
      payload: null,
    });
  });
}

it("hides a clean background window without Tauri destroying it", async () => {
  mocks.invoke.mockImplementation(async (command: string) =>
    command === "app_close_hides" ? true : undefined,
  );
  await setup();
  await closeWindow();
  expect(mocks.invoke).toHaveBeenCalledWith("complete_app_close", {
    action: "hide",
  });
  expect(mocks.destroy).not.toHaveBeenCalled();
  expect(mocks.confirm).not.toHaveBeenCalled();
});

it("keeps dirty drafts when hiding, but asks before tray or menu Quit", async () => {
  mocks.invoke.mockImplementation(async (command: string) =>
    command === "app_close_hides" ? true : undefined,
  );
  const { ref } = await setup(true);
  await closeWindow();
  expect(ref.current.workflowDirty).toBe(true);
  expect(mocks.confirm).not.toHaveBeenCalled();
  await act(async () => {
    await mocks.quit?.();
  });
  expect(mocks.confirm).toHaveBeenCalledOnce();
  expect(mocks.invoke).not.toHaveBeenCalledWith("complete_app_close", {
    action: "quit",
  });
  expect(mocks.destroy).not.toHaveBeenCalled();
});

it("allows a confirmed quit and keeps Cancel open", async () => {
  await setup(true);
  await closeWindow();
  expect(mocks.invoke).not.toHaveBeenCalledWith("complete_app_close", {
    action: "quit",
  });
  mocks.confirm.mockResolvedValueOnce(true);
  await closeWindow();
  expect(mocks.invoke).toHaveBeenCalledWith("complete_app_close", {
    action: "quit",
  });
  expect(mocks.destroy).not.toHaveBeenCalled();
});

it("quits a clean window through the application handler", async () => {
  await setup();
  await closeWindow();
  expect(mocks.invoke).toHaveBeenCalledWith("complete_app_close", {
    action: "quit",
  });
  expect(mocks.destroy).not.toHaveBeenCalled();
});

it("keeps the window intact after a close failure and permits a retry", async () => {
  await setup();
  mocks.invoke.mockImplementationOnce(async () => {
    throw new Error("connection unavailable");
  });
  await closeWindow();
  expect(mocks.notify).toHaveBeenCalledWith(
    expect.stringContaining("connection unavailable"),
  );
  expect(mocks.destroy).not.toHaveBeenCalled();
  expect(mocks.invoke).not.toHaveBeenCalledWith("complete_app_close", {
    action: "quit",
  });
  await closeWindow();
  expect(mocks.invoke).toHaveBeenCalledWith("complete_app_close", {
    action: "quit",
  });
});

it("does not queue duplicate quit confirmations and removes both listeners", async () => {
  const { unmount } = await setup(true);
  let answer!: (value: boolean) => void;
  mocks.confirm.mockImplementationOnce(
    () =>
      new Promise<boolean>((resolve) => {
        answer = resolve;
      }),
  );
  let first!: Promise<void>;
  await act(async () => {
    first = mocks.quit!();
  });
  await act(async () => {
    await mocks.quit?.();
  });
  expect(mocks.confirm).toHaveBeenCalledOnce();
  await act(async () => {
    answer(false);
    await first;
  });
  unmount();
  expect(mocks.stopClose).toHaveBeenCalledOnce();
  expect(mocks.stopQuit).toHaveBeenCalledOnce();
});

it("flushes drafts before quitting and keeps the window open if retention fails", async () => {
  const flush = vi.fn(() => false);
  renderHook(() => useDraftFlush(flush));
  await setup();
  await closeWindow();
  expect(flush).toHaveBeenCalledOnce();
  expect(mocks.invoke).not.toHaveBeenCalledWith("complete_app_close", {
    action: "quit",
  });
  expect(mocks.notify).toHaveBeenCalledWith(
    expect.stringContaining("Save your files"),
  );
  flush.mockReturnValue(true);
  await closeWindow();
  expect(mocks.invoke).toHaveBeenCalledWith("complete_app_close", {
    action: "quit",
  });
});

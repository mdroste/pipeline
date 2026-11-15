import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { appEvents } from "../lib/appEvents";
import useConversationIndex from "./useConversationIndex";

const mocks = vi.hoisted(() => ({
  index: vi.fn(),
  listWorkspaces: vi.fn(),
  listeners: [] as Array<(event: { payload: { kind: string } }) => void>,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_name: string, listener: any) => {
    mocks.listeners.push(listener);
    return () => {
      mocks.listeners.splice(mocks.listeners.indexOf(listener), 1);
    };
  }),
}));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: { index: mocks.index },
}));
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: { listWorkspaces: mocks.listWorkspaces },
}));

const session = (id: string, updatedAt: string) => ({ id, updatedAt });

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listeners.length = 0;
  mocks.listWorkspaces.mockResolvedValue({ workspaces: [] });
  mocks.index.mockImplementation(
    async (_archived, _unfiled, _query, _palette, cursor) =>
      cursor
        ? {
            items: [
              { session: session("older", "2026-01-01"), projectName: null },
            ],
            nextCursor: null,
          }
        : {
            items: [
              { session: session("newest", "2026-02-01"), projectName: null },
            ],
            nextCursor: { updatedAt: "2026-02-01", id: "newest" },
          },
  );
});

it("continues from the cursor when older chats are requested", async () => {
  const { result } = renderHook(() => useConversationIndex());
  await waitFor(() => expect(result.current.loading).toBe(false));
  act(() => result.current.loadMore());
  await waitFor(() => expect(result.current.conversations.length).toBe(2));
  expect(result.current.conversations.map(({ session }) => session.id)).toEqual(
    ["newest", "older"],
  );
});

it("stops at the newest page for a caller that shows only those", async () => {
  const { result } = renderHook(() => useConversationIndex());
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.conversations.map(({ session }) => session.id)).toEqual(
    ["newest"],
  );
  expect(mocks.index).toHaveBeenCalledTimes(1);
});

const reads = () => mocks.index.mock.calls.length;

it.each(["sessionTitleUpdated", "turnCompleted"])(
  "reads the list again after %s",
  async (kind) => {
    const { result, unmount } = renderHook(() => useConversationIndex());
    await waitFor(() => expect(mocks.listeners).toHaveLength(1));
    await waitFor(() => expect(result.current.loading).toBe(false));
    const before = reads();
    act(() => mocks.listeners[0]({ payload: { kind } }));
    await waitFor(() => expect(reads()).toBeGreaterThan(before));
    unmount();
  },
);

it("reads the list again after a conversation changes here", async () => {
  const { result, unmount } = renderHook(() => useConversationIndex());
  await waitFor(() => expect(result.current.loading).toBe(false));
  mocks.index.mockImplementation(async () => ({
    items: [{ session: session("renamed", "2026-03-01"), projectName: null }],
    nextCursor: null,
  }));
  act(() => appEvents.emit("conversations-changed", {}));
  await waitFor(() =>
    expect(result.current.conversations[0]?.session.id).toBe("renamed"),
  );
  unmount();
});

it("ignores other events and stops listening when closed", async () => {
  const { result, unmount } = renderHook(() => useConversationIndex());
  await waitFor(() => expect(mocks.listeners).toHaveLength(1));
  await waitFor(() => expect(result.current.loading).toBe(false));
  const before = reads();
  act(() => mocks.listeners[0]({ payload: { kind: "agentMessageDelta" } }));
  unmount();
  await waitFor(() => expect(mocks.listeners).toHaveLength(0));
  act(() => appEvents.emit("conversations-changed", {}));
  expect(reads()).toBe(before);
});

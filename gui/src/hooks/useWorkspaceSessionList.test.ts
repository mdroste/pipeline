import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import useWorkspaceSessionList, {
  useOlderSessions,
} from "./useWorkspaceSessionList";

const mocks = vi.hoisted(() => ({ listSessions: vi.fn() }));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: mocks,
}));

function setup(onError?: (message: string) => void) {
  const setSessions = vi.fn();
  const setSessionId = vi.fn();
  const workspace = { current: "project" as string | null };
  const sequence = { current: 0 };
  const { result } = renderHook(() =>
    useWorkspaceSessionList(
      workspace,
      sequence,
      setSessions,
      setSessionId,
      false,
      onError,
    ),
  );
  return { load: result.current, setSessions, setSessionId, workspace };
}

beforeEach(() => {
  vi.clearAllMocks();
});

it("reports a failed conversation list instead of rejecting", async () => {
  mocks.listSessions.mockRejectedValue({
    message: "Failed to list sessions",
  });
  const onError = vi.fn();
  const { load, setSessions, setSessionId } = setup(onError);

  await expect(load("project")).resolves.toBeUndefined();

  expect(onError).toHaveBeenCalledWith(
    "Could not load the conversation list: Failed to list sessions",
  );
  expect(setSessions).not.toHaveBeenCalled();
  expect(setSessionId).not.toHaveBeenCalled();
});

it("does not report a failure for a project that is no longer selected", async () => {
  const onError = vi.fn();
  const harness = setup(onError);
  mocks.listSessions.mockImplementation(async () => {
    harness.workspace.current = "other";
    throw new Error("Failed to list sessions");
  });

  await harness.load("project");

  expect(onError).not.toHaveBeenCalled();
});

it("propagates the failure when no error handler is supplied", async () => {
  mocks.listSessions.mockRejectedValue(new Error("Failed to list sessions"));
  const { load } = setup();

  await expect(load("project")).rejects.toThrow("Failed to list sessions");
});

function pagedSetup() {
  let sessions: { id: string }[] = [];
  const setSessions = vi.fn((update: unknown) => {
    sessions =
      typeof update === "function" ? update(sessions) : (update as never);
  });
  const setSessionId = vi.fn();
  const workspace = { current: "project" as string | null };
  const sequence = { current: 0 };
  const { result } = renderHook(() => {
    const older = useOlderSessions(
      workspace,
      sequence,
      setSessions as never,
      false,
      vi.fn(),
    );
    return {
      older,
      load: useWorkspaceSessionList(
        workspace,
        sequence,
        setSessions as never,
        setSessionId,
        false,
        undefined,
        older,
      ),
    };
  });
  return { result, listed: () => sessions.map((session) => session.id) };
}

const cursor = (id: string) => ({ updatedAt: "2026-01-01", id });
const pages: Record<string, unknown> = {
  first: {
    sessions: [{ id: "a" }, { id: "b" }],
    nextCursor: cursor("b"),
  },
  b: { sessions: [{ id: "c" }], nextCursor: cursor("c") },
  c: { sessions: [{ id: "d" }], nextCursor: null },
};

it("reads older conversations a page at a time and says when none remain", async () => {
  mocks.listSessions.mockImplementation(async (_w, _a, next) =>
    next ? pages[next.id] : pages.first,
  );
  const { result, listed } = pagedSetup();
  await act(() => result.current.load("project"));
  expect(listed()).toEqual(["a", "b"]);
  expect(result.current.older.hasOlder).toBe(true);

  await act(() => result.current.older.loadOlder());
  expect(listed()).toEqual(["a", "b", "c"]);
  expect(result.current.older.hasOlder).toBe(true);

  await act(() => result.current.older.loadOlder());
  expect(listed()).toEqual(["a", "b", "c", "d"]);
  expect(result.current.older.hasOlder).toBe(false);
});

it("a reload keeps the older pages already on screen", async () => {
  mocks.listSessions.mockImplementation(async (_w, _a, next) =>
    next ? pages[next.id] : pages.first,
  );
  const { result, listed } = pagedSetup();
  await act(() => result.current.load("project"));
  await act(() => result.current.older.loadOlder());

  await act(() => result.current.load("project"));
  expect(listed()).toEqual(["a", "b", "c"]);
  expect(result.current.older.hasOlder).toBe(true);
});

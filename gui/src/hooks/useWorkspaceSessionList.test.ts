import { renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import useWorkspaceSessionList from "./useWorkspaceSessionList";

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

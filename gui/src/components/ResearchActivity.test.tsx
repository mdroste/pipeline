import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import ResearchActivity from "./ResearchActivity";
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  list: vi.fn(),
  stop: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("../lib/taskClient", () => ({ taskClient: { list: mocks.list } }));
vi.mock("../lib/workbenchClient", () => ({
  workbenchClient: { interruptTurn: mocks.stop },
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
it("keeps other runtime owners visible when one adapter fails and targets exact owners", async () => {
  mocks.list.mockRejectedValue(new Error("Task adapter unavailable"));
  mocks.invoke.mockImplementation(async (command) =>
    command === "list_runs"
      ? [
          {
            run_id: "review-old",
            title: "Interrupted review",
            status: "interrupted",
          },
        ]
      : {
          turns: [
            {
              sessionId: "session",
              title: "Working conversation",
              threadId: "thread",
              turnId: "turn",
              state: "running",
            },
          ],
          jobs: [],
          pendingRequests: [],
          researchAttention: 0,
        },
  );
  mocks.stop.mockResolvedValue(undefined);
  const props = {
    onClose: vi.fn(),
    onSession: vi.fn(),
    onProject: vi.fn(),
    onTasks: vi.fn(),
    onReview: vi.fn(),
  };
  render(<ResearchActivity {...props} />);
  expect(await screen.findByText("Working conversation")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent(
    "Task adapter unavailable",
  );
  fireEvent.click(screen.getByRole("button", { name: "Open Review controls" }));
  expect(props.onReview).toHaveBeenCalledWith("review-old");
  fireEvent.click(
    screen.getByRole("button", { name: "Open conversation and requests" }),
  );
  expect(props.onSession).toHaveBeenCalledWith("session");
  fireEvent.click(screen.getByRole("button", { name: "Stop this turn" }));
  expect(mocks.stop).toHaveBeenCalledWith("thread", "turn");
});

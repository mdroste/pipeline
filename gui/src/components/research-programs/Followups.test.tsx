import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Followups from "./Followups";
const mocks = vi.hoisted(() => ({ queue: vi.fn(), context: vi.fn() }));
vi.mock("../../lib/programClient", () => ({
  programClient: { queue: mocks.queue },
}));
vi.mock("../../lib/deskClient", async (original) => ({
  ...(await original<typeof import("../../lib/deskClient")>()),
  deskClient: { context: mocks.context },
}));
afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  mocks.queue.mockResolvedValue([]);
  mocks.context.mockResolvedValue({ revision: 1, items: [] });
});
const props = {
  sessionId: "session",
  active: true,
  model: "model",
  effort: "high",
  onError: vi.fn(),
  onDispatch: vi.fn(),
  onRefresh: vi.fn().mockResolvedValue(undefined),
  onBranch: vi.fn().mockResolvedValue(undefined),
};
it("keeps an editable next draft during work and queues without sending it", async () => {
  const view = render(<Followups {...props} />);
  const input = screen.getByRole("textbox", { name: "Next message draft" });
  expect(input).not.toBeDisabled();
  fireEvent.change(input, {
    target: { value: "Test the alternative mechanism" },
  });
  expect(localStorage.getItem("pipeline.nextFollowup.session")).toBe(
    "Test the alternative mechanism",
  );
  view.unmount();
  render(<Followups {...props} />);
  expect(
    screen.getByRole("textbox", { name: "Next message draft" }),
  ).toHaveValue("Test the alternative mechanism");
  fireEvent.click(screen.getByRole("button", { name: "Queue next message" }));
  await waitFor(() =>
    expect(mocks.queue).toHaveBeenCalledWith(
      "session",
      expect.objectContaining({
        action: "enqueue",
        text: "Test the alternative mechanism",
        model: "model",
        effort: "high",
      }),
    ),
  );
  expect(mocks.queue.mock.calls.some((c) => c[1].action === "run")).toBe(false);
});
it("never auto-dispatches persisted work and disables Run while a turn is active", async () => {
  mocks.queue.mockResolvedValue([
    {
      id: "q",
      position: 1,
      revision: 1,
      request: {
        text: "Queued question",
        context: { items: [] },
        turnBudget: 1,
      },
      state: "queued",
      fingerprint: "exact",
      result: null,
    },
  ]);
  render(<Followups {...props} />);
  expect(
    await screen.findByRole("button", { name: "Run next" }),
  ).toBeDisabled();
  expect(mocks.queue.mock.calls.every((c) => c[1].action === "list")).toBe(
    true,
  );
});
it("uses the reviewed fingerprint once despite a double click", async () => {
  let resolve: () => void = () => undefined;
  const pending = new Promise<void>((r) => {
    resolve = r;
  });
  mocks.queue.mockImplementation(async (_id, a) => {
    if (a.action === "run") {
      await pending;
      return [];
    }
    return [
      {
        id: "q",
        position: 1,
        revision: 1,
        request: {
          text: "Queued question",
          context: { items: [] },
          turnBudget: 1,
        },
        state: "queued",
        fingerprint: "exact",
        result: null,
      },
    ];
  });
  render(<Followups {...props} active={false} />);
  const run = await screen.findByRole("button", { name: "Run next" });
  fireEvent.click(run);
  fireEvent.click(run);
  expect(
    mocks.queue.mock.calls.filter((c) => c[1].action === "run"),
  ).toHaveLength(1);
  expect(mocks.queue).toHaveBeenCalledWith("session", {
    action: "run",
    id: "q",
    fingerprint: "exact",
  });
  resolve();
  await waitFor(() => expect(props.onRefresh).toHaveBeenCalled());
});

it("pages terminal history while keeping the queued request accessible", async () => {
  const pending = {
    id: "pending",
    position: 200,
    revision: 1,
    state: "queued",
    fingerprint: "f",
    result: null,
    request: { text: "Still pending", context: { items: [] }, turnBudget: 1 },
  };
  mocks.queue.mockImplementation(async (_session, action) => [
    pending,
    ...Array.from({ length: action.historyOffset ? 1 : 100 }, (_, i) => ({
      ...pending,
      id: `old-${i}`,
      state: "cancelled",
      request: {
        ...pending.request,
        text: action.historyOffset ? "Oldest message" : `History ${i}`,
      },
    })),
  ]);
  render(<Followups {...props} active={false} />);
  expect(await screen.findByText("Still pending")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Older history" }));
  await screen.findByText("Oldest message");
  expect(mocks.queue).toHaveBeenCalledWith("session", {
    action: "list",
    historyOffset: 100,
  });
  expect(screen.getByRole("button", { name: "Run next" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Older history" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Newer history" }));
  await screen.findByText("History 99");
});

it("reviews and refreshes a queued message without sending, then uses its new fingerprint", async () => {
  let refreshed = false;
  const row = () => ({
    id: "q",
    position: 1,
    revision: refreshed ? 2 : 1,
    state: "queued",
    fingerprint: refreshed ? "fresh" : "old",
    result: null,
    request: {
      text: "Keep my question",
      model: "saved-model",
      effort: "high",
      binding: { runtimeRoot: "/project" },
      context: { items: [] },
      turnBudget: 1,
    },
  });
  mocks.queue.mockImplementation(async (_session, action) => {
    if (action.action === "reviewContext")
      return {
        id: "q",
        revision: 1,
        request: row().request,
        previousContext: { items: [] },
        previousRoot: "/project",
        conversationTitle: "Research",
        workspaceName: "Paper",
        conversationChanged: true,
        settingsChanged: false,
        settings: {
          preset: "Research",
          permissions: "read-only",
          commandNetwork: false,
          modules: [],
          instructions: "",
        },
        fingerprint: "review-token",
      };
    if (action.action === "refreshContext") refreshed = true;
    return [row()];
  });
  render(<Followups {...props} active={false} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Review current context" }),
  );
  await screen.findByRole("region", { name: "Review follow-up context" });
  expect(screen.getByText(/Model: saved-model/)).toBeInTheDocument();
  expect(
    screen.getByText(/conversation has newer messages/),
  ).toBeInTheDocument();
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Use reviewed context" }),
    ).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "Use reviewed context" }));
  await waitFor(() =>
    expect(mocks.queue).toHaveBeenCalledWith("session", {
      action: "refreshContext",
      id: "q",
      revision: 1,
      fingerprint: "review-token",
    }),
  );
  await waitFor(() =>
    expect(
      screen.queryByRole("region", { name: "Review follow-up context" }),
    ).toBeNull(),
  );
  expect(mocks.queue.mock.calls.some((c) => c[1].action === "run")).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Run next" }));
  await waitFor(() =>
    expect(mocks.queue).toHaveBeenCalledWith("session", {
      action: "run",
      id: "q",
      fingerprint: "fresh",
    }),
  );
});

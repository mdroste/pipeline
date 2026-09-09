import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import TasksPage from "./TasksPage";
import { template, type TaskRun, type Receipt } from "../lib/taskClient";
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (name: string, callback: (event: { payload: unknown }) => void) => {
      mocks.listeners.set(name, callback);
      return () => mocks.listeners.delete(name);
    },
  ),
}));
const run: TaskRun = {
  id: "test-run",
  revision: 0,
  name: "Workspace follow-up",
  state: "draft",
  reason: null,
  createdAt: 10,
  updatedAt: 10,
  dueAt: 100,
  deadlineAt: 600,
  sessionId: "session",
  scheduleId: null,
  scope: {
    sessionId: "session",
    workspaceId: null,
    runtimeRoot: "/research/task",
    profiles: {},
    checks: {},
  },
  chain: template("prompt", "Continue the idea", ""),
  inputs: {},
  progress: { actions: 0, receipts: {}, outputs: {}, limitReached: false },
};
beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.listeners.clear();
  localStorage.clear();
  mocks.invoke.mockImplementation(
    async (command: string, args: Record<string, unknown> = {}) => {
      switch (command) {
        case "task_list":
        case "task_schedules":
        case "task_saved_chains":
        case "task_events":
          return [];
        case "task_background":
          return false;
        case "list_profiles":
          return [{ id: "auto", name: "Automatic Paper Review" }];
        case "task_sessions":
          return [
            { id: "session", title: "Research idea", workspaceName: null },
          ];
        case "task_preview_times":
          return [];
        case "task_validate_chain":
          return JSON.parse(String(args.json));
        case "task_prepare":
        case "task_get":
          return run;
        case "task_control":
          return { ...run, state: "queued", revision: 1 };
        default:
          throw new Error(`Unexpected command ${command}`);
      }
    },
  );
});
it("prepares a concrete scope before starting work", async () => {
  render(<TasksPage initialSessionId="session" />);
  await screen.findByRole("option", { name: "Research idea" });
  fireEvent.click(screen.getByRole("button", { name: "Follow up" }));
  fireEvent.change(screen.getByLabelText("What should happen next?"), {
    target: { value: "Continue the idea" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Prepare automation →" }));
  await screen.findByRole("button", { name: "Start automation" });
  expect(
    mocks.invoke.mock.calls.some(([command]) => command === "task_control"),
  ).toBe(false);
  expect(screen.getByText("/research/task")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Start automation" }));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("task_control", {
      id: "test-run",
      revision: 0,
      action: "start",
    }),
  );
});
it("surfaces preparation failures without launching or clearing the request", async () => {
  render(<TasksPage initialSessionId="session" />);
  await screen.findByRole("option", { name: "Research idea" });
  fireEvent.change(screen.getByLabelText("Idea or instructions"), {
    target: { value: "An idea" },
  });
  const implementation = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation((command, ...args) =>
    command === "task_prepare"
      ? Promise.reject("Conversation changed")
      : implementation(command, ...args),
  );
  fireEvent.click(screen.getByRole("button", { name: "Prepare automation →" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Conversation changed",
  );
  expect(screen.getByLabelText("Idea or instructions")).toHaveValue("An idea");
  expect(
    mocks.invoke.mock.calls.some(([command]) => command === "task_control"),
  ).toBe(false);
});
it("keeps recurring schedules separate from active occurrences", async () => {
  render(<TasksPage />);
  await screen.findByText("Give your work a next step.");
  fireEvent.click(screen.getByRole("tab", { name: "Scheduled" }));
  await screen.findByText("Make time for recurring work.");
  expect(screen.getByRole("tab", { name: "Scheduled" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(mocks.invoke).toHaveBeenCalledWith("task_schedules");
});
it("handles task errors emitted by the native coordinator", async () => {
  render(<TasksPage />);
  await screen.findByText("Give your work a next step.");
  act(() =>
    mocks.listeners.get("tasks:error")?.({
      payload: "Another instance owns task execution",
    }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Another instance owns task execution",
  );
});

it("inspects the retained operation for a prior attempt after retry", async () => {
  const original: Receipt = {
    sequence: 1,
    address: "followup",
    stepId: "followup",
    label: "Continue the idea",
    state: "unknown",
    operation: "original-operation",
    startedAt: 10,
    finishedAt: null,
    wakeAt: null,
    output: null,
    error: "Interrupted action",
    child: { turnId: "original-turn" },
  };
  const replacement: Receipt = {
    ...original,
    sequence: 2,
    state: "completed",
    operation: "replacement-operation",
    error: null,
    output: { text: "Replacement preview" },
  };
  const completed: TaskRun = {
    ...run,
    state: "finished",
    revision: 4,
    progress: {
      ...run.progress,
      actions: 2,
      receipts: { followup: replacement },
    },
  };
  const implementation = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation((command, args) => {
    if (command === "task_get") return Promise.resolve(completed);
    if (command === "task_events")
      return Promise.resolve([
        {
          sequence: 3,
          at: 20,
          kind: "retry",
          detail: "Retry requested; previous attempts retained",
          attempts: [original],
        },
      ]);
    if (command === "task_step_output")
      return Promise.resolve({
        text:
          args.operation === "original-operation"
            ? "Full original response"
            : "Full replacement response",
      });
    return implementation(command, args);
  });
  render(<TasksPage initialTaskId={run.id} />);
  const prior = await screen.findByText(
    "Prior attempt 1: Continue the idea · Unknown",
  );
  const priorDetails = prior.closest("details")!;
  fireEvent.click(prior);
  const output = within(priorDetails).getByText("Output").closest("details")!;
  output.open = true;
  fireEvent(output, new Event("toggle"));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("task_step_output", {
      id: run.id,
      address: "followup",
      operation: "original-operation",
    }),
  );
  expect(
    await within(priorDetails).findByText(/Full original response/),
  ).toBeInTheDocument();
  expect(screen.getByText(/Replacement preview/)).toBeInTheDocument();
  expect(
    mocks.invoke.mock.calls.some(([command]) => command === "task_control"),
  ).toBe(false);
});

it("roves tabs without loading until activation and associates every panel", async () => {
  const user = (await import("@testing-library/user-event")).default.setup();
  render(<TasksPage />);
  await screen.findByText("Give your work a next step.");
  const active = screen.getByRole("tab", { name: "Active" });
  const scheduled = screen.getByRole("tab", { name: "Scheduled" });
  const calls = mocks.invoke.mock.calls.length;
  act(() => active.focus());
  await user.keyboard("{ArrowRight}");
  expect(scheduled).toHaveFocus();
  expect(active).toHaveAttribute("aria-selected", "true");
  expect(
    screen.getAllByRole("tab").filter((tab) => tab.tabIndex === 0),
  ).toEqual([scheduled]);
  expect(mocks.invoke.mock.calls).toHaveLength(calls);
  await user.keyboard("{Enter}");
  await screen.findByText("Make time for recurring work.");
  expect(screen.getByRole("tabpanel", { name: "Scheduled" }).id).toBe(
    scheduled.getAttribute("aria-controls"),
  );
  await user.keyboard("{End}");
  expect(screen.getByRole("tab", { name: "Research" })).toHaveFocus();
  await user.keyboard("{Home}");
  expect(active).toHaveFocus();
  for (const tab of screen.getAllByRole("tab"))
    expect(
      document.getElementById(tab.getAttribute("aria-controls")!),
    ).toHaveAttribute("aria-labelledby", tab.id);
});

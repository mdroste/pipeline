import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useState } from "react";
import Builder from "./Builder";
import Detail from "./Detail";
import PaperInput from "./PaperInput";
import { Timing } from "./Timing";
import { template } from "../../lib/taskTemplates";
import { taskClient } from "../../lib/taskClient";
import { router } from "../../lib/router";
import type { TaskRun, Trigger } from "../../lib/taskTypes";
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
const savedChain = template("prompt", "Saved instructions", "auto");
const run: TaskRun = {
  id: "run",
  name: "My automation",
  revision: 8,
  state: "draft",
  reason: null,
  createdAt: 1,
  updatedAt: 2,
  dueAt: 1794420000,
  deadlineAt: 9999999999,
  sessionId: "chat-id",
  scheduleId: null,
  chain: savedChain,
  scope: {
    sessionId: "chat-id",
    conversationTitle: "My paper",
    projectName: "Economics",
    workspaceId: "project-id",
    runtimeRoot: "C:/research",
    profiles: {},
    checks: {},
  },
  inputs: {
    __trigger: {
      kind: "calendar",
      timezone: "America/New_York",
      hour: 9,
      minute: 0,
      weekdays: [0],
    },
  },
  progress: { actions: 0, receipts: {}, outputs: {}, limitReached: false },
};
beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.open.mockReset();
  mocks.invoke.mockImplementation(async (command, args) => {
    if (command === "list_profiles")
      return [{ id: "auto", name: "Automatic Paper Review" }];
    if (command === "task_sessions")
      return [{ id: "chat-id", title: "My paper", workspaceName: "Economics" }];
    if (command === "task_saved_chains")
      return [{ id: "saved", chain: savedChain }];
    if (command === "task_validate_chain") return JSON.parse(args.json);
    if (command === "task_save_chain") return "saved";
    if (command === "task_preview_times" || command === "task_events")
      return [];
    if (command === "task_control")
      return { ...run, revision: 9, state: "queued" };
    if (command === "task_validate_input_file")
      return {
        root: "C:/research",
        path: "papers/draft.pdf",
        name: "draft.pdf",
      };
    if (command === "task_input_files")
      return {
        directory: "",
        entries: [
          { path: "papers/draft.pdf", name: "draft.pdf", kind: "file" },
        ],
        truncated: false,
      };
    throw new Error(`Unexpected ${command}`);
  });
});
it("restores edited structures, names, quick fields, and timing per template", async () => {
  render(
    <Builder
      initialSessionId="chat-id"
      onPrepared={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  await screen.findByRole("option", { name: "Automatic Paper Review" });
  fireEvent.change(screen.getByLabelText("Idea or instructions"), {
    target: { value: "My idea" },
  });
  fireEvent.change(screen.getByLabelText("Automation name"), {
    target: { value: "My revised paper" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Weekly" }));
  const limit = screen.getByLabelText("Action limit");
  fireEvent.change(limit, { target: { value: "80" } });
  fireEvent.blur(limit);
  fireEvent.click(screen.getByRole("button", { name: "Follow up" }));
  fireEvent.change(screen.getByLabelText("What should happen next?"), {
    target: { value: "My follow-up" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Review and revise" }));
  expect(screen.getByLabelText("Automation name")).toHaveValue(
    "My revised paper",
  );
  expect(screen.getByLabelText("Action limit")).toHaveValue(80);
  expect(screen.getByLabelText("Start rule")).toHaveValue("calendar");
  expect(screen.getByLabelText("Automation definition").textContent).toContain(
    "My idea",
  );
  expect(screen.queryByLabelText("Idea or instructions")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Follow up" }));
  expect(screen.getByLabelText("What should happen next?")).toHaveValue(
    "My follow-up",
  );
});
it("keeps My templates and naming outside Advanced and preserves saved-template edits", async () => {
  render(
    <Builder
      initialSessionId="chat-id"
      onPrepared={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  await screen.findByRole("option", { name: savedChain.name });
  const picker = screen.getByLabelText("My templates");
  expect(picker.closest("details")).toBeNull();
  fireEvent.change(picker, { target: { value: "saved" } });
  fireEvent.change(screen.getByLabelText("Automation name"), {
    target: { value: "My saved version" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Start empty" }));
  fireEvent.change(picker, { target: { value: "saved" } });
  expect(screen.getByLabelText("Automation name")).toHaveValue(
    "My saved version",
  );
  fireEvent.click(screen.getByRole("button", { name: "Save as template" }));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("task_save_chain", {
      chain: expect.objectContaining({ name: "My saved version" }),
    }),
  );
});
it("shows scope and local readiness without silently enabling background mode", async () => {
  const setup = vi.fn();
  render(
    <Detail
      run={run}
      background={false}
      onBackgroundSetup={setup}
      onUpdated={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  expect(screen.getByText("My paper")).toBeVisible();
  expect(screen.getByText("Economics")).toBeVisible();
  const readiness = screen.getByRole("region", {
    name: "Where and when this runs",
  });
  expect(readiness).toHaveTextContent("Runs on this computer");
  expect(readiness).toHaveTextContent("Background mode: Off");
  expect(readiness).toHaveTextContent("America / New York");
  fireEvent.click(
    screen.getByRole("button", { name: "Set up background running" }),
  );
  expect(setup).toHaveBeenCalledOnce();
  expect(
    mocks.invoke.mock.calls.some(([name]) => name === "task_background"),
  ).toBe(false);
});
it("previews the exact unfinished actions before retry and preserves the revision", async () => {
  const receipt = {
    address: "action",
    stepId: "action",
    label: "Revise paper",
    state: "unknown",
    operation: "op",
    startedAt: 1,
    finishedAt: null,
    wakeAt: null,
    error: null,
    output: null,
  };
  render(
    <Detail
      run={{
        ...run,
        state: "attention",
        progress: { ...run.progress, receipts: { action: receipt } },
      }}
      onUpdated={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByText("More recovery options"));
  fireEvent.click(screen.getByRole("button", { name: "Review retry" }));
  expect(
    mocks.invoke.mock.calls.some(([name]) => name === "task_control"),
  ).toBe(false);
  expect(screen.getByLabelText("Retry preview")).toHaveTextContent(
    "Revise paper",
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Retry unfinished actions" }),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("task_control", {
      id: "run",
      revision: 8,
      action: "retry",
    }),
  );
});
it("opens a review result from its readable label", async () => {
  const navigate = vi.spyOn(router, "navigate").mockResolvedValue(true);
  const receipt = {
    address: "review",
    stepId: "review",
    label: "Review revised paper",
    state: "completed",
    operation: "op",
    startedAt: 1,
    finishedAt: 2,
    wakeAt: null,
    error: null,
    output: { runId: "review-id", complete: true, highPriorityCount: 2 },
  };
  render(
    <Detail
      run={{
        ...run,
        state: "finished",
        progress: { ...run.progress, receipts: { review: receipt } },
      }}
      onUpdated={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Open review · Review revised paper" }),
  );
  expect(navigate).toHaveBeenCalledWith({
    page: "history",
    runId: "review-id",
  });
  navigate.mockRestore();
});
it("selects a project paper and validates a native Browse choice inline", async () => {
  const validity = vi.fn();
  function Harness() {
    const [value, setValue] = useState("");
    return (
      <PaperInput
        sessionId="chat-id"
        value={value}
        onChange={setValue}
        onValidChange={validity}
      />
    );
  }
  render(<Harness />);
  fireEvent.click(
    screen.getByRole("button", { name: "Choose from project files" }),
  );
  fireEvent.click(await screen.findByRole("button", { name: "draft.pdf" }));
  await screen.findByText("Revisions will be saved as Markdown.");
  expect(validity).toHaveBeenLastCalledWith(true);
  mocks.open.mockResolvedValue("C:/outside.pdf");
  const validate = vi
    .spyOn(taskClient, "validateInputFile")
    .mockRejectedValue("Choose a file inside this conversation’s folder.");
  fireEvent.click(screen.getByRole("button", { name: "Browse…" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "inside this conversation",
  );
  expect(validity).toHaveBeenLastCalledWith(false);
  validate.mockRestore();
});
it("ignores stale file validation after switching conversation", async () => {
  let resolve!: (value: { root: string; path: string; name: string }) => void;
  const validate = vi
    .spyOn(taskClient, "validateInputFile")
    .mockImplementationOnce(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    )
    .mockRejectedValueOnce("Missing in this conversation");
  const valid = vi.fn();
  const { rerender } = render(
    <PaperInput
      sessionId="one"
      value="draft.pdf"
      onChange={vi.fn()}
      onValidChange={valid}
    />,
  );
  rerender(
    <PaperInput
      sessionId="two"
      value="draft.pdf"
      onChange={vi.fn()}
      onValidChange={valid}
    />,
  );
  await screen.findByRole("alert");
  await act(async () =>
    resolve({ root: "old", path: "draft.pdf", name: "draft.pdf" }),
  );
  expect(valid).toHaveBeenLastCalledWith(false);
  expect(screen.queryByText("old")).toBeNull();
  validate.mockRestore();
});

it("marks a project-file selection as an unsaved builder edit", async () => {
  const dirty = vi.fn();
  render(
    <Builder
      initialSessionId="chat-id"
      onDirtyChange={dirty}
      onPrepared={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  await screen.findByRole("option", { name: "Automatic Paper Review" });
  fireEvent.click(screen.getByText("Start from an existing paper"));
  fireEvent.click(
    screen.getByRole("button", { name: "Choose from project files" }),
  );
  fireEvent.click(await screen.findByRole("button", { name: "draft.pdf" }));
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Preview automation →" }),
    ).toBeEnabled(),
  );
  expect(dirty).toHaveBeenLastCalledWith(true);
});

it("explains the context replacement before applying it", async () => {
  render(
    <Detail
      run={{ ...run, state: "attention" }}
      onUpdated={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByText("More recovery options"));
  fireEvent.click(
    screen.getByRole("button", { name: "Review conversation update" }),
  );
  expect(
    screen.getByLabelText("Conversation update preview"),
  ).toHaveTextContent(
    "connection, folder, tools, permissions, and check definitions",
  );
  expect(
    mocks.invoke.mock.calls.some(([name]) => name === "task_control"),
  ).toBe(false);
  fireEvent.click(
    screen.getByRole("button", { name: "Use current conversation context" }),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("task_control", {
      id: "run",
      revision: 8,
      action: "refreshContext",
    }),
  );
});
it("applies calendar presets and converts interval units", async () => {
  const changed = vi.fn();
  function Harness() {
    const [value, setValue] = useState<Trigger>({ kind: "now" });
    return (
      <Timing
        value={value}
        onChange={(v) => {
          changed(v);
          setValue(v);
        }}
      />
    );
  }
  render(<Harness />);
  fireEvent.click(screen.getByRole("button", { name: "Weekdays" }));
  expect(changed).toHaveBeenLastCalledWith(
    expect.objectContaining({ kind: "calendar", weekdays: [0, 1, 2, 3, 4] }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Daily" }));
  expect(changed).toHaveBeenLastCalledWith(
    expect.objectContaining({ weekdays: [0, 1, 2, 3, 4, 5, 6] }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Hourly" }));
  expect(screen.getByLabelText("Interval amount")).toHaveValue(1);
  fireEvent.change(screen.getByLabelText("Interval unit"), {
    target: { value: "86400" },
  });
  expect(changed).toHaveBeenLastCalledWith(
    expect.objectContaining({ kind: "interval", seconds: 86400 }),
  );
});

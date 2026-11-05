import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ScheduleCard from "./ScheduleCard";
import { type Schedule } from "../../lib/taskTypes";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), confirm: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("../DialogService", () => ({ confirmDialog: mocks.confirm }));

const schedule: Schedule = {
  id: "weekly",
  revision: 4,
  name: "Weekly review",
  enabled: true,
  chain: {
    schemaVersion: 1,
    name: "Weekly review",
    description: "",
    limits: { maxActions: 64, deadlineHours: 168, actionTimeoutSecs: 7200 },
    steps: [],
  },
  scope: {
    sessionId: "conversation",
    workspaceId: "project",
    runtimeRoot: "/project",
    profiles: {},
    checks: {},
  },
  trigger: { kind: "interval", seconds: 3600, anchor: 0 },
  nextDueAt: 1000,
};

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.invoke.mockResolvedValue(schedule);
  mocks.confirm.mockReset();
});

it("rebinds a schedule to its conversation without changing its timing", async () => {
  const updated = vi.fn();
  render(
    <ScheduleCard schedule={schedule} onUpdated={updated} onError={vi.fn()} />,
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Use current conversation settings" }),
  );
  await waitFor(() => expect(updated).toHaveBeenCalledOnce());
  expect(mocks.invoke).toHaveBeenCalledWith("task_update_schedule", {
    id: "weekly",
    revision: 4,
    enabled: true,
    trigger: null,
    refresh: true,
  });
});

it("shows why a refresh was refused", async () => {
  const failed = vi.fn();
  mocks.invoke.mockRejectedValue("Prepare a new schedule.");
  render(
    <ScheduleCard schedule={schedule} onUpdated={vi.fn()} onError={failed} />,
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Use current conversation settings" }),
  );
  await waitFor(() =>
    expect(failed).toHaveBeenCalledWith("Prepare a new schedule."),
  );
});

it("offers no refresh for a schedule without a conversation", () => {
  render(
    <ScheduleCard
      schedule={{ ...schedule, scope: { ...schedule.scope, sessionId: null } }}
      onUpdated={vi.fn()}
      onError={vi.fn()}
    />,
  );
  expect(
    screen.queryByRole("button", { name: "Use current conversation settings" }),
  ).not.toBeInTheDocument();
});

it("shows what holds a schedule back instead of a past run time", () => {
  const open = vi.fn();
  render(
    <ScheduleCard
      schedule={{
        ...schedule,
        blockingRun: { id: "previous", state: "attention" },
      }}
      onUpdated={vi.fn()}
      onError={vi.fn()}
      onOpenRun={open}
    />,
  );
  expect(
    screen.getByText("Waiting for the previous run · Needs attention"),
  ).toBeInTheDocument();
  expect(screen.queryByText(/^Next run/)).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Open previous run" }));
  expect(open).toHaveBeenCalledWith("previous");
});

it("keeps the next run time while the current run is on schedule", () => {
  render(
    <ScheduleCard
      schedule={{
        ...schedule,
        nextDueAt: Math.floor(Date.now() / 1000) + 3600,
        blockingRun: { id: "current", state: "running" },
      }}
      onUpdated={vi.fn()}
      onError={vi.fn()}
      onOpenRun={vi.fn()}
    />,
  );
  expect(screen.getByText(/^Next run/)).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Open previous run" }),
  ).not.toBeInTheDocument();
});

it("deletes a schedule only after confirmation", async () => {
  const updated = vi.fn();
  mocks.confirm.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
  render(
    <ScheduleCard schedule={schedule} onUpdated={updated} onError={vi.fn()} />,
  );
  const remove = screen.getByRole("button", { name: "Delete schedule" });
  fireEvent.click(remove);
  await waitFor(() => expect(mocks.confirm).toHaveBeenCalledOnce());
  expect(mocks.invoke).not.toHaveBeenCalled();
  fireEvent.click(remove);
  await waitFor(() => expect(updated).toHaveBeenCalledOnce());
  expect(mocks.invoke).toHaveBeenCalledWith("task_delete_schedule", {
    id: "weekly",
    revision: 4,
  });
});

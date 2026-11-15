import { act, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Activity from "./Activity";
import {
  newDiscovery,
  type DiscoveryActivity,
  type DiscoveryRun,
} from "../../lib/discoveryClient";

const mocks = vi.hoisted(() => ({ activity: vi.fn(), get: vi.fn() }));
vi.mock("../../lib/discoveryClient", async (original) => ({
  ...(await original<typeof import("../../lib/discoveryClient")>()),
  discoveryClient: { activity: mocks.activity, get: mocks.get },
}));
const run = {
  id: "run",
  revision: 1,
  state: "running",
  activeChild: "step",
  definition: { ...newDiscovery(), ideaAgents: 2 },
} as DiscoveryRun;
const snapshot: DiscoveryActivity = {
  revision: 1,
  observedAt: 1000,
  lastActivityAt: 995,
  step: {
    id: "step",
    label: "Generate ideas",
    state: "running",
    startedAt: 900,
    finishedAt: null,
  },
  entries: [
    { id: "a-start", at: 995, label: "Idea agent 1", state: "running" },
  ],
  agents: [
    { id: "a", label: "Idea agent 1", state: "running", updatedAt: 995 },
    { id: "b", label: "Idea agent 2", state: "queued", updatedAt: 990 },
  ],
};
beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date(1000 * 1000));
  mocks.activity.mockReset().mockResolvedValue(snapshot);
  mocks.get.mockReset();
});
afterEach(() => vi.useRealTimers());
const settle = async () => {
  await act(async () => {
    await Promise.resolve();
  });
};

it("ticks elapsed and last activity time between snapshots and reports partial agent completion", async () => {
  const view = render(<Activity run={run} onUpdated={vi.fn()} />);
  await settle();
  expect(screen.getByText("5s ago")).toBeInTheDocument();
  expect(screen.getByText("1m 40s")).toBeInTheDocument();
  expect(
    within(screen.getByRole("list", { name: "Agent status" })).getByText(
      "Waiting to start",
    ),
  ).toBeInTheDocument();
  mocks.activity.mockResolvedValue({
    ...snapshot,
    observedAt: 1002,
    agents: [
      { ...snapshot.agents[0], state: "completed" },
      { ...snapshot.agents[1], state: "running" },
    ],
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1000);
  });
  expect(screen.getByText("6s ago")).toBeInTheDocument();
  expect(screen.getByText("1m 41s")).toBeInTheDocument();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1000);
  });
  expect(
    screen.getByText("Idea agents · 1 of 2 completed"),
  ).toBeInTheDocument();
  view.unmount();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(10000);
  });
  expect(mocks.activity).toHaveBeenCalledTimes(2);
});

it("freezes a finished step's duration and refreshes a newer run revision", async () => {
  const updated = vi.fn();
  mocks.activity.mockResolvedValue({
    ...snapshot,
    revision: 2,
    step: { ...snapshot.step, finishedAt: 950, state: "failed" },
  });
  mocks.get.mockResolvedValue({ ...run, revision: 2, state: "attention" });
  render(
    <Activity
      run={{ ...run, state: "attention", activeChild: null }}
      onUpdated={updated}
    />,
  );
  await settle();
  expect(screen.getByText("50s")).toBeInTheDocument();
  expect(updated).toHaveBeenCalledWith(
    expect.objectContaining({ revision: 2 }),
  );
  await act(async () => {
    await vi.advanceTimersByTimeAsync(5000);
  });
  expect(screen.getByText("50s")).toBeInTheDocument();
  expect(mocks.activity).toHaveBeenCalledTimes(1);
});

it("keeps requests sequential and ignores a late response after unmount", async () => {
  let resolve!: (value: DiscoveryActivity) => void;
  mocks.activity.mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  const updated = vi.fn();
  const view = render(<Activity run={run} onUpdated={updated} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(6000);
  });
  expect(mocks.activity).toHaveBeenCalledTimes(1);
  view.unmount();
  await act(async () => resolve({ ...snapshot, revision: 2 }));
  expect(mocks.get).not.toHaveBeenCalled();
  expect(updated).not.toHaveBeenCalled();
});

it("shows update failures and recovers on the next poll", async () => {
  mocks.activity.mockRejectedValueOnce("Connection unavailable");
  render(<Activity run={run} onUpdated={vi.fn()} />);
  await settle();
  expect(screen.getByRole("status")).toHaveTextContent(
    "Connection unavailable",
  );
  await act(async () => {
    await vi.advanceTimersByTimeAsync(2000);
  });
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  expect(screen.getByText("Activity feed")).toBeInTheDocument();
});

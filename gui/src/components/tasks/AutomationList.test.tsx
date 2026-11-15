import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import TasksPage from "../TasksPage";
import { newMission, type Mission } from "../../lib/missionClient";
import { newDiscovery, type DiscoveryRun } from "../../lib/discoveryClient";
import type { AutomationSummary } from "../../lib/automationClient";
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listeners: new Map<string, () => void>(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event, callback) => {
    mocks.listeners.set(event, callback);
    return () => mocks.listeners.delete(event);
  }),
}));
vi.mock("../ReportViewer", () => ({ default: () => null }));
const scope = {
  sessionId: "source",
  workspaceId: "project",
  runtimeRoot: "/research",
  profiles: {},
  checks: {},
};
const investigation: Mission = {
  id: "shared-id",
  revision: 1,
  definition: { ...newMission(), name: "Mechanism investigation" },
  state: "running",
  reason: "Checking assumptions",
  sourceSessionId: "source",
  workspaceId: "project",
  scope,
  plannerScope: scope,
  challengerScope: scope,
  capabilities: [],
  goals: [],
  rounds: [],
  questions: [],
  methods: [],
  selectedMethods: [],
  activeChild: null,
  childIds: [],
  phase: "plan",
  actionsReserved: 0,
  activeSeconds: 0,
  stagnantRounds: 0,
  createdAt: 1,
  updatedAt: 3,
  deadlineAt: null,
  dueAt: null,
  brief: "",
  attentionCount: 0,
  changes: [],
};
const discovery: DiscoveryRun = {
  id: "shared-id",
  revision: 2,
  definition: { ...newDiscovery(), prompt: "Fiscal policy proposals" },
  state: "running",
  reason: "Comparing questions",
  phase: { kind: "orient" },
  phaseLabel: "Planning proposals",
  candidateCount: 0,
  orientation: null,
  selection: null,
  selectionHash: null,
  selected: [],
  ranking: [],
  actionsReserved: 0,
  activeSeconds: 0,
  deadlineAt: 9999999999,
  activeChild: null,
  workspaceId: "project",
  sourceSessionId: "source",
  papers: [],
};
const rows: AutomationSummary[] = [
  {
    id: "shared-id",
    kind: "research",
    name: investigation.definition.name,
    state: "running",
    reason: "Checking assumptions",
    updatedAt: 3,
    dueAt: null,
  },
  {
    id: "shared-id",
    kind: "discovery",
    name: discovery.definition.prompt,
    state: "running",
    reason: "Comparing questions",
    updatedAt: 2,
    dueAt: null,
  },
  {
    id: "task-one",
    kind: "task",
    name: "Follow up on results",
    state: "queued",
    reason: null,
    updatedAt: 1,
    dueAt: null,
  },
];
beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.listeners.clear();
  mocks.invoke.mockImplementation(async (command, args) => {
    if (command === "task_storage_issues") return [];
    if (command === "automation_list")
      return args.view === "history"
        ? rows.map((r) => ({
            ...r,
            state: r.kind === "task" ? "finished" : "completed",
          }))
        : rows;
    if (command === "task_background") return false;
    if (command === "mission_get") return investigation;
    if (command === "discovery_get") return discovery;
    if (command === "discovery_control")
      return { ...discovery, state: "paused", revision: 3 };
    if (
      command === "task_schedules" ||
      command === "mission_events" ||
      command === "discovery_events"
    )
      return [];
    throw new Error(`Unexpected ${command}`);
  });
});

it("lists all automation kinds together and routes equal IDs to their own detail and control commands", async () => {
  const user = userEvent.setup();
  render(<TasksPage />);
  await screen.findByRole("button", { name: /Mechanism investigation/ });
  const views = screen.getByRole("tablist", { name: "Automations view" });
  expect(
    within(views)
      .getAllByRole("tab")
      .map((tab) => tab.textContent),
  ).toEqual(["Active", "Scheduled", "History"]);
  expect(
    screen.getByRole("button", { name: /Fiscal policy proposals/ }),
  ).toHaveTextContent("Discovery");
  expect(
    screen.getByRole("button", { name: /Follow up on results/ }),
  ).toHaveTextContent("Steps");
  await user.click(
    screen.getByRole("button", { name: /Mechanism investigation/ }),
  );
  expect(
    await screen.findByRole("region", { name: "Research automation details" }),
  ).toBeVisible();
  expect(mocks.invoke).toHaveBeenCalledWith("mission_get", { id: "shared-id" });
  await user.click(
    screen.getByRole("button", { name: /Fiscal policy proposals/ }),
  );
  const detail = await screen.findByRole("region", {
    name: "Research discovery progress",
  });
  expect(
    screen.queryByRole("region", { name: "Research automation details" }),
  ).toBeNull();
  await user.click(within(detail).getByRole("button", { name: "Pause" }));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("discovery_control", {
      id: "shared-id",
      revision: 2,
      action: "pause",
    }),
  );
  expect(
    mocks.invoke.mock.calls.some(
      ([command]) =>
        command === "task_control" || command === "mission_control",
    ),
  ).toBe(false);
});

it("includes research results in History and requests only schedules on Scheduled", async () => {
  const user = userEvent.setup();
  render(<TasksPage />);
  await screen.findByRole("button", { name: /Mechanism investigation/ });
  await user.click(screen.getByRole("tab", { name: "History" }));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("automation_list", {
      view: "history",
      offset: 0,
    }),
  );
  expect(
    screen.getByRole("button", { name: /Mechanism investigation/ }),
  ).toHaveTextContent("Criteria assessed as met");
  expect(
    screen.getByRole("button", { name: /Fiscal policy proposals/ }),
  ).toHaveTextContent("Completed");
  await user.click(screen.getByRole("tab", { name: "Scheduled" }));
  await screen.findByText("Make time for recurring work.");
  expect(mocks.invoke).toHaveBeenCalledWith("task_schedules");
  expect(mocks.invoke).not.toHaveBeenCalledWith(
    "automation_list",
    expect.objectContaining({ view: "scheduled" }),
  );
});

it("retains access to the previous page when the next page is empty", async () => {
  const handler = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation((command, args) =>
    command === "automation_list"
      ? Promise.resolve(
          args.offset
            ? []
            : Array.from({ length: 50 }, (_, i) => ({
                ...rows[0],
                id: `run-${i}`,
                name: `Investigation ${i}`,
              })),
        )
      : handler(command, args),
  );
  const user = userEvent.setup();
  render(<TasksPage />);
  await user.click(await screen.findByRole("button", { name: "Next" }));
  expect(await screen.findByText("No more automations.")).toBeVisible();
  expect(mocks.invoke).toHaveBeenCalledWith("automation_list", {
    view: "active",
    offset: 50,
  });
  await user.click(screen.getByRole("button", { name: "Previous" }));
  expect(await screen.findByText("Investigation 0")).toBeVisible();
});

it("opens discovery links without a Research tab and refreshes the detail on task events", async () => {
  render(<TasksPage initialDiscoveryId="shared-id" />);
  await screen.findByRole("region", { name: "Research discovery progress" });
  expect(mocks.invoke).toHaveBeenCalledWith("discovery_get", {
    id: "shared-id",
  });
  expect(screen.queryByRole("tab", { name: "Research" })).toBeNull();
  const handler = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation((command, args) =>
    command === "discovery_get"
      ? Promise.resolve({
          ...discovery,
          revision: 3,
          phaseLabel: "Drafting selected projects",
        })
      : handler(command, args),
  );
  act(() => mocks.listeners.get("tasks:changed")?.());
  expect(await screen.findByText("Drafting selected projects")).toBeVisible();
});

it("preserves research details when a background refresh fails", async () => {
  render(<TasksPage initialDiscoveryId="shared-id" />);
  await screen.findByRole("region", { name: "Research discovery progress" });
  const handler = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation((command, args) =>
    command === "discovery_get"
      ? Promise.reject(new Error("Could not read discovery"))
      : handler(command, args),
  );
  act(() => mocks.listeners.get("discovery:changed")?.());
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Could not read discovery",
  );
  expect(
    screen.getByRole("region", { name: "Research discovery progress" }),
  ).toBeVisible();
});

it("ignores an older detail response after selecting a different automation kind", async () => {
  let resolve!: (value: DiscoveryRun) => void;
  const handler = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation((command, args) =>
    command === "discovery_get"
      ? new Promise<DiscoveryRun>((done) => {
          resolve = done;
        })
      : handler(command, args),
  );
  render(<TasksPage />);
  fireEvent.click(
    await screen.findByRole("button", { name: /Fiscal policy proposals/ }),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("discovery_get", {
      id: "shared-id",
    }),
  );
  fireEvent.click(
    screen.getByRole("button", { name: /Mechanism investigation/ }),
  );
  await screen.findByRole("region", { name: "Research automation details" });
  await act(async () => resolve(discovery));
  expect(
    screen.getByRole("region", { name: "Research automation details" }),
  ).toBeVisible();
  expect(
    screen.queryByRole("region", { name: "Research discovery progress" }),
  ).toBeNull();
});

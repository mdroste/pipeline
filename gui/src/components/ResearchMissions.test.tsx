import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ResearchMissions from "./ResearchMissions";
import MissionDetail from "./research-missions/Detail";
import MissionBuilder from "./research-missions/Builder";
import { newMission, type Mission } from "../lib/missionClient";
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listeners: new Map<string, () => void>(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, callback: () => void) => {
    mocks.listeners.set(name, callback);
    return () => mocks.listeners.delete(name);
  }),
}));
vi.mock("./ReportViewer", () => ({
  default: ({ markdown }: { markdown: string }) => <pre>{markdown}</pre>,
}));
const scope = {
  sessionId: "investigator",
  workspaceId: "project",
  runtimeRoot: "/research/task-copy",
  profiles: {},
  checks: {},
};
const mission: Mission = {
  id: "mission1",
  revision: 1,
  definition: newMission(),
  state: "draft",
  reason: "Prepared research scope",
  sourceSessionId: "source",
  workspaceId: "project",
  scope,
  plannerScope: { ...scope, sessionId: "planner" },
  challengerScope: { ...scope, sessionId: "challenger" },
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
  createdAt: 100,
  updatedAt: 100,
  deadlineAt: null,
  dueAt: null,
  brief: "Retained research brief",
  attentionCount: 0,
  changes: [],
};
beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.listeners.clear();
  localStorage.clear();
  mocks.invoke.mockImplementation(async (name: string) => {
    switch (name) {
      case "mission_list":
      case "mission_methods":
      case "mission_events":
        return [];
      case "task_sessions":
        return [
          {
            id: "source",
            title: "Research question",
            workspaceName: "Project",
          },
        ];
      case "list_profiles":
        return [{ id: "review", name: "Paper review" }];
      case "mission_choices":
        return {
          workspaceId: "project",
          root: "/research/task-copy",
          canEdit: false,
          commandNetwork: false,
          canHostCompute: true,
          checks: [{ id: "check", name: "Baseline check" }],
          experiments: [
            { id: "grid", title: "Elasticity grid", kind: "experiment_plan" },
          ],
          monitors: [{ id: "monitor", title: "Accepted data" }],
        };
      case "mission_prepare":
      case "mission_get":
        return mission;
      case "mission_control":
        return { ...mission, state: "queued", revision: 2 };
      case "mission_answer":
        return mission;
      case "mission_evidence":
        return { text: "Exact retained evidence", truncated: false };
      case "mission_export":
        return null;
      default:
        throw new Error(`Unexpected command: ${name}`);
    }
  });
});
it("shows missions without starting a model or loading research configuration", async () => {
  render(<ResearchMissions />);
  await screen.findByText("Start autonomous research");
  expect(mocks.invoke.mock.calls.map(([name]) => name)).toEqual([
    "mission_list",
  ]);
});
it("prepares an inspect-only mission and shows its concrete scope before start", async () => {
  render(<ResearchMissions initialSessionId="source" />);
  fireEvent.click(
    await screen.findByRole("button", { name: "+ New research automation" }),
  );
  const prepare = await screen.findByRole("button", {
    name: "Prepare research automation →",
  });
  await waitFor(() => expect(prepare).toBeEnabled());
  expect(
    screen.getByLabelText("Allow edits in the working copy"),
  ).toBeDisabled();
  fireEvent.click(prepare);
  await screen.findByRole("button", { name: "Start research automation" });
  expect(screen.getByText("/research/task-copy")).toBeInTheDocument();
  expect(
    mocks.invoke.mock.calls.some(([name]) => name === "mission_control"),
  ).toBe(false);
  fireEvent.click(
    screen.getByRole("button", { name: "Start research automation" }),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("mission_control", {
      id: "mission1",
      revision: 1,
      action: "start",
    }),
  );
});
it("keeps a recoverable mission draft after preparation fails", async () => {
  const handler = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation((name, ...args) =>
    name === "mission_prepare"
      ? Promise.reject("Captured inputs changed")
      : handler(name, ...args),
  );
  render(
    <MissionBuilder
      initialSessionId="source"
      onPrepared={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Prepare research automation →" }),
    ).toBeEnabled(),
  );
  fireEvent.change(screen.getByLabelText("Automation name"), {
    target: { value: "Test the extension" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Prepare research automation →" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Captured inputs changed",
  );
  expect(screen.getByLabelText("Automation name")).toHaveValue(
    "Test the extension",
  );
  expect(
    JSON.parse(localStorage.getItem("pipeline.researchMission.draft.v1")!)
      .definition.name,
  ).toBe("Test the extension");
  expect(
    mocks.invoke.mock.calls.some(([name]) => name === "mission_control"),
  ).toBe(false);
});
it("passes selected computations and monitors as bounded explicit policy", async () => {
  render(
    <MissionBuilder
      initialSessionId="source"
      onPrepared={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Prepare research automation →" }),
    ).toBeEnabled(),
  );
  fireEvent.click(screen.getByText("Computation and project changes"));
  fireEvent.click(screen.getByLabelText("Baseline check"));
  fireEvent.click(screen.getByLabelText(/Elasticity grid/));
  fireEvent.click(screen.getByLabelText("Accepted data"));
  fireEvent.click(
    screen.getByRole("button", { name: "Prepare research automation →" }),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "mission_prepare",
      expect.objectContaining({
        request: expect.objectContaining({
          definition: expect.objectContaining({
            policy: expect.objectContaining({
              checkProfileIds: ["check"],
              experimentIds: ["grid"],
              monitorIds: ["monitor"],
              allowEdits: false,
            }),
          }),
        }),
      }),
    ),
  );
});
it("does not allow host computations through an isolated task conversation", async () => {
  const handler = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation(async (name, ...args) =>
    name === "mission_choices"
      ? {
          ...(await handler(name, ...args)),
          canEdit: true,
          canHostCompute: false,
        }
      : handler(name, ...args),
  );
  render(
    <MissionBuilder
      initialSessionId="source"
      onPrepared={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Prepare research automation →" }),
    ).toBeEnabled(),
  );
  fireEvent.click(screen.getByText("Computation and project changes"));
  expect(screen.getByLabelText("Baseline check")).toBeDisabled();
  expect(screen.getByLabelText(/Elasticity grid/)).toBeDisabled();
});
it("sends answers with a stable operation and leaves other decisions available", async () => {
  const updated = vi.fn();
  const questions = [
    {
      id: "q1",
      question: "Which sample should be used?",
      whyNeeded: "The target population is unclear.",
      goalIds: ["g1"],
      answer: null,
    },
    {
      id: "q2",
      question: "Which horizon matters?",
      whyNeeded: "An independent experiment can continue.",
      goalIds: ["g2"],
      answer: null,
    },
  ];
  render(
    <MissionDetail
      mission={{ ...mission, state: "waiting", questions }}
      onUpdated={updated}
      onClose={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("tab", { name: "Decisions (2)" }));
  fireEvent.change(screen.getByLabelText("Which sample should be used?"), {
    target: { value: "The baseline sample" },
  });
  fireEvent.click(
    screen.getAllByRole("button", { name: "Send research input" })[0],
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "mission_answer",
      expect.objectContaining({
        id: "mission1",
        questionId: "q1",
        answer: "The baseline sample",
        operationId: expect.any(String),
      }),
    ),
  );
  expect(screen.getByLabelText("Which horizon matters?")).toHaveValue("");
});
it("offers reconciliation without automatically retrying an uncertain action", async () => {
  render(
    <MissionDetail
      mission={{
        ...mission,
        state: "attention",
        activeChild: "child1",
        reason: "Turn outcome is unknown",
      }}
      onUpdated={vi.fn()}
      onClose={vi.fn()}
      onTask={vi.fn()}
    />,
  );
  expect(
    screen.getByRole("button", { name: "Reconcile recorded results" }),
  ).toBeEnabled();
  expect(mocks.invoke).not.toHaveBeenCalledWith(
    "mission_control",
    expect.anything(),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Reconcile recorded results" }),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("mission_control", {
      id: "mission1",
      revision: 1,
      action: "reconcile",
    }),
  );
});
it("keeps terminal budget outcomes distinct from scientific completion", () => {
  render(
    <MissionDetail
      mission={{ ...mission, state: "exhausted", actionsReserved: 32 }}
      onUpdated={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  expect(screen.getByText("Budget reached")).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Start research automation" }),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByText("Criteria assessed as met"),
  ).not.toBeInTheDocument();
});
it("refreshes a selected mission after a native mission change", async () => {
  const handler = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation((name, ...args) =>
    name === "mission_list"
      ? Promise.resolve([
          {
            id: mission.id,
            name: mission.definition.name,
            state: "draft",
            reason: "Prepared",
            rounds: 0,
            openQuestions: 0,
          },
        ])
      : handler(name, ...args),
  );
  render(<ResearchMissions />);
  fireEvent.click(
    await screen.findByRole("button", {
      name: /Investigate a theoretical result/,
    }),
  );
  await screen.findByRole("button", { name: "Start research automation" });
  mocks.invoke.mockImplementation((name, ...args) =>
    name === "mission_get"
      ? Promise.resolve({
          ...mission,
          state: "completed",
          reason: "All criteria assessed with evidence",
        })
      : handler(name, ...args),
  );
  act(() => mocks.listeners.get("missions:changed")?.());
  await screen.findByText("All criteria assessed with evidence");
  expect(
    screen.queryByRole("button", { name: "Start research automation" }),
  ).not.toBeInTheDocument();
});

import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ScriptRunControls, { profileFor } from "./ScriptRunControls";
import type {
  ExecutionProfile,
  ResearchExecution,
} from "../../lib/workbench/executionTypes";

const mocks = vi.hoisted(() => ({
  listExecutionProfiles: vi.fn(),
  listExecutions: vi.fn(),
  runExecution: vi.fn(),
  cancelExecution: vi.fn(),
  previewExecution: vi.fn(),
  authorizeExecution: vi.fn(),
  notify: vi.fn(),
}));
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: mocks,
}));
vi.mock("../../lib/projectClient", () => ({ projectClient: mocks }));
vi.mock("../DialogService", () => ({ notify: mocks.notify }));

const profile = (overrides: Partial<ExecutionProfile> = {}) =>
  ({
    id: "profile",
    name: "code/03_panel.do",
    inputs: ["code/03_panel.do"],
    testStatus: "passed",
    timeoutSeconds: 300,
    ...overrides,
  }) as ExecutionProfile;
const run = (outcome: ResearchExecution["outcome"], id = "run-1") =>
  ({
    id,
    profileId: "profile",
    outcome,
    createdAt: "2026-11-26T12:00:00Z",
  }) as ResearchExecution;
const preview = (authorized: boolean) => ({
  profileId: "profile",
  fingerprint: "print",
  command: ["stata", "-b", "do", "code/03_panel.do"],
  cwd: "/project",
  inputs: {},
  launch: {},
  outputs: [],
  authorized,
  boundary: "Host execution: this command runs with your access.",
});
const onSetup = vi.fn();
const onShowRun = vi.fn();
function show(path = "code/03_panel.do", dirty = false) {
  return render(
    <ScriptRunControls
      workspaceId="project"
      path={path}
      dirty={dirty}
      onSetup={onSetup}
      onShowRun={onShowRun}
    />,
  );
}
const play = () =>
  fireEvent.click(screen.getByRole("button", { name: "Run 03_panel.do" }));

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listExecutionProfiles.mockResolvedValue([profile()]);
  mocks.listExecutions.mockResolvedValue([]);
  mocks.previewExecution.mockResolvedValue(preview(true));
  mocks.authorizeExecution.mockResolvedValue(preview(true));
  mocks.runExecution.mockResolvedValue(run("running"));
  mocks.cancelExecution.mockResolvedValue(undefined);
});

it("runs an authorized script at once and follows it to the end", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  show();
  play();
  await screen.findByRole("button", { name: "Stop 03_panel.do" });
  expect(mocks.runExecution).toHaveBeenCalledWith(
    expect.objectContaining({
      profileId: "profile",
      sessionId: null,
      testOnly: false,
    }),
  );
  expect(screen.getByRole("button", { name: "Running…" })).toBeInTheDocument();
  mocks.listExecutions.mockResolvedValue([run("completed")]);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1100);
  });
  const status = await screen.findByRole("button", { name: "Completed" });
  fireEvent.click(status);
  expect(onShowRun).toHaveBeenCalledWith("code/03_panel.do", "run-1");
  expect(
    screen.getByRole("button", { name: "Run 03_panel.do" }),
  ).toBeInTheDocument();
  vi.useRealTimers();
});

it("shows the command for review before running one that changed", async () => {
  mocks.previewExecution.mockResolvedValue(preview(false));
  show();
  play();
  expect(
    await screen.findByText(/Host execution: this command runs/),
  ).toBeInTheDocument();
  expect(mocks.runExecution).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(mocks.authorizeExecution).not.toHaveBeenCalled();
  play();
  fireEvent.click(
    await screen.findByRole("button", {
      name: "Authorize this command and run",
    }),
  );
  await waitFor(() => expect(mocks.runExecution).toHaveBeenCalled());
  expect(mocks.authorizeExecution).toHaveBeenCalledWith("profile", "print");
});

it("tests a profile that has not passed its test yet", async () => {
  mocks.listExecutionProfiles.mockResolvedValue([
    profile({ testStatus: null }),
  ]);
  show();
  play();
  expect(
    await screen.findByRole("button", { name: "Test: Running…" }),
  ).toBeInTheDocument();
  expect(mocks.runExecution).toHaveBeenCalledWith(
    expect.objectContaining({ testOnly: true }),
  );
});

it("opens the setup when the script has no saved profile", async () => {
  mocks.listExecutionProfiles.mockResolvedValue([]);
  show();
  play();
  await waitFor(() => expect(onSetup).toHaveBeenCalledWith("code/03_panel.do"));
  expect(mocks.notify).toHaveBeenCalledWith(
    expect.stringMatching(/Set up how to run 03_panel.do/),
    "info",
  );
  expect(mocks.previewExecution).not.toHaveBeenCalled();
});

it("stops a running script and shows the last run on return", async () => {
  mocks.listExecutions.mockResolvedValue([
    run("failed", "old"),
    { ...run("running", "new"), createdAt: "2026-11-26T13:00:00Z" },
  ]);
  show();
  fireEvent.click(
    await screen.findByRole("button", { name: "Stop 03_panel.do" }),
  );
  await waitFor(() =>
    expect(mocks.cancelExecution).toHaveBeenCalledWith("new"),
  );
});

it("offers nothing for files that are not scripts, and waits for a save", () => {
  const view = show("data/panel.dta");
  expect(view.container).toBeEmptyDOMElement();
  view.unmount();
  show("code/03_panel.do", true);
  expect(
    screen.getByRole("button", { name: "Run 03_panel.do" }),
  ).toBeDisabled();
  expect(screen.getByRole("button", { name: "Run…" })).toBeDisabled();
});

it("prefers a tested profile named for the script", () => {
  const named = profile({ id: "named", testStatus: null });
  const tested = profile({ id: "tested", name: "Panel build" });
  const other = profile({ id: "other", name: "x", inputs: ["y.do"] });
  expect(profileFor([named, tested, other], "code/03_panel.do")?.id).toBe(
    "tested",
  );
  expect(profileFor([other], "code/03_panel.do")).toBeNull();
});

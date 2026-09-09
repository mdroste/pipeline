import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Manuscript from "./Manuscript";
import Responses from "./Responses";
import { JobLauncher } from "./Jobs";
import type { ProjectHome } from "../../lib/projectClient";
import type { ExecutionProfile } from "../../lib/workbenchTypes";
const mocks = vi.hoisted(() => ({
  records: vi.fn(),
  editor: vi.fn(),
  mutate: vi.fn(),
  previewExecution: vi.fn(),
  authorizeExecution: vi.fn(),
  runExecution: vi.fn(),
  invoke: vi.fn(),
  listExecutionProfiles: vi.fn(),
}));
vi.mock("../../lib/studioClient", async (original) => ({
  ...(await original<typeof import("../../lib/studioClient")>()),
  studioClient: mocks,
}));
vi.mock("../../lib/projectClient", () => ({ projectClient: mocks }));
vi.mock("../../lib/workbenchClient", () => ({ workbenchClient: mocks }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
// The save/conflict integration is independent of the editor's DOM implementation.
vi.mock("../file-workspace/SourceEditor", () => ({
  default: ({
    value,
    onChange,
    label,
  }: {
    value: string;
    onChange: (s: string) => void;
    label: string;
  }) => (
    <textarea
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}));
const data = {
  settings: { body: { manuscriptRevisionId: null } },
  papers: [],
  changes: [],
  tasks: [],
  applications: [],
  anchors: [],
  executions: [],
} as unknown as ProjectHome;
beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  mocks.records.mockResolvedValue([]);
  mocks.listExecutionProfiles.mockResolvedValue([]);
  mocks.editor.mockResolvedValue({
    path: "main.tex",
    content: "Old text",
    hash: "original-hash",
    checkpointId: null,
  });
});
afterEach(cleanup);
it("keeps a draft after a save conflict and sends the original hash", async () => {
  mocks.mutate.mockRejectedValue(new Error("external change"));
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByText("Load / refresh file"));
  await screen.findByLabelText("Manuscript source editor");
  fireEvent.change(screen.getByLabelText("Manuscript source editor"), {
    target: { value: "My draft" },
  });
  fireEvent.click(screen.getByText("Save working copy"));
  await screen.findByRole("alert");
  expect(screen.getByLabelText("Manuscript source editor")).toHaveValue(
    "My draft",
  );
  expect(mocks.mutate).toHaveBeenCalledWith("ws", {
    action: "saveText",
    checkpointId: null,
    path: "main.tex",
    expectedHash: "original-hash",
    content: "My draft",
  });
});
it("does not discard a dirty draft when another file is loaded", async () => {
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByText("Load / refresh file"));
  await screen.findByLabelText("Manuscript source editor");
  fireEvent.change(screen.getByLabelText("Manuscript source editor"), {
    target: { value: "Draft" },
  });
  fireEvent.click(screen.getByText("Load / refresh file"));
  await screen.findByRole("alert");
  expect(mocks.editor).toHaveBeenCalledTimes(1);
  expect(screen.getByLabelText("Manuscript source editor")).toHaveValue(
    "Draft",
  );
});
it("retains a trailing newline while editing build dependencies", async () => {
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  const dependencies = screen.getByLabelText(
    "Declared inputs (one per line, relative to build directory)",
  );
  fireEvent.change(dependencies, { target: { value: "main.tex\n" } });
  expect(dependencies).toHaveValue("main.tex\n");
  fireEvent.change(dependencies, {
    target: { value: "main.tex\nsection.tex\n" },
  });
  expect(dependencies).toHaveValue("main.tex\nsection.tex\n");
});
it("restores the original draft hash and does not resurrect a discarded draft", async () => {
  localStorage.setItem(
    "pipeline.manuscriptDraft.ws",
    JSON.stringify({
      file: {
        path: "main.tex",
        content: "Old text",
        hash: "retained-hash",
        checkpointId: null,
      },
      draft: "Retained draft",
    }),
  );
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByText("Load / refresh file"));
  await waitFor(() =>
    expect(screen.getByLabelText("Manuscript source editor")).toHaveValue(
      "Retained draft",
    ),
  );
  fireEvent.click(screen.getByText("Discard unsaved draft"));
  expect(localStorage.getItem("pipeline.manuscriptDraft.ws")).toBeNull();
  fireEvent.click(screen.getByText("Load / refresh file"));
  await waitFor(() => expect(mocks.editor).toHaveBeenCalledTimes(2));
  expect(screen.getByLabelText("Manuscript source editor")).toHaveValue(
    "Old text",
  );
});
it("reads no Workflow state until the bridge and run selection are requested", async () => {
  render(
    <Responses
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  await waitFor(() => expect(mocks.records).toHaveBeenCalled());
  expect(mocks.invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByLabelText("Enable importing Workflow findings"));
  expect(mocks.invoke).not.toHaveBeenCalled();
  mocks.invoke.mockResolvedValue([]);
  fireEvent.click(screen.getByText("Choose a completed Workflow run"));
  await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("list_runs"));
});
it("requires review and exact host authorization before a detached job starts", async () => {
  const profile = {
    id: "profile",
    revision: 1,
    testStatus: null,
  } as ExecutionProfile;
  mocks.previewExecution.mockResolvedValue({
    profileId: "profile",
    fingerprint: "fingerprint",
    command: ["python3", "analysis.py"],
    cwd: "/paper",
    boundary: "Host execution",
    inputs: {},
    launch: {},
    outputs: ["results.json"],
  });
  mocks.authorizeExecution.mockResolvedValue({});
  mocks.runExecution.mockResolvedValue({ id: "queued", outcome: "queued" });
  render(<JobLauncher profile={profile} />);
  expect(mocks.runExecution).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("Review test run"));
  await screen.findByText("Authorize and start test");
  expect(mocks.runExecution).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("Authorize and start test"));
  await waitFor(() => expect(mocks.runExecution).toHaveBeenCalled());
  expect(mocks.authorizeExecution).toHaveBeenCalledWith(
    "profile",
    "fingerprint",
  );
  expect(mocks.runExecution).toHaveBeenCalledWith(
    expect.objectContaining({
      profileId: "profile",
      testOnly: true,
      sessionId: null,
    }),
  );
});

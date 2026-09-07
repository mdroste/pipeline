import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceProjectSurface from "./WorkspaceProjectSurface";
import type { ProjectHome } from "../lib/projectClient";
const mocks = vi.hoisted(() => ({ home: vi.fn(), mutate: vi.fn(), getWorkspace: vi.fn() }));
vi.mock("../lib/projectClient", async original => ({ ...await original<typeof import("../lib/projectClient")>(), projectClient: mocks }));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: { getWorkspace: mocks.getWorkspace } }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
function home(): ProjectHome {
  return { settings: { id: "home", workspaceId: "workspace", kind: "home", revision: 4, updatedAt: "now", body: { manuscriptRevisionId: null, baselineExecutionId: null, briefNoteIds: [], excludedNoteIds: [], ignoredPaths: [], layout: "reading" } }, notes: [], noteHistory: [], tasks: [], anchors: [], papers: [], executions: [], ledger: { claims: [], evidence: [], staleClaims: [], unsupportedAcceptedClaims: [] }, inventory: { id: "inventory", workspaceId: "workspace", kind: "inventory", revision: 1, updatedAt: "now", body: { rootIdentity: "root", capturedAt: "now", complete: true, warnings: [], files: [{ path: "paper.tex", hash: "hash", size: 20, executable: false, status: "current" }] } }, changes: [], applications: [], workingCopyStatus: "Refresh files", fileAcceptance: true, contextPreview: "Reference results run: not selected." };
}
const openTask = { id: "task", workspaceId: "workspace", kind: "task", revision: 1, updatedAt: "now", body: { objective: "Revise proof", anchorId: null, expectedOutputs: [], expectedChecks: [], status: "open" } };
beforeEach(() => { localStorage.clear(); vi.clearAllMocks(); mocks.home.mockResolvedValue(home()); mocks.getWorkspace.mockResolvedValue({ id: "workspace", name: "Local research", root: "/research", revision: 1 }); mocks.mutate.mockResolvedValue({ id: "task", revision: 1, body: {} }); });
afterEach(cleanup);
const mount = async (conversation = vi.fn()) => { render(<WorkspaceProjectSurface workspaceId="workspace" onConversation={conversation} onWorkspaceChanged={vi.fn()}/>); await screen.findByRole("heading", { name: "Local research" }); };

it("opens signed-out project state without a model call or choosing reference results", async () => {
  const conversation = vi.fn(); await mount(conversation);
  expect(screen.getByLabelText("Reference results run")).toHaveValue("");
  expect(screen.getByLabelText("Current version of the paper")).toHaveValue("");
  expect(screen.getByText("research")).toBeInTheDocument();
  expect(mocks.mutate).not.toHaveBeenCalled(); expect(conversation).not.toHaveBeenCalled();
});
it("uses plain-language section names instead of provenance vocabulary", async () => {
  await mount();
  for (const label of ["Overview", "Documents", "Edits", "Research tools"]) expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
  expect(screen.queryByText(/accepted research state/i)).toBeNull();
  expect(screen.queryByText(/computational baseline/i)).toBeNull();
  expect(screen.queryByText(/isolated task copy/i)).toBeNull();
});
it("records the task and user-specified outputs and checks", async () => {
  await mount();
  fireEvent.change(screen.getByLabelText("New task"), { target: { value: "Check the envelope condition" } });
  fireEvent.change(screen.getByLabelText("Intended outputs"), { target: { value: "model.tex\nproof.md" } });
  fireEvent.change(screen.getByLabelText("Checks to run"), { target: { value: "Finite difference\nCompile manuscript" } });
  fireEvent.click(screen.getByRole("button", { name: "Add task" }));
  await waitFor(() => expect(mocks.mutate).toHaveBeenCalledWith("workspace", { action: "createTask", objective: "Check the envelope condition", anchorId: null, expectedOutputs: ["model.tex", "proof.md"], expectedChecks: ["Finite difference", "Compile manuscript"] }));
});
it("starts an edit from a task and copies only the explicitly selected files", async () => {
  const data = home(); data.tasks = [openTask]; mocks.home.mockResolvedValue(data);
  await mount();
  fireEvent.click(screen.getByRole("button", { name: "Start an edit" }));
  expect(screen.getByLabelText("Task for this edit")).toHaveValue("task");
  expect(screen.getByRole("button", { name: "Copy 0 files and start" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Select shown files" }));
  fireEvent.click(screen.getByRole("button", { name: "Copy 1 file and start" }));
  await waitFor(() => expect(mocks.mutate).toHaveBeenCalledWith("workspace", { action: "checkpoint", taskId: "task", paths: ["paper.tex"], backend: "copy" }));
});
it("explains that a folder is needed before an edit can start", async () => {
  mocks.getWorkspace.mockResolvedValue({ id: "workspace", name: "Local research", root: null, revision: 1 });
  await mount();
  expect(screen.getByText("No folder attached")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Edits" }));
  expect(screen.getByText(/Attach a folder on the Overview tab first/)).toBeInTheDocument();
  expect(screen.queryByLabelText("Task for this edit")).toBeNull();
});
it("opens a research conversation only when explicitly requested", async () => {
  const conversation = vi.fn().mockResolvedValue(undefined); await mount(conversation);
  fireEvent.click(screen.getByRole("button", { name: "Continue in a conversation" }));
  await waitFor(() => expect(conversation).toHaveBeenCalledWith("Continue from the project summary and open tasks."));
});

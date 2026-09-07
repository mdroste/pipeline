import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import WorkspacePage from "./WorkspacePage";
import type { ConversationSnapshot, WorkbenchSession, Workspace } from "../lib/workbenchTypes";

const mocks = vi.hoisted(() => ({ listWorkspaces: vi.fn(), pendingRequests: vi.fn(), connectCodex: vi.fn(), accountState: vi.fn(), listSessions: vi.fn(), conversationSnapshot: vi.fn(), reconcileSession: vi.fn(), updateSession: vi.fn(), moveSession: vi.fn(), deleteSession: vi.fn(), generateSessionTitle: vi.fn(), sendTurn: vi.fn(), listPapers: vi.fn(), effectiveHarness: vi.fn(), importPaper: vi.fn(), open: vi.fn() }));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open, save: vi.fn() }));
const workspace: Workspace = { id: "project", name: "Monetary policy", root: null, rootIdentity: null, settingsRevision: 1, revision: 1, archivedAt: null, missingRootAt: null, createdAt: "now", updatedAt: "now" };
let snapshot: ConversationSnapshot;
const scrollIntoView = vi.fn();
beforeEach(() => {
  vi.clearAllMocks(); localStorage.clear();
  const session: WorkbenchSession = { id: "conversation", title: "Policy discussion", workspaceId: null, paperId: null, presetId: null, overrides: {}, draft: "", revision: 1, archivedAt: null, createdAt: "now", updatedAt: "now" };
  snapshot = { workspace: null, session, sequence: 1, activeBinding: null, turns: [], items: [] };
  mocks.listWorkspaces.mockResolvedValue({ workspaces: [workspace] }); mocks.pendingRequests.mockResolvedValue([]);
  mocks.connectCodex.mockResolvedValue({}); mocks.accountState.mockResolvedValue({ status: "signedOut" });
  mocks.listSessions.mockImplementation(async () => ({ sessions: [snapshot.session] }));
  mocks.conversationSnapshot.mockImplementation(async () => snapshot);
  mocks.reconcileSession.mockResolvedValue(false);
  mocks.updateSession.mockImplementation(async request => { snapshot = { ...snapshot, session: { ...snapshot.session, ...request, revision: snapshot.session.revision + 1 } }; return { record: snapshot.session, sequence: 2 }; });
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: scrollIntoView });
});
const mount = async () => { render(<WorkspacePage onOpenSettings={vi.fn()} />); await screen.findByRole("heading", { name: "Policy discussion" }); };

it("opens composer tools lazily and explains voice availability", async () => {
  await mount();
  expect(mocks.listPapers).not.toHaveBeenCalled(); expect(mocks.effectiveHarness).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Add files and tools" }));
  fireEvent.click(screen.getByRole("button", { name: /Voice input/ }));
  expect(screen.getByText("Live voice conversations are not available in Workspace yet.")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Focus message for dictation" }));
  expect(screen.getByRole("textbox", { name: "Message" })).toHaveFocus();
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("requires a project before importing and preserves a draft when switching", async () => {
  await mount();
  fireEvent.change(screen.getByLabelText("Message"), { target: { value: "Keep this unsent question" } });
  fireEvent.click(screen.getByRole("button", { name: "Add files and tools" }));
  fireEvent.click(screen.getByRole("button", { name: /^Add filesDocuments/ }));
  fireEvent.click(screen.getByRole("button", { name: "Choose a project" }));
  fireEvent.click(screen.getByRole("button", { name: "Monetary policy" }));
  await waitFor(() => expect(mocks.updateSession).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "conversation", draft: "Keep this unsent question" })));
  await waitFor(() => expect(mocks.listSessions).toHaveBeenCalledWith("project", false));
  expect(mocks.importPaper).not.toHaveBeenCalled(); expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("jumps to an older response through search while keeping at most 200 transcript messages mounted", async () => {
  snapshot.items = Array.from({ length: 405 }, (_, index) => ({ id: `message-${index}`, turnId: null, providerItemId: `native-${index}`, itemKind: index % 2 ? "agentMessage" : "userMessage", payload: { text: index === 1 ? "The early identification argument" : `Message body ${index}` }, isFinal: true, createdAt: "now", updatedAt: "now" }));
  await mount();
  expect(screen.getAllByRole("article")).toHaveLength(200);
  expect(document.getElementById("workspace-message-message-1")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Outline" }));
  fireEvent.change(screen.getByLabelText("Search prompts and responses"), { target: { value: "identification" } });
  const outline = within(screen.getByRole("complementary", { name: "Conversation outline" }));
  fireEvent.click(outline.getByRole("button", { name: /Response.*The early identification argument/ }));
  await waitFor(() => expect(document.getElementById("workspace-message-message-1")).toHaveFocus());
  expect(screen.getAllByRole("article").length).toBeLessThanOrEqual(200);
  expect(scrollIntoView).toHaveBeenCalled();
  fireEvent.click(outline.getByRole("button", { name: "Close conversation outline" }));
  fireEvent.click(screen.getByRole("button", { name: "↓ Latest" }));
  expect(document.getElementById("workspace-message-message-404")).toBeInTheDocument();
});

it("does not send Enter while an input method is composing", async () => {
  await mount();
  const input = screen.getByLabelText("Message");
  fireEvent.change(input, { target: { value: "研究" } });
  fireEvent.keyDown(input, { key: "Enter", isComposing: true });
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("imports several files, reports failed extraction, and explicitly selects readable context", async () => {
  snapshot = { ...snapshot, workspace, session: { ...snapshot.session, workspaceId: workspace.id, draft: "My unsaved question" } };
  mocks.listPapers.mockResolvedValue([]); mocks.effectiveHarness.mockResolvedValue({ enabledModules: [] });
  mocks.open.mockResolvedValue(["/tmp/paper.md", "/tmp/scanned.pdf"]);
  mocks.importPaper.mockImplementation(async request => ({ paper: { id: request.title, title: request.title }, revision: { extraction: { status: request.title === "paper.md" ? "complete" : "failed", error: "No readable text" } } }));
  await mount();
  fireEvent.click(screen.getByRole("button", { name: "Add files and tools" }));
  fireEvent.click(screen.getByRole("button", { name: /^Add filesDocuments/ }));
  fireEvent.click(await screen.findByRole("button", { name: "Choose files…" }));
  await screen.findByText("paper.md"); await screen.findByText("Text unavailable: No readable text");
  expect(mocks.importPaper).toHaveBeenCalledTimes(2);
  const buttons = screen.getAllByRole("button", { name: "Use in conversation" });
  expect(buttons[0]).toBeDisabled();
  fireEvent.click(buttons[1]);
  await waitFor(() => expect(mocks.updateSession).toHaveBeenCalledWith(expect.objectContaining({ paperId: "paper.md", presetId: "research_assistant" })));
  expect(screen.getByLabelText("Message")).toHaveValue("My unsaved question");
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("blocks sending while files are being imported", async () => {
  snapshot = { ...snapshot, workspace, session: { ...snapshot.session, workspaceId: workspace.id, draft: "Read this" } };
  mocks.listPapers.mockResolvedValue([]); mocks.effectiveHarness.mockResolvedValue({ enabledModules: ["paper_context"] });
  let release!: (value: null) => void;
  mocks.open.mockImplementation(() => new Promise(resolve => { release = resolve; }));
  await mount(); fireEvent.click(screen.getByRole("button", { name: "Add files and tools" }));
  fireEvent.click(screen.getByRole("button", { name: /^Add filesDocuments/ }));
  fireEvent.click(await screen.findByRole("button", { name: "Choose files…" }));
  expect(screen.getByRole("button", { name: "Send ↑" })).toBeDisabled();
  expect(screen.getByLabelText("Project")).toBeDisabled();
  await act(async () => release(null));
  expect(screen.getByRole("button", { name: "Send ↑" })).toBeEnabled();
});

const openMenu = () => fireEvent.click(screen.getByRole("button", { name: "Conversation actions for Policy discussion" }));

it("renames a conversation from its row menu", async () => {
  await mount();
  vi.spyOn(window, "prompt").mockReturnValue("  Identification strategy  ");
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
  await waitFor(() => expect(mocks.updateSession).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "conversation", title: "Identification strategy" })));
  await screen.findByRole("heading", { name: "Identification strategy" });
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
});

it("deletes a conversation only after confirmation and clears the open transcript", async () => {
  await mount();
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Delete…" }));
  expect(mocks.deleteSession).not.toHaveBeenCalled();
  confirm.mockReturnValue(true);
  mocks.deleteSession.mockResolvedValue(7);
  mocks.listSessions.mockResolvedValue({ sessions: [] });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Delete…" }));
  await waitFor(() => expect(mocks.deleteSession).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "conversation" })));
  await screen.findByText("No conversations yet.");
  expect(screen.queryByRole("heading", { name: "Policy discussion" })).not.toBeInTheDocument();
});

it("moves an unfiled conversation into a project and follows it there", async () => {
  await mount();
  mocks.moveSession.mockImplementation(async request => { snapshot = { ...snapshot, workspace, session: { ...snapshot.session, workspaceId: request.workspaceId, revision: snapshot.session.revision + 1 } }; return { record: snapshot.session, sequence: 3 }; });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Move to project…" }));
  const dialog = screen.getByRole("dialog", { name: "Move “Policy discussion”" });
  expect(within(dialog).getByLabelText("Destination project")).toHaveValue("project");
  expect(within(dialog).queryByRole("option", { name: "Unfiled" })).not.toBeInTheDocument();
  fireEvent.click(within(dialog).getByRole("button", { name: "Move" }));
  await waitFor(() => expect(mocks.moveSession).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "conversation", workspaceId: "project", expectedRevision: 1 })));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  await waitFor(() => expect(mocks.listSessions).toHaveBeenCalledWith("project", false));
  expect(screen.getByLabelText("Project")).toHaveValue("project");
  expect(localStorage.getItem("pipeline.workspace.workspaceId")).toBe("project");
});

it("keeps a move dialog open with the error when the store refuses", async () => {
  await mount();
  mocks.moveSession.mockRejectedValue({ code: "invalid_input", message: "This conversation has research records in its current Workspace and cannot be moved" });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Move to project…" }));
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Move" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("cannot be moved");
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("regenerates a title on request and refreshes it from a background title event", async () => {
  await mount();
  mocks.generateSessionTitle.mockImplementation(async () => { snapshot = { ...snapshot, session: { ...snapshot.session, title: "Panel identification" } }; return "Panel identification"; });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Generate title" }));
  await screen.findByRole("heading", { name: "Panel identification" });
  expect(mocks.generateSessionTitle).toHaveBeenCalledWith("conversation");
  const handler = vi.mocked(listen).mock.calls.find(([name]) => name === "workbench:event")?.[1] as ((event: { payload: Record<string, unknown> }) => void) | undefined;
  expect(handler).toBeDefined();
  snapshot = { ...snapshot, session: { ...snapshot.session, title: "Automatic title" } };
  await act(async () => { handler!({ payload: { kind: "sessionTitleUpdated", epoch: 1, sessionId: "conversation", title: "Automatic title" } }); });
  await screen.findByRole("heading", { name: "Automatic title" });
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

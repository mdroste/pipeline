import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, expect, it, vi } from "vitest";
import WorkspacePage from "./WorkspacePage";
import { NavRailSlotProvider, useNavRailSlot } from "./nav-rail/navRailSlot";
import type {
  ConversationSnapshot,
  WorkbenchSession,
  Workspace,
} from "../lib/workbenchTypes";

const mocks = vi.hoisted(() => ({
  modelCatalog: vi.fn(),
  harnessCatalog: vi.fn(),
  listWorkspaces: vi.fn(),
  pendingRequests: vi.fn(),
  connectCodex: vi.fn(),
  accountState: vi.fn(),
  listSessions: vi.fn(),
  conversationSnapshot: vi.fn(),
  sessionSnapshot: vi.fn(),
  createSession: vi.fn(),
  openConversationFile: vi.fn(),
  readConversationFile: vi.fn(),
  reconcileSession: vi.fn(),
  updateSession: vi.fn(),
  moveSession: vi.fn(),
  deleteSession: vi.fn(),
  generateSessionTitle: vi.fn(),
  sendTurn: vi.fn(),
  interruptTurn: vi.fn(),
  exportConversation: vi.fn(),
  save: vi.fn(),
  listPapers: vi.fn(),
  effectiveHarness: vi.fn(),
  importPaper: vi.fn(),
  open: vi.fn(),
  confirmDialog: vi.fn(),
  promptDialog: vi.fn(),
}));
vi.mock("./DialogService", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./DialogService")>()),
  confirmDialog: mocks.confirmDialog,
  promptDialog: mocks.promptDialog,
}));
vi.mock("./WorkspaceProjectSurface", () => ({
  default: () => (
    <section aria-label="Project working area">Project content</section>
  ),
}));
vi.mock("../lib/programClient", () => ({
  programClient: { queue: vi.fn().mockResolvedValue([]) },
}));
vi.mock("../lib/workbench/workspaceClient", () => ({ workspaceClient: mocks }));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: mocks,
}));
vi.mock("../lib/workbench/harnessClient", () => ({ harnessClient: mocks }));
vi.mock("../lib/workbench/researchClient", () => ({ researchClient: mocks }));
vi.mock("../lib/workbench/runtimeClient", () => ({ runtimeClient: mocks }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: mocks.open,
  save: mocks.save,
}));
const workspace: Workspace = {
  id: "project",
  name: "Monetary policy",
  root: null,
  rootIdentity: null,
  settingsRevision: 1,
  revision: 1,
  archivedAt: null,
  missingRootAt: null,
  createdAt: "now",
  updatedAt: "now",
};
let snapshot: ConversationSnapshot;
const scrollIntoView = vi.fn();
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  const session: WorkbenchSession = {
    id: "conversation",
    title: "Policy discussion",
    workspaceId: null,
    paperId: null,
    presetId: null,
    overrides: {},
    draft: "",
    revision: 1,
    archivedAt: null,
    createdAt: "now",
    updatedAt: "now",
  };
  snapshot = {
    workspace: null,
    session,
    sequence: 1,
    activeBinding: null,
    turns: [],
    items: [],
  };
  mocks.listWorkspaces.mockResolvedValue({ workspaces: [workspace] });
  mocks.pendingRequests.mockResolvedValue([]);
  mocks.modelCatalog.mockResolvedValue({
    models: [
      {
        id: "test-model",
        model: "test-model",
        displayName: "Test model",
        supportedReasoningEfforts: [
          { reasoningEffort: "high", description: "High" },
        ],
      },
    ],
  });
  mocks.connectCodex.mockResolvedValue({});
  mocks.accountState.mockResolvedValue({ status: "signedOut" });
  mocks.listSessions.mockImplementation(async () => ({
    sessions: [snapshot.session],
  }));
  mocks.conversationSnapshot.mockImplementation(async () => snapshot);
  mocks.sessionSnapshot.mockImplementation(async (id: string) =>
    mocks.conversationSnapshot(id),
  );
  mocks.openConversationFile.mockResolvedValue(undefined);
  mocks.readConversationFile.mockResolvedValue({
    path: "figure.png",
    hash: "figure-hash",
    bytes: 1,
    text: null,
    base64: "AA==",
    mime: "image/png",
    editable: false,
    truncated: false,
    externalPath: null,
  });
  mocks.reconcileSession.mockResolvedValue(false);
  mocks.updateSession.mockImplementation(async (request) => {
    snapshot = {
      ...snapshot,
      session: {
        ...snapshot.session,
        ...request,
        revision: snapshot.session.revision + 1,
      },
    };
    return { record: snapshot.session, sequence: 2 };
  });
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: scrollIntoView,
  });
});
/** The app shell's sidebar slot, so the page's navigation has somewhere to go. */
function RailSlot() {
  return <div ref={useNavRailSlot()?.setSlot} aria-label="Sidebar" />;
}
function RailHost({ children }: { children: ReactNode }) {
  return (
    <NavRailSlotProvider>
      <RailSlot />
      {children}
    </NavRailSlotProvider>
  );
}

it("moves this chat and its unsent draft into the selected project before importing", async () => {
  const onAllProjects = vi.fn();
  mocks.effectiveHarness.mockResolvedValue({
    enabledModules: ["paper_context"],
  });
  mocks.moveSession.mockImplementation(async ({ workspaceId }) => {
    snapshot = {
      ...snapshot,
      workspace,
      session: {
        ...snapshot.session,
        workspaceId,
        revision: snapshot.session.revision + 1,
      },
    };
  });
  render(<WorkspacePage onAllProjects={onAllProjects} />, {
    wrapper: RailHost,
  });
  await screen.findByRole("heading", { name: "Policy discussion" });
  fireEvent.change(screen.getByLabelText("Message"), {
    target: { value: "Keep this unsent question" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add to message" }));
  fireEvent.click(screen.getByRole("menuitem", { name: /^Add files/ }));
  fireEvent.change(await screen.findByRole("combobox", { name: "Project" }), {
    target: { value: "project" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Move chat and continue" }),
  );
  await screen.findByRole("button", { name: "Choose files…" });
  expect(mocks.moveSession).toHaveBeenCalledWith(
    expect.objectContaining({
      sessionId: "conversation",
      workspaceId: "project",
    }),
  );
  expect(snapshot.session.draft).toBe("Keep this unsent question");
  expect(screen.getByLabelText("Message")).toHaveValue(
    "Keep this unsent question",
  );
  expect(onAllProjects).not.toHaveBeenCalled();
  expect(mocks.importPaper).not.toHaveBeenCalled();
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

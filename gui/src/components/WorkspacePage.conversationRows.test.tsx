// Conversation rows in the sidebar: rename, delete, move, recent responses, and
// titles. The setup matches WorkspacePage.test.tsx.

import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import WorkspacePage from "./WorkspacePage";
import { NavRailSlotProvider, useNavRailSlot } from "./nav-rail/navRailSlot";
import type {
  ConversationSnapshot,
  WorkbenchSession,
  Workspace,
} from "../lib/workbench/sessionTypes";

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
  previewConversationFile: vi.fn(),
  listConversationFiles: vi.fn(),
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
const mount = async () => {
  const view = render(<WorkspacePage />, { wrapper: RailHost });
  await screen.findByRole("heading", { name: "Policy discussion" });
  return view;
};

const openMenu = () => {
  if (
    !screen.queryByRole("button", {
      name: "Conversation actions for Policy discussion",
    })
  )
    fireEvent.click(
      screen.getByRole("button", { name: "Browse conversations" }),
    );
  fireEvent.click(
    screen.getByRole("button", {
      name: "Conversation actions for Policy discussion",
    }),
  );
};

it("renames a conversation from its row menu", async () => {
  await mount();
  mocks.promptDialog.mockResolvedValue("  Identification strategy  ");
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
  await waitFor(() =>
    expect(mocks.updateSession).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: "conversation",
        title: "Identification strategy",
      }),
    ),
  );
  await screen.findByRole("heading", { name: "Identification strategy" });
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
});

it("deletes a conversation only after confirmation and clears the open transcript", async () => {
  await mount();
  mocks.confirmDialog.mockResolvedValue(false);
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Delete…" }));
  await waitFor(() => expect(mocks.confirmDialog).toHaveBeenCalledTimes(1));
  expect(mocks.deleteSession).not.toHaveBeenCalled();
  // The prompt also covers Claude and API conversations.
  expect(mocks.confirmDialog.mock.calls[0][0]).toContain(
    "provider may retain its own history",
  );
  expect(mocks.confirmDialog.mock.calls[0][0]).not.toContain("ChatGPT");
  mocks.confirmDialog.mockResolvedValue(true);
  mocks.deleteSession.mockResolvedValue(7);
  mocks.listSessions.mockResolvedValue({ sessions: [] });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Delete…" }));
  await waitFor(() =>
    expect(mocks.deleteSession).toHaveBeenCalledWith(
      expect.objectContaining({ sessionId: "conversation" }),
    ),
  );
  await screen.findByText("No conversations yet.");
  expect(
    screen.queryByRole("heading", { name: "Policy discussion" }),
  ).not.toBeInTheDocument();
});

it("reads recent responses in plain words, not stored values", async () => {
  snapshot = {
    ...snapshot,
    turns: [
      {
        id: "turn-1",
        bindingId: "binding",
        clientSubmissionId: "submission",
        providerTurnId: null,
        state: "inProgress",
        error: null,
        createdAt: "2026-11-01T10:30:00Z",
        updatedAt: "2026-11-01T10:30:00Z",
        terminalAt: null,
      },
    ],
  };
  await mount();
  fireEvent.click(screen.getByRole("button", { name: "Conversation menu" }));
  fireEvent.click(
    await screen.findByRole("menuitem", { name: "Activity & follow-ups" }),
  );
  const activity = await screen.findByRole("region", {
    name: "Assistant activity",
  });
  await waitFor(() => expect(activity.textContent).toContain("Working"));
  expect(activity.textContent).not.toContain("inProgress");
  expect(activity.textContent).not.toContain("2026-11-01T10:30:00Z");
});

it("moves an unfiled conversation into a project and follows it there", async () => {
  await mount();
  mocks.moveSession.mockImplementation(async (request) => {
    snapshot = {
      ...snapshot,
      workspace,
      session: {
        ...snapshot.session,
        workspaceId: request.workspaceId,
        revision: snapshot.session.revision + 1,
      },
    };
    return { record: snapshot.session, sequence: 3 };
  });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Move to project…" }));
  const dialog = screen.getByRole("dialog", {
    name: "Move “Policy discussion”",
  });
  expect(within(dialog).getByLabelText("Destination project")).toHaveValue(
    "project",
  );
  expect(
    within(dialog).queryByRole("option", { name: "Unfiled" }),
  ).not.toBeInTheDocument();
  fireEvent.click(within(dialog).getByRole("button", { name: "Move" }));
  await waitFor(() =>
    expect(mocks.moveSession).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: "conversation",
        workspaceId: "project",
        expectedRevision: 1,
      }),
    ),
  );
  await waitFor(() =>
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
  );
  await waitFor(() =>
    expect(mocks.listSessions).toHaveBeenCalledWith("project", false, null),
  );
  expect(
    (
      await screen.findAllByRole("button", {
        name: "Switch project: Monetary policy",
      })
    )[0],
  ).toBeVisible();
  expect(localStorage.getItem("pipeline.workspace.workspaceId")).toBe(
    "project",
  );
});

it("keeps a move dialog open with the error when the store refuses", async () => {
  await mount();
  mocks.moveSession.mockRejectedValue({
    code: "invalid_input",
    message:
      "This conversation has research records in its current Workspace and cannot be moved",
  });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Move to project…" }));
  fireEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", { name: "Move" }),
  );
  await waitFor(() =>
    expect(
      within(screen.getByRole("dialog")).getByRole("alert"),
    ).toHaveTextContent("cannot be moved"),
  );
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("regenerates a title on request and refreshes it from a background title event", async () => {
  await mount();
  mocks.generateSessionTitle.mockImplementation(async () => {
    snapshot = {
      ...snapshot,
      session: { ...snapshot.session, title: "Panel identification" },
    };
    return "Panel identification";
  });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Generate title" }));
  await screen.findByRole("heading", { name: "Panel identification" });
  expect(mocks.generateSessionTitle).toHaveBeenCalledWith("conversation");
  const handler = vi
    .mocked(listen)
    .mock.calls.find(([name]) => name === "workbench:event")?.[1] as
    ((event: { payload: Record<string, unknown> }) => void) | undefined;
  expect(handler).toBeDefined();
  snapshot = {
    ...snapshot,
    session: { ...snapshot.session, title: "Automatic title" },
  };
  await act(async () => {
    handler!({
      payload: {
        kind: "sessionTitleUpdated",
        epoch: 1,
        sessionId: "conversation",
        title: "Automatic title",
      },
    });
  });
  await screen.findByRole("heading", { name: "Automatic title" });
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

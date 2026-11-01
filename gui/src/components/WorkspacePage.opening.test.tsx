import { fireEvent, render, screen, waitFor } from "@testing-library/react";
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
  listWorkspaces: vi.fn(),
  pendingRequests: vi.fn(),
  connectCodex: vi.fn(),
  accountState: vi.fn(),
  activeTurn: vi.fn(),
  listSessions: vi.fn(),
  conversationSnapshot: vi.fn(),
  sessionSnapshot: vi.fn(),
  reconcileSession: vi.fn(),
  updateSession: vi.fn(),
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
vi.mock("../lib/workbench/runtimeClient", () => ({ runtimeClient: mocks }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

const workspace = { id: "project", name: "Monetary policy" } as Workspace;
let snapshot: ConversationSnapshot;
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  const session = {
    id: "conversation",
    title: "Policy discussion",
    workspaceId: null,
    overrides: {},
    draft: "",
    revision: 1,
    archivedAt: null,
  } as unknown as WorkbenchSession;
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
  mocks.connectCodex.mockResolvedValue({});
  mocks.accountState.mockResolvedValue({ status: "signedOut" });
  mocks.activeTurn.mockResolvedValue(null);
  mocks.listSessions.mockImplementation(async () => ({
    sessions: [snapshot.session],
  }));
  mocks.conversationSnapshot.mockImplementation(async () => snapshot);
  mocks.sessionSnapshot.mockImplementation(async (id: string) =>
    mocks.conversationSnapshot(id),
  );
  mocks.reconcileSession.mockResolvedValue(false);
  mocks.updateSession.mockImplementation(async () => ({
    record: snapshot.session,
    sequence: 2,
  }));
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

it("puts the cursor in the composer when a conversation opens", async () => {
  const second = { ...snapshot.session, id: "second", title: "Second chat" };
  mocks.listSessions.mockResolvedValue({
    sessions: [snapshot.session, second],
  });
  mocks.conversationSnapshot.mockImplementation(async (id: string) =>
    id === second.id ? { ...snapshot, session: second } : snapshot,
  );
  await mount();
  fireEvent.click(screen.getByRole("button", { name: "Second chat" }));
  await screen.findByRole("heading", { name: "Second chat" });
  await waitFor(() => expect(screen.getByLabelText("Message")).toHaveFocus());
});

it("reveals a conversation in the project it opens, not the project left", async () => {
  const other = { ...workspace, id: "other", name: "Other" };
  const inOther = {
    ...snapshot,
    workspace: other,
    session: { ...snapshot.session, workspaceId: other.id },
  };
  mocks.listWorkspaces.mockResolvedValue({ workspaces: [workspace, other] });
  localStorage.setItem(`pipeline.workspace.view.${workspace.id}`, "project");
  localStorage.setItem(`pipeline.workspace.view.${other.id}`, "project");
  const view = render(
    <WorkspacePage
      entryRequest={1}
      entrySurface="project"
      entryTarget={{
        page: "workspace",
        projectId: workspace.id,
        surface: "project",
      }}
    />,
  );
  await screen.findByRole("region", { name: "Project working area" });
  snapshot = inOther;
  view.rerender(
    <WorkspacePage
      entryRequest={2}
      entrySurface="chat"
      entryTarget={{
        page: "workspace",
        projectId: other.id,
        sessionId: "conversation",
        surface: "chat",
      }}
    />,
  );
  await waitFor(() =>
    expect(localStorage.getItem(`pipeline.workspace.view.${other.id}`)).toBe(
      "assistant",
    ),
  );
  expect(localStorage.getItem(`pipeline.workspace.view.${workspace.id}`)).toBe(
    "project",
  );
});

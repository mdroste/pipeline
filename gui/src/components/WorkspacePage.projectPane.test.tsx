import { act, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { appEvents } from "../lib/appEvents";
import WorkspacePage from "./WorkspacePage";
import { NavRailSlotProvider, useNavRailSlot } from "./nav-rail/navRailSlot";
import type { ConversationSnapshot, Workspace } from "../lib/workbenchTypes";

const mocks = vi.hoisted(() => ({
  modelCatalog: vi.fn(),
  harnessCatalog: vi.fn(),
  effectiveHarness: vi.fn(),
  listWorkspaces: vi.fn(),
  pendingRequests: vi.fn(),
  connectCodex: vi.fn(),
  accountState: vi.fn(),
  listSessions: vi.fn(),
  conversationSnapshot: vi.fn(),
  sessionSnapshot: vi.fn(),
  reconcileSession: vi.fn(),
  updateSession: vi.fn(),
  listPapers: vi.fn(),
  surfaceRender: vi.fn(),
}));
vi.mock("./WorkspaceProjectSurface", () => ({
  default: () => {
    mocks.surfaceRender();
    return <section aria-label="Project working area">Project content</section>;
  },
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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

const workspace = {
  id: "project",
  name: "Monetary policy",
  root: "/research",
  revision: 1,
} as Workspace;
let snapshot: ConversationSnapshot;

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  snapshot = {
    workspace,
    sequence: 1,
    turns: [],
    items: [],
    session: {
      id: "conversation",
      title: "Policy discussion",
      workspaceId: workspace.id,
      paperId: null,
      presetId: null,
      overrides: {},
      draft: "",
      revision: 1,
      archivedAt: null,
      createdAt: "now",
      updatedAt: "now",
    },
    activeBinding: {
      id: "binding",
      sessionId: "conversation",
      runtimeNamespace: "test",
      providerThreadId: "thread",
      incarnation: 1,
      createdAt: "now",
      retiredAt: null,
      retirementReason: null,
      harnessFingerprint: null,
      instructionSources: [],
    },
  };
  mocks.listWorkspaces.mockResolvedValue({ workspaces: [workspace] });
  mocks.pendingRequests.mockResolvedValue([]);
  mocks.modelCatalog.mockResolvedValue({ models: [] });
  mocks.connectCodex.mockResolvedValue({});
  mocks.accountState.mockResolvedValue({ status: "signedOut" });
  mocks.listSessions.mockImplementation(async () => ({
    sessions: [snapshot.session],
  }));
  mocks.conversationSnapshot.mockImplementation(async () => snapshot);
  mocks.sessionSnapshot.mockImplementation(async () => snapshot);
  mocks.reconcileSession.mockResolvedValue(false);
  mocks.updateSession.mockImplementation(async () => ({
    record: snapshot.session,
    sequence: 2,
  }));
  HTMLElement.prototype.scrollIntoView = vi.fn();
});

function RailHost({ children }: { children: ReactNode }) {
  const Slot = () => (
    <div ref={useNavRailSlot()?.setSlot} aria-label="Sidebar" />
  );
  return (
    <NavRailSlotProvider>
      <Slot />
      {children}
    </NavRailSlotProvider>
  );
}

describe("entering a project at a named destination", () => {
  const object = { kind: "source", id: "s", revision: "r" };
  const layoutKey = `pipeline.desk.${workspace.id}`;
  const saved = () => JSON.parse(localStorage.getItem(layoutKey)!);
  const target = {
    page: "workspace",
    projectId: workspace.id,
    surface: "project",
    destination: "tasks",
  } as const;
  beforeEach(() =>
    localStorage.setItem(
      layoutKey,
      JSON.stringify({ tab: "overview", object, comparison: object }),
    ),
  );

  it("clears a saved research item so it can't cover the destination", async () => {
    const heard = vi.fn();
    const off = appEvents.on("workspace-destination", heard);
    render(<WorkspacePage entryRequest={1} entryTarget={target} />);
    await waitFor(() => expect(saved().object).toBeNull());
    expect(saved().comparison).toBeNull();
    // A project already on screen hears about it instead.
    expect(heard).toHaveBeenCalledWith({
      workspaceId: workspace.id,
      tab: "tasks",
    });
    off();
  });

  it("restores the saved research item on a relaunch", async () => {
    render(<WorkspacePage entryRequest={0} entryTarget={target} />);
    await screen.findByRole("region", { name: "Project working area" });
    await act(async () => {});
    expect(saved().object).toEqual(object);
  });
});

it("streams a reply without rendering the project pane again", async () => {
  localStorage.setItem("pipeline.workspace.workspaceId", workspace.id);
  localStorage.setItem("pipeline.workspace.sessionId", "conversation");
  localStorage.setItem(`pipeline.workspace.view.${workspace.id}`, "assistant");
  render(<WorkspacePage />, { wrapper: RailHost });
  await screen.findByRole("heading", { name: "Policy discussion" });
  await act(async () => {});
  const handler = vi
    .mocked(listen)
    .mock.calls.filter(([name]) => name === "workbench:event")
    .at(-1)?.[1] as (event: { payload: Record<string, unknown> }) => void;
  const renders = mocks.surfaceRender.mock.calls.length;
  expect(renders).toBeGreaterThan(0);
  for (const delta of ["Streamed ", "reply ", "text"]) {
    await act(async () => {
      handler({
        payload: {
          kind: "agentMessageDelta",
          epoch: 1,
          threadId: "thread",
          turnId: "turn",
          itemId: "item",
          delta,
        },
      });
    });
    await act(() => new Promise((done) => setTimeout(done, 80)));
  }
  await screen.findByText(/Streamed reply text/);
  expect(mocks.surfaceRender).toHaveBeenCalledTimes(renders);
});

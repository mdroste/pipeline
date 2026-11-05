import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import ProjectIndexPage from "./ProjectIndexPage";
import type { ProjectIndexItem } from "../lib/projectIndex";
import type { WorkbenchSession } from "../lib/workbenchTypes";

const session = (
  id: string,
  title: string,
  workspaceId: string | null,
  extra: Partial<WorkbenchSession> = {},
) =>
  ({
    id,
    title,
    workspaceId,
    archivedAt: null,
    updatedAt: `2026-10-${id.padStart(2, "0")}T10:00:00Z`,
    ...extra,
  }) as WorkbenchSession;

const mocks = vi.hoisted(() => ({
  listWorkspaces: vi.fn(),
  listSessions: vi.fn(),
  sessionSnapshot: vi.fn(),
  moveSession: vi.fn(),
}));
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: { listWorkspaces: mocks.listWorkspaces },
}));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: mocks,
}));

const project: ProjectIndexItem = {
  id: "trade",
  name: "Trade paper",
  root: null,
  missingRootAt: null,
  updatedAt: "2026-10-02T10:00:00Z",
  brief: null,
  conversation: null,
  activity: null,
  nextTask: null,
  proposedNotes: 0,
  interruptedEdits: 0,
};
vi.mock("../hooks/useProjectIndex", () => ({
  default: () => ({
    projects: [project],
    loading: false,
    error: null,
    refresh: vi.fn(),
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
const renderPage = (
  onOpen = vi.fn(),
  onNew = vi.fn(),
  onOpenProject = vi.fn(),
) =>
  render(
    <ProjectIndexPage
      onResumeConversation={onOpen}
      onNewConversation={onNew}
      onOpenProject={onOpenProject}
    />,
  );

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  mocks.listWorkspaces.mockResolvedValue({
    workspaces: [{ id: "trade", name: "Trade paper" }],
  });
  mocks.listSessions.mockImplementation(
    async (workspaceId: string | null, archived: boolean) => ({
      sessions:
        workspaceId === "trade"
          ? [session("2", "Elasticity priors", "trade")]
          : [
              session("3", "Bartik shares", null),
              ...(archived
                ? [
                    session("1", "Old question", null, {
                      archivedAt: "2026-10-02T10:00:00Z",
                    }),
                  ]
                : []),
            ],
    }),
  );
});

it("lists conversations across projects, newest first", async () => {
  const user = userEvent.setup();
  const onOpen = vi.fn().mockResolvedValue(true);
  renderPage(onOpen);

  const newest = await screen.findByRole("button", { name: "Bartik shares" });
  const older = screen.getByRole("button", { name: "Elasticity priors" });
  expect(
    newest.compareDocumentPosition(older) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
  expect(
    screen.getByRole("button", { name: "Open project Trade paper" }),
  ).toBeVisible();
  await user.click(older);
  expect(onOpen).toHaveBeenCalledWith("2");
});

it("filters to unfiled and archived conversations and by search", async () => {
  const user = userEvent.setup();
  renderPage();
  await screen.findByRole("button", { name: "Bartik shares" });

  await user.click(screen.getByRole("button", { name: "No project" }));
  expect(
    screen.queryByRole("button", { name: "Elasticity priors" }),
  ).toBeNull();

  await user.click(screen.getByRole("button", { name: "Archived" }));
  expect(
    await screen.findByRole("button", { name: "Old question" }),
  ).toBeVisible();
  expect(screen.queryByRole("button", { name: "Bartik shares" })).toBeNull();

  await user.click(screen.getByRole("button", { name: "All chats" }));
  await user.type(
    screen.getByRole("searchbox", { name: "Search projects and chats" }),
    "nothing like this",
  );
  await waitFor(() => expect(screen.getByText(/No chats match/)).toBeVisible());
});

it("reports a list that cannot load", async () => {
  mocks.listWorkspaces.mockRejectedValue(new Error("store offline"));
  renderPage();
  expect(await screen.findByRole("alert")).toHaveTextContent("store offline");
  expect(screen.getByRole("button", { name: "Trade paper" })).toBeVisible();
});

it("starts an independent chat and links a chat to its project", async () => {
  const user = userEvent.setup();
  const onNew = vi.fn();
  const onOpenProject = vi.fn().mockResolvedValue(undefined);
  renderPage(vi.fn(), onNew, onOpenProject);
  await user.click(screen.getByRole("button", { name: "+ New chat" }));
  expect(onNew).toHaveBeenCalledOnce();
  expect(onOpenProject).not.toHaveBeenCalled();
  await user.click(
    await screen.findByRole("button", { name: "Open project Trade paper" }),
  );
  expect(onOpenProject).toHaveBeenCalledWith("trade");
  await user.type(screen.getByRole("searchbox"), "Trade paper");
  expect(screen.getByRole("button", { name: "Trade paper" })).toBeVisible();
  expect(
    screen.getByRole("button", { name: "Elasticity priors" }),
  ).toBeVisible();
  expect(screen.queryByRole("button", { name: "Bartik shares" })).toBeNull();
});

it("moves an unfiled chat into a project and refreshes its membership", async () => {
  const user = userEvent.setup();
  const chat = session("3", "Bartik shares", null);
  mocks.sessionSnapshot.mockResolvedValue({
    session: { ...chat, revision: 7 },
  });
  mocks.moveSession.mockImplementation(async () => {
    mocks.listSessions.mockImplementation(
      async (workspaceId: string | null) => ({
        sessions:
          workspaceId === "trade" ? [{ ...chat, workspaceId: "trade" }] : [],
      }),
    );
  });
  renderPage();
  await user.click(
    await screen.findByRole("button", {
      name: "Move Bartik shares to project",
    }),
  );
  const dialog = screen.getByRole("dialog", { name: "Move “Bartik shares”" });
  expect(within(dialog).getByRole("combobox")).toHaveValue("trade");
  await user.click(within(dialog).getByRole("button", { name: "Move" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mocks.moveSession).toHaveBeenCalledWith({
    sessionId: "3",
    workspaceId: "trade",
    expectedRevision: 7,
    operationId: expect.any(String),
  });
  await user.click(screen.getByRole("button", { name: "No project" }));
  expect(await screen.findByText(/No chats without a project/)).toBeVisible();
});

it("keeps a failed move retryable and can remove a chat's project", async () => {
  const user = userEvent.setup();
  mocks.sessionSnapshot.mockResolvedValue({ session: { revision: 9 } });
  mocks.moveSession.mockRejectedValueOnce(new Error("Chat is busy"));
  renderPage();
  await user.click(
    await screen.findByRole("button", {
      name: "Move Elasticity priors to project",
    }),
  );
  const dialog = within(screen.getByRole("dialog"));
  expect(dialog.getByRole("combobox")).toHaveValue("");
  await user.click(dialog.getByRole("button", { name: "Move" }));
  expect(await dialog.findByRole("alert")).toHaveTextContent("Chat is busy");
  mocks.moveSession.mockResolvedValueOnce(undefined);
  await user.click(dialog.getByRole("button", { name: "Move" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mocks.moveSession).toHaveBeenLastCalledWith({
    sessionId: "2",
    workspaceId: null,
    expectedRevision: 9,
    operationId: expect.any(String),
  });
});

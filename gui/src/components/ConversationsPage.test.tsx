import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import ConversationsPage from "./ConversationsPage";
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
}));
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: { listWorkspaces: mocks.listWorkspaces },
}));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: { listSessions: mocks.listSessions },
}));

beforeEach(() => {
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
  render(<ConversationsPage onOpen={onOpen} onNew={vi.fn()} />);

  const newest = await screen.findByRole("button", { name: "Bartik shares" });
  const older = screen.getByRole("button", { name: "Elasticity priors" });
  expect(
    newest.compareDocumentPosition(older) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
  expect(screen.getByText(/^Trade paper ·/)).toBeVisible();
  await user.click(older);
  expect(onOpen).toHaveBeenCalledWith("2");
});

it("filters to unfiled and archived conversations and by search", async () => {
  const user = userEvent.setup();
  render(<ConversationsPage onOpen={vi.fn()} onNew={vi.fn()} />);
  await screen.findByRole("button", { name: "Bartik shares" });

  await user.click(screen.getByRole("button", { name: "Unfiled" }));
  expect(
    screen.queryByRole("button", { name: "Elasticity priors" }),
  ).toBeNull();

  await user.click(screen.getByRole("button", { name: "Archived" }));
  expect(
    await screen.findByRole("button", { name: "Old question" }),
  ).toBeVisible();
  expect(screen.queryByRole("button", { name: "Bartik shares" })).toBeNull();

  await user.click(screen.getByRole("button", { name: "All" }));
  await user.type(
    screen.getByRole("searchbox", { name: "Search conversations" }),
    "nothing like this",
  );
  await waitFor(() =>
    expect(screen.getByText(/No conversations match/)).toBeVisible(),
  );
});

it("reports a list that cannot load", async () => {
  mocks.listWorkspaces.mockRejectedValue(new Error("store offline"));
  render(<ConversationsPage onOpen={vi.fn()} onNew={vi.fn()} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("store offline");
});

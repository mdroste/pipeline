import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import AppSearch from "./AppSearch";

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  listWorkspaces: vi.fn(),
  listSessions: vi.fn(),
  listRuns: vi.fn(),
  listTasks: vi.fn(),
}));
vi.mock("../lib/router", () => ({ router: { navigate: mocks.navigate } }));
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: { listWorkspaces: mocks.listWorkspaces },
}));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: { listSessions: mocks.listSessions },
}));
vi.mock("../lib/reviewHistoryClient", () => ({
  reviewHistoryClient: { list: mocks.listRuns },
}));
vi.mock("../lib/taskClient", () => ({ taskClient: { list: mocks.listTasks } }));

const props = () => ({
  onClose: vi.fn(),
  onOpenConversation: vi.fn(),
  onOpenRun: vi.fn(),
});

beforeEach(() => {
  mocks.navigate.mockClear();
  mocks.listWorkspaces.mockResolvedValue({
    workspaces: [{ id: "trade", name: "Trade paper", updatedAt: "2026-10-30" }],
  });
  mocks.listSessions.mockImplementation(async (id: string | null) => ({
    sessions: id
      ? []
      : [
          {
            id: "chat",
            title: "Bartik shares",
            workspaceId: null,
            updatedAt: "2026-10-30",
          },
        ],
  }));
  mocks.listRuns.mockResolvedValue([
    { run_id: "run", input_name: "spillovers.pdf", profile_name: "Auto" },
  ]);
  mocks.listTasks.mockResolvedValue([{ id: "task", name: "Citation check" }]);
});

const search = async (text: string) => {
  const user = userEvent.setup();
  await user.type(
    await screen.findByRole("combobox", { name: "Search Pipeline" }),
    text,
  );
  return user;
};

it("finds projects, project views, and automations through the router", async () => {
  const handlers = props();
  render(<AppSearch {...handlers} />);
  await screen.findByRole("option", { name: /Trade paper/ });

  const user = await search("specification");
  await user.keyboard("{Enter}");
  expect(mocks.navigate).toHaveBeenCalledWith({
    page: "workspace",
    projectId: "trade",
    surface: "project",
    destination: "grids",
  });
  expect(handlers.onClose).toHaveBeenCalled();
});

it("opens conversations and reviews through their own paths", async () => {
  const handlers = props();
  const user = userEvent.setup();
  render(<AppSearch {...handlers} />);
  await user.click(await screen.findByRole("option", { name: /Bartik/ }));
  expect(handlers.onOpenConversation).toHaveBeenCalledWith("chat");

  await user.type(
    screen.getByRole("combobox", { name: "Search Pipeline" }),
    "spillovers",
  );
  await user.click(await screen.findByRole("option", { name: /spillovers/ }));
  expect(handlers.onOpenRun).toHaveBeenCalledWith("run");
});

it("keeps working when one source fails, and says when nothing matches", async () => {
  mocks.listRuns.mockRejectedValue(new Error("offline"));
  render(<AppSearch {...props()} />);
  await search("citation");
  expect(
    await screen.findByRole("option", { name: /Citation check/ }),
  ).toBeVisible();
  await search("zzz");
  await waitFor(() =>
    expect(screen.getByText("Nothing matches “citationzzz”.")).toBeVisible(),
  );
});

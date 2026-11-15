import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import ProjectIndexPage from "./ProjectIndexPage";
import type { ProjectIndexItem } from "../lib/projectIndex";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../hooks/useConversationIndex", () => ({
  default: () => ({
    conversations: [],
    loading: false,
    error: null,
    refresh: vi.fn(),
  }),
}));
const project: ProjectIndexItem = {
  id: "networks",
  name: "Production networks",
  root: null,
  missingRootAt: null,
  updatedAt: "2026-09-13T12:00:00Z",
  brief: {
    id: "question",
    title: "How do shocks propagate?",
    kind: "question",
    updatedAt: "2026-09-12T12:00:00Z",
  },
  conversation: {
    id: "conversation-actual",
    title: "Aggregation argument",
    kind: "conversation",
    updatedAt: "2026-09-13T12:00:00Z",
  },
  activity: null,
  nextTask: null,
  proposedNotes: 2,
  interruptedEdits: 0,
};
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  invoke.mockResolvedValue([project]);
});
afterEach(cleanup);

it("opens the selected project with research details and chats out of the picker", async () => {
  const onOpenProject = vi.fn().mockResolvedValue(undefined);
  const onResumeConversation = vi.fn().mockResolvedValue(true);
  const user = userEvent.setup();
  render(
    <ProjectIndexPage
      onNewConversation={vi.fn()}
      {...{ onOpenProject, onResumeConversation }}
    />,
  );
  await screen.findByRole("button", { name: "Production networks" });
  expect(
    screen.getByRole("heading", { level: 1, name: "Projects" }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "New project" }),
  ).toBeInTheDocument();
  expect(onOpenProject).not.toHaveBeenCalled();
  expect(onResumeConversation).not.toHaveBeenCalled();
  expect(invoke).toHaveBeenCalledExactlyOnceWith("workbench_project_index");
  expect(
    screen.queryByRole("region", { name: "Chats" }),
  ).not.toBeInTheDocument();
  expect(screen.queryByText(/suggested notes/)).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: /Resume conversation/ }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Production networks" }));
  expect(onOpenProject).toHaveBeenCalledWith("networks");
});

it("searches research questions and persists pins independently from project data", async () => {
  const user = userEvent.setup();
  const props = { onOpenProject: vi.fn(), onResumeConversation: vi.fn() };
  const view = render(
    <ProjectIndexPage onNewConversation={vi.fn()} {...props} />,
  );
  await screen.findByRole("button", { name: "Production networks" });
  await user.type(screen.getByRole("searchbox"), "shocks");
  expect(
    screen.getByRole("button", { name: "Production networks" }),
  ).toBeVisible();
  await user.click(
    screen.getByRole("button", { name: "Pin Production networks" }),
  );
  expect(
    within(screen.getByRole("region", { name: "Pinned" })).getByText(
      "Production networks",
    ),
  ).toBeVisible();
  view.unmount();
  render(<ProjectIndexPage onNewConversation={vi.fn()} {...props} />);
  expect(
    await screen.findByRole("button", { name: "Unpin Production networks" }),
  ).toHaveAttribute("aria-pressed", "true");
  expect(
    invoke.mock.calls.every(([name]) => name === "workbench_project_index"),
  ).toBe(true);
});

it("reports load and open failures with retry and retains the project list", async () => {
  invoke.mockRejectedValueOnce(new Error("Store unavailable"));
  const user = userEvent.setup();
  render(
    <ProjectIndexPage
      onNewConversation={vi.fn()}
      onOpenProject={vi
        .fn()
        .mockRejectedValue(new Error("Project unavailable"))}
      onResumeConversation={vi.fn()}
    />,
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Store unavailable",
  );
  await user.click(screen.getByRole("button", { name: "Try again" }));
  await user.click(
    await screen.findByRole("button", { name: "Production networks" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Project unavailable",
  );
  expect(
    screen.getByRole("button", { name: "Production networks" }),
  ).toBeEnabled();
});

it("offers project creation from the empty state", async () => {
  invoke.mockResolvedValueOnce([]);
  const user = userEvent.setup();
  render(
    <ProjectIndexPage
      onNewConversation={vi.fn()}
      onOpenProject={vi.fn()}
      onResumeConversation={vi.fn()}
    />,
  );
  await user.click(
    await screen.findByRole("button", { name: "Create your first project" }),
  );
  expect(await screen.findByRole("dialog")).toBeVisible();
  expect(screen.getByRole("button", { name: "Create project" })).toBeDisabled();
});

it("clears an unmatched project search", async () => {
  const user = userEvent.setup();
  render(
    <ProjectIndexPage
      onNewConversation={vi.fn()}
      onOpenProject={vi.fn()}
      onResumeConversation={vi.fn()}
    />,
  );
  await screen.findByRole("button", { name: "Production networks" });
  await user.type(
    screen.getByRole("searchbox", { name: "Search projects" }),
    "missing",
  );
  expect(screen.getByRole("status")).toHaveTextContent("No projects match");
  await user.click(screen.getByRole("button", { name: "Clear search" }));
  expect(
    screen.getByRole("button", { name: "Production networks" }),
  ).toBeVisible();
});

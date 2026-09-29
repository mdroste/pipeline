import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
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

it("opens as an index and resumes the exact conversation only on request", async () => {
  const onOpenProject = vi.fn().mockResolvedValue(undefined);
  const onResumeConversation = vi.fn().mockResolvedValue(true);
  const user = userEvent.setup();
  render(<ProjectIndexPage {...{ onOpenProject, onResumeConversation }} />);
  await screen.findByRole("button", { name: "Production networks" });
  expect(onOpenProject).not.toHaveBeenCalled();
  expect(onResumeConversation).not.toHaveBeenCalled();
  expect(invoke).toHaveBeenCalledExactlyOnceWith("workbench_project_index");
  await user.click(
    screen.getByRole("button", { name: "Resume conversation →" }),
  );
  expect(onResumeConversation).toHaveBeenCalledWith("conversation-actual");
  await user.click(screen.getByRole("button", { name: "Production networks" }));
  expect(onOpenProject).toHaveBeenCalledWith("networks");
});

it("searches research questions and persists pins independently from project data", async () => {
  const user = userEvent.setup();
  const props = { onOpenProject: vi.fn(), onResumeConversation: vi.fn() };
  const view = render(<ProjectIndexPage {...props} />);
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
  render(<ProjectIndexPage {...props} />);
  expect(
    await screen.findByRole("button", { name: "Unpin Production networks" }),
  ).toHaveAttribute("aria-pressed", "true");
  expect(
    invoke.mock.calls.every(([name]) => name === "workbench_project_index"),
  ).toBe(true);
});

it("reports load and resume failures with retry and retains the project list", async () => {
  invoke.mockRejectedValueOnce(new Error("Store unavailable"));
  const user = userEvent.setup();
  render(
    <ProjectIndexPage
      onOpenProject={vi.fn()}
      onResumeConversation={vi
        .fn()
        .mockRejectedValue(new Error("Conversation unavailable"))}
    />,
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Store unavailable",
  );
  await user.click(screen.getByRole("button", { name: "Refresh" }));
  await user.click(
    await screen.findByRole("button", { name: "Resume conversation →" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Conversation unavailable",
  );
  expect(
    screen.getByRole("button", { name: "Production networks" }),
  ).toBeEnabled();
});

it("offers folderless project creation and routes attention to the owning project", async () => {
  invoke.mockResolvedValueOnce([]);
  const user = userEvent.setup();
  const open = vi.fn().mockResolvedValue(undefined);
  render(
    <ProjectIndexPage onOpenProject={open} onResumeConversation={vi.fn()} />,
  );
  expect(
    await screen.findByRole("button", { name: "Create your first project" }),
  ).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Refresh" }));
  await user.click(
    await screen.findByRole("button", { name: /2 suggested notes await/ }),
  );
  await waitFor(() =>
    expect(open).toHaveBeenCalledWith("networks", "overview"),
  );
});

import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { expect, it, vi } from "vitest";
import WorkspaceRailNavigation from "./WorkspaceRailNavigation";
import type { WorkbenchSession, Workspace } from "../lib/workbenchTypes";

const project = (id: string, name: string, extra: Partial<Workspace> = {}) =>
  ({
    id,
    name,
    root: `/papers/${id}`,
    archivedAt: null,
    ...extra,
  }) as Workspace;
const session = (id: string) =>
  ({ id, title: `Conversation ${id}`, archivedAt: null }) as WorkbenchSession;

function renderNavigation(
  overrides: Partial<ComponentProps<typeof WorkspaceRailNavigation>> = {},
) {
  const trade = project("trade", "Trade paper");
  const props: ComponentProps<typeof WorkspaceRailNavigation> = {
    project: trade,
    projects: [
      trade,
      project("wages", "Minimum wages"),
      project("old", "Old paper", { archivedAt: "2026-01-01" }),
    ],
    destination: "results",
    sessions: ["1", "2", "3", "4", "5"].map(session),
    sessionId: "1",
    chatOpen: false,
    background: null,
    disabled: false,
    onNavigate: vi.fn(),
    onSelectSession: vi.fn(),
    onCreateSession: vi.fn(),
    onSelectProject: vi.fn(),
    onNewProject: vi.fn(),
    onAllProjects: vi.fn(),
    conversationActions: () => ({
      onRename: vi.fn(),
      onGenerateTitle: vi.fn(),
      onMove: vi.fn(),
      onArchive: vi.fn(),
      onDelete: vi.fn(),
    }),
    ...overrides,
  };
  render(<WorkspaceRailNavigation {...props} />);
  return props;
}

it("shows the project's sections and returns to its overview from its name", async () => {
  const user = userEvent.setup();
  const props = renderNavigation();

  const sections = within(
    screen.getByRole("navigation", { name: "Project sections" }),
  );
  expect(sections.getByRole("button", { name: "Analyze" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  await user.click(screen.getByRole("button", { name: "Trade paper" }));
  expect(props.onNavigate).toHaveBeenCalledWith("overview");
});

it("lists a few conversations, keeps the open one, and shows the rest on request", async () => {
  const user = userEvent.setup();
  const props = renderNavigation({ sessionId: "5", chatOpen: true });
  const list = within(
    screen.getByRole("navigation", { name: "Conversations" }),
  );

  expect(list.getByRole("button", { name: "Conversation 5" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  expect(list.queryByRole("button", { name: "Conversation 4" })).toBeNull();

  await user.click(list.getByRole("button", { name: "Show all 5" }));
  await user.click(list.getByRole("button", { name: "Conversation 4" }));
  expect(props.onSelectSession).toHaveBeenCalledWith("4");
  await user.click(screen.getByRole("button", { name: "New conversation" }));
  expect(props.onCreateSession).toHaveBeenCalledOnce();
});

it("marks a conversation running elsewhere or waiting for approval", () => {
  renderNavigation({ background: { sessionId: "2", waiting: 1 } });
  expect(screen.getByLabelText("Needs your approval")).toBeVisible();
});

it("switches between active projects and reaches every project", async () => {
  const user = userEvent.setup();
  const props = renderNavigation();
  const open = () =>
    user.click(screen.getByRole("button", { name: "Switch project" }));

  await open();
  expect(screen.queryByRole("menuitemradio", { name: "Old paper" })).toBeNull();
  await user.click(
    screen.getByRole("menuitemradio", { name: "Minimum wages" }),
  );
  expect(props.onSelectProject).toHaveBeenCalledWith("wages");

  await open();
  await user.click(screen.getByRole("menuitem", { name: "All projects" }));
  expect(props.onAllProjects).toHaveBeenCalledOnce();

  await open();
  await user.click(screen.getByRole("menuitem", { name: "New project" }));
  expect(props.onNewProject).toHaveBeenCalledOnce();
});

it("lists unfiled conversations without project controls", () => {
  renderNavigation({ project: null, sessions: [] });
  expect(screen.getByText("No conversations yet.")).toBeVisible();
  expect(
    screen.queryByRole("navigation", { name: "Project sections" }),
  ).toBeNull();
  expect(screen.queryByRole("button", { name: "Switch project" })).toBeNull();
});

it("holds every control while an editor is busy", () => {
  renderNavigation({ disabled: true });
  for (const name of ["Trade paper", "Analyze", "Switch project"])
    expect(screen.getByRole("button", { name })).toBeDisabled();
});

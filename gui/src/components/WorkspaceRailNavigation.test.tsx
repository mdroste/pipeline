import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { expect, it, vi } from "vitest";
import WorkspaceRailNavigation from "./WorkspaceRailNavigation";
import type { Workspace } from "../lib/workbenchTypes";
import { saveProjectPins } from "../lib/projectIndex";
const project = (id: string, name: string, archivedAt: string | null = null) =>
  ({ id, name, root: "/research", updatedAt: id, archivedAt }) as Workspace;
function mount(
  overrides: Partial<ComponentProps<typeof WorkspaceRailNavigation>> = {},
) {
  const current = project("a", "Trade paper");
  const props = {
    project: current,
    projects: [
      current,
      project("b", "Minimum wages"),
      project("c", "Old paper", "archived"),
    ],
    chatOpen: false,
    disabled: false,
    destination: "theory" as const,
    onNavigate: vi.fn(),
    onSelectProject: vi.fn(),
    onNewProject: vi.fn(),
    onAllProjects: vi.fn(),
    onBrowseUnfiled: vi.fn(),
    ...overrides,
  };
  render(<WorkspaceRailNavigation {...props} />);
  return props;
}
it("searches pinned and recent projects without mixing in configuration", async () => {
  saveProjectPins(["b"]);
  const user = userEvent.setup();
  const props = mount();
  const button = screen.getByRole("button", {
    name: "Switch project: Trade paper",
  });
  await user.click(button);
  const dialog = within(screen.getByRole("dialog", { name: "Switch project" }));
  expect(dialog.getByRole("heading", { name: "Pinned" })).toBeVisible();
  expect(dialog.queryByText("Old paper")).toBeNull();
  expect(dialog.queryByText("Project settings")).toBeNull();
  await user.type(dialog.getByRole("searchbox"), "wage");
  expect(dialog.queryByText("Trade paper")).toBeNull();
  await user.click(dialog.getByRole("button", { name: "Minimum wages" }));
  expect(props.onSelectProject).toHaveBeenCalledWith("b");
  await user.click(button);
  await user.keyboard("{Escape}");
  expect(button).toHaveFocus();
  await user.click(screen.getByRole("button", { name: "Project settings" }));
  expect(props.onNavigate).toHaveBeenCalledWith("project-settings");
});
it("opens unfiled chats without inheriting project tools", async () => {
  const props = mount({ project: null });
  await userEvent.click(screen.getByRole("button", { name: "Chats" }));
  expect(props.onBrowseUnfiled).toHaveBeenCalledOnce();
  expect(
    screen.queryByRole("navigation", { name: "Project sections" }),
  ).toBeNull();
});
it("disables project changes while an editor is busy", () => {
  mount({ disabled: true });
  for (const b of screen.getAllByRole("button")) expect(b).toBeDisabled();
});

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import WorkspaceToolPicker from "./WorkspaceToolPicker";
import { saveWorkspacePins } from "../lib/workspaceNavigation";

beforeEach(() => localStorage.clear());

it("finds tools across sections and opens the keyboard selection without sending anything", async () => {
  const choose = vi.fn(), close = vi.fn();
  saveWorkspacePins("project", ["writing", "files"]);
  render(<WorkspaceToolPicker workspaceId="project" current="overview" onChoose={choose} onClose={close} />);
  expect(screen.getAllByRole("option").slice(0, 2).map(item => item.textContent)).toEqual(["ManuscriptPinned · Writing", "FilesPinned · Library"]);
  const search = screen.getByRole("combobox", { name: "Find a project tool" });
  await waitFor(() => expect(search).toHaveFocus());
  fireEvent.change(search, { target: { value: "execution" } });
  const options = screen.getAllByRole("option");
  expect(options).toHaveLength(2);
  expect(options[0]).toHaveAttribute("aria-selected", "true");
  fireEvent.keyDown(search, { key: "ArrowDown" });
  expect(options[1]).toHaveAttribute("aria-selected", "true");
  fireEvent.keyDown(search, { key: "Enter" });
  expect(choose).toHaveBeenCalledWith("execution");
  expect(close).toHaveBeenCalledOnce();
});

it("handles an empty search result and restores focus and background access on dismissal", async () => {
  const trigger = document.createElement("button");
  document.body.append(trigger); trigger.focus();
  const close = vi.fn();
  const view = render(<WorkspaceToolPicker workspaceId="project" current="overview" onChoose={vi.fn()} onClose={close} />);
  const search = screen.getByRole("combobox");
  await waitFor(() => expect(search).toHaveFocus());
  expect(trigger).toHaveAttribute("inert");
  fireEvent.change(search, { target: { value: "zzzz nonexistent" } });
  fireEvent.keyDown(search, { key: "ArrowDown" });
  fireEvent.keyDown(search, { key: "Enter" });
  expect(close).not.toHaveBeenCalled();
  expect(screen.getByText("No matching tools.")).toBeInTheDocument();
  fireEvent.keyDown(search, { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
  view.unmount();
  expect(trigger).not.toHaveAttribute("inert");
  expect(trigger).toHaveFocus();
  trigger.remove();
});

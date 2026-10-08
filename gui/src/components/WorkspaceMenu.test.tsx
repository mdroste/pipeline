import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import WorkspaceMenu from "./WorkspaceMenu";
import { MenuItem } from "../ui/Menu";

it("closes after an action or Escape and returns focus to the trigger", () => {
  const action = vi.fn();
  render(
    <WorkspaceMenu label="Conversation menu">
      <MenuItem onSelect={action}>Outline</MenuItem>
    </WorkspaceMenu>,
  );
  const trigger = screen.getByRole("button", { name: "Conversation menu" });
  fireEvent.click(trigger);
  expect(screen.getByRole("menuitem", { name: "Outline" })).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
  expect(action).not.toHaveBeenCalled();
  fireEvent.click(trigger);
  fireEvent.click(screen.getByRole("menuitem", { name: "Outline" }));
  expect(action).toHaveBeenCalledOnce();
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
});

it("lets focus leave a nonmodal menu without pulling it back", () => {
  render(
    <>
      <WorkspaceMenu label="Layout">
        <MenuItem>Reset</MenuItem>
      </WorkspaceMenu>
      <button>Message</button>
    </>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Layout" }));
  fireEvent.focusIn(screen.getByRole("button", { name: "Message" }));
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
});

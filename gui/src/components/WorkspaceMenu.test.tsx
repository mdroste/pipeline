import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import WorkspaceMenu from "./WorkspaceMenu";

it("closes after an action or Escape and returns focus to the trigger", () => {
  const action = vi.fn();
  render(
    <WorkspaceMenu label="Conversation menu">
      <button onClick={action}>Outline</button>
    </WorkspaceMenu>,
  );
  const trigger = screen.getByRole("button", { name: "Conversation menu" });
  fireEvent.click(trigger);
  expect(screen.getByRole("button", { name: "Outline" })).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  expect(screen.queryByRole("group")).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
  expect(action).not.toHaveBeenCalled();
  fireEvent.click(trigger);
  fireEvent.click(screen.getByRole("button", { name: "Outline" }));
  expect(action).toHaveBeenCalledOnce();
  expect(screen.queryByRole("group")).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
});

it("lets focus leave a nonmodal menu without pulling it back", () => {
  render(
    <>
      <WorkspaceMenu label="Layout">
        <button>Reset</button>
      </WorkspaceMenu>
      <button>Message</button>
    </>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Layout" }));
  fireEvent.focusIn(screen.getByRole("button", { name: "Message" }));
  expect(screen.queryByRole("group")).not.toBeInTheDocument();
});

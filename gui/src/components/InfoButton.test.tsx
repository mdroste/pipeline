import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import InfoButton from "./InfoButton";

it("keeps its help in sentence case inside an uppercase heading", () => {
  render(
    <div className="uppercase tracking-wider">
      <InfoButton label="Limits">Applies to each step.</InfoButton>
    </div>,
  );
  fireEvent.click(
    screen.getByRole("button", { name: "More information about Limits" }),
  );
  expect(screen.getByRole("dialog", { name: "Limits help" })).toHaveClass(
    "normal-case",
    "tracking-normal",
  );
});

it("keeps help within a native dialog and Escape dismisses only help", () => {
  const cancel = vi.fn();
  render(
    <dialog open onCancel={cancel} aria-label="Parent dialog">
      <InfoButton label="Limits">Applies to each step.</InfoButton>
    </dialog>,
  );
  const trigger = screen.getByRole("button", {
    name: "More information about Limits",
  });
  fireEvent.click(trigger);
  const help = screen.getByRole("dialog", { name: "Limits help" });
  expect(help.closest("dialog")).toBe(
    screen.getByRole("dialog", { name: "Parent dialog" }),
  );
  expect(help).toHaveFocus();
  fireEvent.keyDown(help, { key: "Escape" });
  expect(
    screen.queryByRole("dialog", { name: "Limits help" }),
  ).not.toBeInTheDocument();
  expect(screen.getByRole("dialog", { name: "Parent dialog" })).toBeVisible();
  expect(trigger).toHaveFocus();
  expect(cancel).not.toHaveBeenCalled();
});

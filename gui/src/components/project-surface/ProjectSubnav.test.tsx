import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import ProjectSubnav from "./ProjectSubnav";

it("shows a section's main views and keeps the rest under More", () => {
  const onSelect = vi.fn();
  render(<ProjectSubnav tab="files" onSelect={onSelect} />);
  expect(
    screen.getAllByRole("button").map((button) => button.textContent),
  ).toEqual([
    "Documents",
    "Files",
    "Version control",
    "Papers & sources",
    "More",
  ]);
  expect(screen.getByRole("button", { name: "Files" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  fireEvent.click(screen.getByRole("button", { name: "More" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Reading inbox" }));
  expect(onSelect).toHaveBeenCalledWith("acquisition");
});

it("shows the open view as a tab even when it normally sits under More", () => {
  render(<ProjectSubnav tab="exchange" onSelect={vi.fn()} />);
  expect(screen.getByRole("button", { name: "Share project" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  fireEvent.click(screen.getByRole("button", { name: "More" }));
  expect(screen.queryByRole("menuitem", { name: "Share project" })).toBeNull();
});

it("has no More menu when every view in the section is a tab", () => {
  render(<ProjectSubnav tab="tasks" onSelect={vi.fn()} />);
  expect(screen.queryByRole("button", { name: "More" })).toBeNull();
});

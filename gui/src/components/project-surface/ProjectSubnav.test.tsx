import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import ProjectSubnav from "./ProjectSubnav";

const labels = () =>
  screen.getAllByRole("button").map((button) => button.textContent);

it("shows every view in the section as a tab, with no menu", () => {
  const onSelect = vi.fn();
  render(<ProjectSubnav tab="edits" onSelect={onSelect} />);
  expect(labels()).toEqual(["Review edits", "Version control"]);
  expect(screen.getByRole("button", { name: "Review edits" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  fireEvent.click(screen.getByRole("button", { name: "Version control" }));
  expect(onSelect).toHaveBeenCalledWith("version-control");
});

it("offers Responses once the paper is past drafting", () => {
  const { rerender } = render(
    <ProjectSubnav tab="reviews" stage="drafting" onSelect={vi.fn()} />,
  );
  expect(labels()).toEqual(["Reviews & findings", "Send for review"]);
  rerender(<ProjectSubnav tab="reviews" stage="revising" onSelect={vi.fn()} />);
  expect(labels()).toEqual([
    "Reviews & findings",
    "Responses",
    "Send for review",
  ]);
  // A project with no stage hides nothing.
  rerender(<ProjectSubnav tab="reviews" onSelect={vi.fn()} />);
  expect(labels()).toContain("Responses");
  // The open view always keeps its tab.
  rerender(<ProjectSubnav tab="responses" stage="idea" onSelect={vi.fn()} />);
  expect(labels()).toContain("Responses");
});

it("groups project setup pages apart from the overview", () => {
  render(<ProjectSubnav tab="exchange" onSelect={vi.fn()} />);
  expect(labels()).toEqual([
    "Project settings",
    "Command settings",
    "Share project",
  ]);
});

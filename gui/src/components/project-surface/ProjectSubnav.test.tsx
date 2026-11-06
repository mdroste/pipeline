import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import ProjectSubnav from "./ProjectSubnav";
import ProjectTools from "./ProjectTools";
import {
  allProjectViews,
  workspaceDestinations,
} from "../../lib/workspaceNavigation";
it("keeps related choices fixed even on a specialist page", () => {
  const onSelect = vi.fn();
  const page = render(<ProjectSubnav tab="results" onSelect={onSelect} />);
  const tabs = () =>
    within(screen.getByRole("navigation", { name: "Analysis views" }))
      .getAllByRole("button")
      .map((b) => b.textContent);
  expect(tabs()).toEqual(["Results", "Data", "Experiments"]);
  fireEvent.click(screen.getByRole("button", { name: "Theory" }));
  expect(onSelect).toHaveBeenCalledWith("theory");
  page.rerender(<ProjectSubnav tab="theory" onSelect={onSelect} />);
  expect(tabs()).toEqual(["Results", "Data", "Experiments"]);
  expect(screen.getByText("Theory")).toBeVisible();
  expect(screen.queryByText("More")).toBeNull();
});
it("browses every project route and opens a specialist result by its original ID", () => {
  const select = vi.fn();
  render(<ProjectTools onSelect={select} />);
  for (const view of allProjectViews.filter((v) => v !== "project-tools")) {
    const button = screen
      .getByText(workspaceDestinations[view].label, { selector: "strong" })
      .closest("button")!;
    fireEvent.click(button);
    expect(select).toHaveBeenLastCalledWith(view);
  }
  fireEvent.change(screen.getByRole("searchbox"), {
    target: { value: "Symbols & assumptions" },
  });
  expect(screen.getAllByRole("button")).toHaveLength(1);
  expect(screen.getByText("Symbols & assumptions")).toBeVisible();
});

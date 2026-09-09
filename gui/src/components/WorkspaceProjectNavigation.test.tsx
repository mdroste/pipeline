import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import WorkspaceProjectNavigation from "./WorkspaceProjectNavigation";
import { loadWorkspacePins } from "../lib/workspaceNavigation";
beforeEach(() => localStorage.clear());
it("exposes six stable project areas and routes each to its default view", () => {
  const navigate = vi.fn();
  render(
    <WorkspaceProjectNavigation
      destination="theory"
      disabled={false}
      onNavigate={navigate}
    />,
  );
  const sections = within(
    screen.getByRole("navigation", { name: "Project sections" }),
  );
  expect(
    sections
      .getAllByRole("button")
      .map((button) => button.querySelector("span")?.textContent),
  ).toEqual([
    "Overview",
    "Library",
    "Analyze",
    "Write",
    "Automate",
    "Activity",
  ]);
  expect(sections.getByRole("button", { name: /Analyze/ })).toHaveAttribute(
    "aria-current",
    "page",
  );
  fireEvent.click(sections.getByRole("button", { name: /Activity/ }));
  expect(navigate).toHaveBeenCalledWith("memory");
});
it("rejects corrupt destinations, duplicates, and unsupported preference versions", () => {
  localStorage.setItem(
    "pipeline.workspace.pins.one",
    JSON.stringify({
      version: 1,
      pins: ["files", "files", "__proto__", "removed", "results"],
    }),
  );
  expect(loadWorkspacePins("one")).toEqual(["files", "results"]);
  localStorage.setItem(
    "pipeline.workspace.pins.one",
    JSON.stringify({ version: 999, pins: ["evidence"] }),
  );
  expect(loadWorkspacePins("one")).toEqual(["files", "writing", "results"]);
});

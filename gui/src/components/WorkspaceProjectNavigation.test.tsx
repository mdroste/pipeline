import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import WorkspaceProjectNavigation from "./WorkspaceProjectNavigation";
import { loadWorkspacePins } from "../lib/workspaceNavigation";
beforeEach(() => localStorage.clear());
it("exposes stable project destinations and routes each to its default view", () => {
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
    sections.getAllByRole("button").map((button) => button.textContent),
  ).toEqual([
    "Overview",
    "Chats",
    "Files",
    "Sources",
    "Analysis",
    "Manuscript",
    "Reviews & edits",
    "Notes",
    "Project tools",
    "Project settings",
  ]);
  expect(sections.getByRole("button", { name: "Analysis" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  fireEvent.click(sections.getByRole("button", { name: "Notes" }));
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

import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import WorkspaceProjectNavigation from "./WorkspaceProjectNavigation";
import { loadWorkspacePins } from "../lib/workspaceNavigation";
beforeEach(() => localStorage.clear());
it("persists pin order per project and resets only the current project's preferences", () => {
  const reset = vi.fn(); const navigate = vi.fn();
  render(<WorkspaceProjectNavigation workspaceId="one" destination="overview" disabled={false} onNavigate={navigate} onResetLayout={reset} />);
  fireEvent.click(screen.getByRole("button", { name: "Customize" }));
  fireEvent.click(screen.getByRole("button", { name: "Move Results up" }));
  fireEvent.change(screen.getByLabelText("Pin a project tool"), { target: { value: "evidence" } });
  expect(loadWorkspacePins("one")).toEqual(["files", "results", "writing", "evidence"]);
  expect(loadWorkspacePins("two")).toEqual(["files", "writing", "results"]);
  expect(screen.getByLabelText("Pin a project tool")).toBeDisabled();
  fireEvent.click(within(screen.getByRole("navigation", { name: "Pinned project tools" })).getByRole("button", { name: "Claims & evidence" }));
  expect(navigate).toHaveBeenCalledWith("evidence");
  fireEvent.click(screen.getByRole("button", { name: "Reset layout and shortcuts" }));
  expect(loadWorkspacePins("one")).toEqual(["files", "writing", "results"]);
  expect(reset).toHaveBeenCalledOnce();
});
it("rejects corrupt destinations, duplicates, and unsupported preference versions", () => {
  localStorage.setItem("pipeline.workspace.pins.one", JSON.stringify({ version: 1, pins: ["files", "files", "__proto__", "removed", "results"] }));
  expect(loadWorkspacePins("one")).toEqual(["files", "results"]);
  localStorage.setItem("pipeline.workspace.pins.one", JSON.stringify({ version: 999, pins: ["evidence"] }));
  expect(loadWorkspacePins("one")).toEqual(["files", "writing", "results"]);
});

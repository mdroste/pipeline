import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import WorkspaceComposerControls from "./WorkspaceComposerControls";
import type { WorkspaceModel } from "../lib/workbenchTypes";

const models: WorkspaceModel[] = [{ id: "example", model: "example", displayName: "Example", description: "", isDefault: true,
  defaultReasoningEffort: "medium", supportedReasoningEfforts: [{ reasoningEffort: "medium", description: "Medium" }, { reasoningEffort: "high", description: "High" }] }];
it("offers only supported efforts and resets effort when changing model", () => {
  const change = vi.fn();
  const view = render(<WorkspaceComposerControls models={models} model="" effort="" disabled={false} onChange={change} />);
  expect(screen.getByLabelText("Thinking")).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Model"), { target: { value: "example" } });
  expect(change).toHaveBeenLastCalledWith("example", "");
  view.rerender(<WorkspaceComposerControls models={models} model="example" effort="" disabled={false} onChange={change} />);
  const thinking = screen.getByLabelText("Thinking");
  expect(within(thinking).getAllByRole("option").map(option => option.textContent)).toEqual(["Default thinking · Medium", "Medium", "High"]);
  fireEvent.change(thinking, { target: { value: "high" } });
  expect(change).toHaveBeenLastCalledWith("example", "high");
});
it("shows saved unavailable settings without silently selecting replacements", () => {
  const change = vi.fn();
  render(<WorkspaceComposerControls models={models} model="removed-model" effort="ultra" disabled={false} onChange={change} />);
  expect(screen.getByLabelText("Model")).toHaveValue("removed-model");
  expect(screen.getByLabelText("Thinking")).toHaveValue("ultra");
  expect(screen.getByLabelText("Thinking")).toBeDisabled();
  expect(change).not.toHaveBeenCalled();
});

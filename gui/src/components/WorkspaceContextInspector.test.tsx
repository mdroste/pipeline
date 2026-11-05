import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import WorkspaceContextInspector from "./WorkspaceContextInspector";

vi.mock("../lib/workbench/harnessClient", () => ({
  harnessClient: {
    effectiveHarness: () =>
      Promise.resolve({
        preset: { name: "Research assistant", instructions: "" },
        contextPreview: "",
      }),
  },
}));

it("points an empty context at the control that adds material", async () => {
  render(
    <WorkspaceContextInspector
      sessionId="conversation"
      workspaceName={null}
      onClose={() => {}}
      onOpenSettings={() => {}}
    />,
  );
  const empty = await screen.findByText(/No project material is attached/);
  expect(empty).toHaveTextContent("+ (Add to message)");
  expect(empty).not.toHaveTextContent("Sources");
});

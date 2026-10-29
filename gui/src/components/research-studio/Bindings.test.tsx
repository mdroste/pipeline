import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import Bindings from "./Bindings";
import type { ProjectHome } from "../../lib/projectClient";

const mocks = vi.hoisted(() => ({
  coverage: vi.fn(),
  mutate: vi.fn(),
  listStructuredResults: vi.fn(),
}));
vi.mock("../../lib/studioClient", async (original) => ({
  ...(await original<typeof import("../../lib/studioClient")>()),
  studioClient: { coverage: mocks.coverage, mutate: mocks.mutate },
}));
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: { listStructuredResults: mocks.listStructuredResults },
}));

const data = {
  changes: [],
  anchors: [
    {
      id: "a1",
      body: {
        selection: { revisionId: "rev", quote: "an effect of 0.42" },
        body: "an effect of 0.42",
      },
    },
  ],
} as unknown as ProjectHome;

it("updates the saved binding when it is saved again", async () => {
  mocks.coverage.mockResolvedValue([]);
  mocks.listStructuredResults.mockResolvedValue([
    { sourceExecutionId: "exec-1", resultId: "effect", units: "pp" },
  ]);
  mocks.mutate.mockResolvedValue({ id: "b1", revision: 1 });
  render(
    <Bindings
      workspaceId="ws"
      data={data}
      onRefresh={async () => {}}
      onDocument={() => {}}
    />,
  );
  const result = screen.getByLabelText("Saved result");
  await waitFor(() =>
    expect(result.querySelectorAll("option").length).toBeGreaterThan(0),
  );
  fireEvent.change(result, { target: { value: "exec-1/effect" } });
  fireEvent.change(screen.getByLabelText("Exact manuscript selection"), {
    target: { value: "a1" },
  });
  const save = screen.getByText("Save numeric binding");
  fireEvent.click(save);
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenLastCalledWith(
      "ws",
      expect.objectContaining({ action: "bindNumber", id: null }),
    ),
  );
  await waitFor(() => expect(save).not.toBeDisabled());
  fireEvent.click(save);
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenLastCalledWith(
      "ws",
      expect.objectContaining({ action: "bindNumber", id: "b1" }),
    ),
  );
});

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ManuscriptBuildReview from "./ManuscriptBuildReview";
import type { ExecutionProfile } from "../../lib/workbench/executionTypes";
const mocks = vi.hoisted(() => ({
  previewExecution: vi.fn(),
  authorizeExecution: vi.fn(),
  runExecution: vi.fn(),
}));
vi.mock("../../lib/projectClient", () => ({ projectClient: mocks }));
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: mocks,
}));
const profile = {
  id: "profile",
  revision: 1,
  testStatus: null,
} as ExecutionProfile;
beforeEach(() => {
  vi.resetAllMocks();
  mocks.previewExecution.mockResolvedValue({
    command: ["latexmk", "main.tex"],
    cwd: "/paper",
    fingerprint: "reviewed-input-bytes",
    inputs: {},
    launch: {},
    outputs: ["main.pdf"],
    boundary: "Exact command authorization",
  });
  mocks.authorizeExecution.mockResolvedValue({});
  mocks.runExecution.mockResolvedValue({ id: "build" });
});
it("reviews automatically but runs only after exact-input authorization", async () => {
  const queued = vi.fn();
  render(<ManuscriptBuildReview profile={profile} onQueued={queued} />);
  const start = await screen.findByRole("button", {
    name: "Authorize and build",
  });
  expect(mocks.runExecution).not.toHaveBeenCalled();
  fireEvent.click(start);
  await waitFor(() => expect(queued).toHaveBeenCalledWith({ id: "build" }));
  expect(mocks.authorizeExecution).toHaveBeenCalledWith(
    "profile",
    "reviewed-input-bytes",
  );
  expect(mocks.runExecution).toHaveBeenCalledWith(
    expect.objectContaining({ profileId: "profile", testOnly: true }),
  );
});
it("does not start a build when inputs changed after review", async () => {
  mocks.authorizeExecution.mockRejectedValue({
    message: "Inputs changed. Review again.",
  });
  render(<ManuscriptBuildReview profile={profile} onQueued={vi.fn()} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Authorize and build" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent("Inputs changed");
  expect(mocks.runExecution).not.toHaveBeenCalled();
});

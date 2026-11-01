import { render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";

import RunOutput from "./RunOutput";

const output = vi.hoisted(() => vi.fn());
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: { executionOutput: output },
}));
beforeEach(() => output.mockReset());

it("loads standard output alongside non-empty errors and warnings", async () => {
  output.mockResolvedValue({
    stdout: "coefficient 0.42",
    stderr: "Warning: package is old",
  });
  render(
    <RunOutput
      workspaceId="project"
      run={{
        id: "run",
        outcome: "completed",
        validation: { stdoutTruncated: false, stderrTruncated: true },
      }}
    />,
  );
  expect(await screen.findByLabelText("Standard output")).toHaveTextContent(
    "coefficient 0.42",
  );
  expect(output).toHaveBeenCalledWith("project", "run");
  expect(screen.getByLabelText("Errors and warnings")).toHaveTextContent(
    "Warning: package is old",
  );
  expect(
    screen.getAllByText("Only the first 4 MiB of this output was saved."),
  ).toHaveLength(1);
});

it("reports a run without captured output", async () => {
  output.mockResolvedValue({ stdout: "", stderr: null });
  render(
    <RunOutput
      workspaceId="project"
      run={{ id: "run", outcome: "failed", validation: {} }}
    />,
  );
  expect(await screen.findByText("No captured output")).toBeInTheDocument();
  expect(screen.queryByLabelText("Standard output")).not.toBeInTheDocument();
});

it("loads a run's output again when the run ends", async () => {
  output.mockResolvedValue({ stdout: "first", stderr: null });
  const run = { id: "run", validation: {} };
  const page = render(
    <RunOutput workspaceId="project" run={{ ...run, outcome: "running" }} />,
  );
  await screen.findByLabelText("Standard output");
  page.rerender(
    <RunOutput workspaceId="project" run={{ ...run, outcome: "completed" }} />,
  );
  expect(output).toHaveBeenCalledTimes(2);
});

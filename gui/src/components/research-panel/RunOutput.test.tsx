import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";

import RunOutput from "./RunOutput";

it("shows standard output alongside non-empty errors and warnings", () => {
  render(
    <RunOutput
      run={{
        stdout: "coefficient 0.42",
        stderr: "Warning: package is old",
        validation: { stdoutTruncated: false, stderrTruncated: true },
      }}
    />,
  );
  expect(screen.getByLabelText("Standard output")).toHaveTextContent(
    "coefficient 0.42",
  );
  expect(screen.getByLabelText("Errors and warnings")).toHaveTextContent(
    "Warning: package is old",
  );
  expect(
    screen.getAllByText("Only the first 4 MiB of this output was saved."),
  ).toHaveLength(1);
});

it("reports a run without captured output", () => {
  render(<RunOutput run={{ stdout: "", stderr: null, validation: {} }} />);
  expect(screen.getByText("No captured output")).toBeInTheDocument();
  expect(screen.queryByLabelText("Standard output")).not.toBeInTheDocument();
});

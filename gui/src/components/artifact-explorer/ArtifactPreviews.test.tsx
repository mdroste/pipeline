import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { BinaryCard, CsvView } from "./ArtifactPreviews";

const content = (kind: string) => ({
  kind,
  bytes: 1024,
  text: null,
  base64: null,
  truncated: false,
  abs_path: "/runs/x/artifacts/summary.jar",
});

describe("BinaryCard", () => {
  it("never offers to launch a model-written program", () => {
    render(
      <BinaryCard runId="run" entry={null} content={content("executable")} />,
    );
    expect(
      screen.queryByRole("button", { name: "Open in system viewer" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText(/Pipeline will not run it from this preview/),
    ).toBeVisible();
  });

  it("still opens ordinary documents in the system viewer", () => {
    render(<BinaryCard runId="run" entry={null} content={content("binary")} />);
    expect(
      screen.getByRole("button", { name: "Open in system viewer" }),
    ).toBeVisible();
  });
});

describe("CsvView", () => {
  it("shows the text beneath the notice for a table too wide to parse", () => {
    const wide = Array.from({ length: 70 }, (_, index) => `col${index}`).join(
      ",",
    );
    render(<CsvView text={wide} relPath="data/wide.csv" />);
    expect(screen.getByText(/exceeds 60 columns/)).toBeVisible();
    expect(screen.getByText(/col69/)).toBeVisible();
  });

  it("shows the text beneath the notice for a stray quote", () => {
    render(<CsvView text={'size\n5" sample'} relPath="data/a.csv" />);
    expect(screen.getByText(/Malformed quoted field/)).toBeVisible();
    expect(screen.getByText(/5" sample/)).toBeVisible();
  });
});

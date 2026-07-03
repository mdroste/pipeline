import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import PipelineProgress from "./PipelineProgress";

describe("PipelineProgress", () => {
  it("renders the base stages without the merge stage by default", () => {
    render(<PipelineProgress state={{ kind: "extracting" }} />);
    expect(screen.getByText("Extract text")).toBeInTheDocument();
    expect(screen.getByText("Run parallel steps")).toBeInTheDocument();
    expect(screen.queryByText("Merge cross-agent reports")).not.toBeInTheDocument();
  });

  it("shows the merge stage while merging", () => {
    render(<PipelineProgress state={{ kind: "merging", passes: {} }} />);
    expect(screen.getByText("Merge cross-agent reports")).toBeInTheDocument();
  });

  it("formats pass keys into readable labels during dispatch", () => {
    render(
      <PipelineProgress
        state={{
          kind: "dispatching",
          passes: {
            "technical/claude": "running",
            empirical: "done",
          },
        }}
      />,
    );
    expect(screen.getByText("Technical (Claude)")).toBeInTheDocument();
    expect(screen.getByText("Empirical")).toBeInTheDocument();
    expect(screen.getByText("running")).toBeInTheDocument();
    expect(screen.getByText("done")).toBeInTheDocument();
  });

  it("formats merge pass keys", () => {
    render(
      <PipelineProgress
        state={{ kind: "merging", passes: { "merge/technical": "running" } }}
      />,
    );
    expect(screen.getByText("Merge: Technical")).toBeInTheDocument();
  });

  it("marks the failed stage on error", () => {
    render(
      <PipelineProgress
        state={{ kind: "error", message: "boom", failedAt: "extracting" }}
      />,
    );
    expect(screen.getByText("failed")).toBeInTheDocument();
    expect(screen.getByText("Extract text")).toBeInTheDocument();
  });

  it("labels a cancelled run as cancelled rather than failed", () => {
    render(
      <PipelineProgress
        state={{
          kind: "error",
          message: "Pipeline cancelled",
          failedAt: "dispatching",
        }}
      />,
    );
    expect(screen.getByText("cancelled")).toBeInTheDocument();
    expect(screen.queryByText("failed")).not.toBeInTheDocument();
  });
});

import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import PipelineProgress from "./PipelineProgress";
import type { ExecutionPlanStage } from "../lib/pipelineHelpers";
import type { RuntimeStage } from "../hooks/usePipeline";

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

  it("uses release-contrast text for elapsed status metadata", () => {
    render(
      <PipelineProgress
        state={{ kind: "extracting" }}
        runStartedAt={Date.now()}
      />,
    );
    const elapsed = screen.getByText("0s");
    expect(elapsed).toHaveClass("text-gray-500", "dark:text-gray-400");
    expect(elapsed).not.toHaveClass("text-gray-400", "dark:text-gray-600");
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

  it("retains completed passes across alternating parallel and sequential waves", () => {
    const plan: ExecutionPlanStage[] = [
      { id: "extract", kind: "extracting", label: "Extract input", stepIds: [] },
      { id: "p1", kind: "dispatching", label: "Parallel wave 1", stepIds: ["first"] },
      { id: "s1", kind: "synthesizing", label: "Synthesis", stepIds: ["middle"] },
      { id: "p2", kind: "dispatching", label: "Parallel wave 2", stepIds: ["last"] },
      { id: "done", kind: "done", label: "Complete", stepIds: [] },
    ];
    const stageHistory: RuntimeStage[] = [
      { id: "extract", kind: "extracting", label: "Extract input", stepIds: [], status: "done", passes: {} },
      { id: "p1", kind: "dispatching", label: "Parallel wave 1", stepIds: ["first"], status: "done", passes: { first: "done" } },
      { id: "s1", kind: "synthesizing", label: "Synthesis", stepIds: ["middle"], status: "done", passes: { middle: "done" } },
      { id: "p2", kind: "dispatching", label: "Parallel wave 2", stepIds: ["last"], status: "active", passes: { last: "running" } },
    ];
    render(
      <PipelineProgress
        state={{ kind: "dispatching", passes: { last: "running" } }}
        plan={plan}
        stageHistory={stageHistory}
      />,
    );

    expect(screen.getByText("Parallel wave 1")).toBeInTheDocument();
    expect(screen.getByText("Parallel wave 2")).toBeInTheDocument();
    expect(screen.getByText("First")).toBeInTheDocument();
    expect(screen.getByText("Middle")).toBeInTheDocument();
    expect(screen.getByText("Last")).toBeInTheDocument();
    expect(screen.getByText("Parallel wave 1").closest("[data-status]")).toHaveAttribute("data-status", "done");
    expect(screen.getByText("Parallel wave 2").closest("[data-status]")).toHaveAttribute("data-status", "active");
  });

  it("uses stable IDs when adjacent planned stages have the same kind", () => {
    const plan: ExecutionPlanStage[] = [
      { id: "extract", kind: "extracting", label: "Extract input", stepIds: [] },
      { id: "guarded", kind: "dispatching", label: "Conditional wave", stepIds: ["guarded"] },
      { id: "later", kind: "dispatching", label: "Later wave", stepIds: ["later"] },
      { id: "done", kind: "done", label: "Complete", stepIds: [] },
    ];
    render(
      <PipelineProgress
        state={{ kind: "dispatching", passes: {} }}
        plan={plan}
        stageHistory={[
          { id: "extract", kind: "extracting", label: "Extract input", stepIds: [], status: "done", passes: {} },
          { id: "guarded", kind: "dispatching", label: "Conditional wave", stepIds: ["guarded"], status: "done", passes: {} },
          { id: "later", kind: "dispatching", label: "Later wave", stepIds: ["later"], status: "active", passes: {} },
        ]}
      />,
    );
    expect(screen.getByText("Conditional wave").closest("[data-status]")).toHaveAttribute("data-status", "done");
    expect(screen.getByText("Later wave").closest("[data-status]")).toHaveAttribute("data-status", "active");
  });

  it("shows an explicitly skipped planned stage without shifting a later same-kind stage", () => {
    const plan: ExecutionPlanStage[] = [
      { id: "merge-1", kind: "merging", label: "Merge wave 1", stepIds: ["first"] },
      { id: "merge-2", kind: "merging", label: "Merge wave 2", stepIds: ["second"] },
      { id: "done", kind: "done", label: "Complete", stepIds: [] },
    ];
    render(
      <PipelineProgress
        state={{ kind: "merging", passes: {} }}
        plan={plan}
        stageHistory={[
          { id: "merge-1", kind: "merging", label: "Merge wave 1", stepIds: ["first"], status: "skipped", passes: {} },
          { id: "merge-2", kind: "merging", label: "Merge wave 2", stepIds: ["second"], status: "active", passes: {} },
        ]}
      />,
    );
    expect(screen.getByText("Merge wave 1").closest("[data-status]")).toHaveAttribute("data-status", "skipped");
    expect(screen.getByText("Merge wave 2").closest("[data-status]")).toHaveAttribute("data-status", "active");
  });
});

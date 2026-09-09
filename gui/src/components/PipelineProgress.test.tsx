import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import PipelineProgress from "./PipelineProgress";
import type { ExecutionPlanStage } from "../lib/pipelineHelpers";
import type { RuntimeStage } from "../hooks/usePipeline";

const autoReviewPlan: ExecutionPlanStage[] = [
  {
    id: "extract",
    kind: "extracting",
    label: "Creating document bundle",
    stepIds: [],
  },
  {
    id: "orient",
    kind: "orienting",
    label: "Creating orientation map & review plan",
    stepIds: [],
  },
  {
    id: "reviews",
    kind: "dispatching",
    label: "Parallel agent wave",
    stepIds: ["contribution", "claims", "exposition"],
    stepLabels: [
      "Contribution & Literature",
      "Claims & Consistency",
      "Exposition & Argument",
    ],
  },
  {
    id: "review-merges",
    kind: "merging",
    label: "Merge parallel wave 1",
    stepIds: ["contribution", "claims", "exposition"],
    stepLabels: [
      "Contribution & Literature",
      "Claims & Consistency",
      "Exposition & Argument",
    ],
  },
  {
    id: "synthesis",
    kind: "synthesizing",
    label: "Sequential agent wave",
    stepIds: ["auto_synthesis"],
    stepLabels: ["Auto Review Synthesis"],
  },
  { id: "done", kind: "done", label: "Complete", stepIds: [] },
];

const autoReviewRouting = {
  primaryDomain: "Mathematics",
  subject: "Algebraic geometry",
  subjectIds: ["subject_mathematics_algebraic_geometry"],
  subjectLabels: ["Mathematics — Algebraic Geometry"],
  methodIds: ["formal_proofs"],
  methodLabels: ["Method — Formal Proofs"],
  specialistIds: ["subject_mathematics_algebraic_geometry", "formal_proofs"],
  specialistLabels: [
    "Mathematics — Algebraic Geometry",
    "Method — Formal Proofs",
  ],
};

describe("PipelineProgress", () => {
  it("renders the base stages without the merge stage by default", () => {
    render(<PipelineProgress state={{ kind: "extracting" }} />);
    expect(screen.getByText("Processing inputs")).toBeInTheDocument();
    expect(screen.getByText("Creating document bundle")).toBeInTheDocument();
    expect(screen.getByText("Creating orientation map")).toBeInTheDocument();
    expect(screen.getByText("Parallel agent wave")).toBeInTheDocument();
    expect(screen.getByText("Sequential agent wave")).toBeInTheDocument();
    expect(
      screen.queryByText("Merge cross-agent reports"),
    ).not.toBeInTheDocument();
  });

  it("supports legacy plans captured before orientation became required", () => {
    render(
      <PipelineProgress
        state={{ kind: "extracting" }}
        plan={[
          {
            id: "extracting",
            kind: "extracting",
            label: "Creating source-tree inventory",
            stepIds: [],
          },
          { id: "done", kind: "done", label: "Complete", stepIds: [] },
        ]}
      />,
    );
    expect(screen.getByText("Processing inputs")).toBeInTheDocument();
    expect(
      screen.getByText("Creating source-tree inventory"),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("Creating orientation map"),
    ).not.toBeInTheDocument();
  });

  it("preserves the combined Auto Review orientation label", () => {
    render(
      <PipelineProgress
        state={{ kind: "orienting" }}
        plan={[
          {
            id: "extracting",
            kind: "extracting",
            label: "Creating document bundle",
            stepIds: [],
          },
          {
            id: "orienting",
            kind: "orienting",
            label: "Creating orientation map & review plan",
            stepIds: [],
          },
          { id: "done", kind: "done", label: "Complete", stepIds: [] },
        ]}
      />,
    );
    expect(
      screen.getByText("Creating orientation map & review plan"),
    ).toBeInTheDocument();
  });

  it("shows routed provider units and merge targets for adaptive specialists", () => {
    const { rerender } = render(
      <PipelineProgress state={{ kind: "orienting" }} plan={autoReviewPlan} />,
    );

    let parallelWave = screen
      .getByText("Parallel agent wave")
      .closest("[data-status]");
    expect(parallelWave).toHaveTextContent("Adaptive agents");
    expect(parallelWave).toHaveTextContent("pending");

    rerender(
      <PipelineProgress
        state={{ kind: "orienting" }}
        plan={autoReviewPlan}
        reviewRouting={autoReviewRouting}
      />,
    );
    parallelWave = screen
      .getByText("Parallel agent wave")
      .closest("[data-status]");
    expect(parallelWave).not.toHaveTextContent("Adaptive agents");
    expect(parallelWave).toHaveTextContent("Mathematics — Algebraic Geometry");
    expect(parallelWave).toHaveTextContent("Method — Formal Proofs");

    const stageHistory: RuntimeStage[] = [
      {
        id: "reviews",
        kind: "dispatching",
        label: "Parallel agent wave",
        stepIds: [
          "contribution",
          "claims",
          "exposition",
          "subject_mathematics_algebraic_geometry",
          "formal_proofs",
        ],
        stepLabels: [
          "Contribution & Literature",
          "Claims & Consistency",
          "Exposition & Argument",
          "Mathematics — Algebraic Geometry",
          "Method — Formal Proofs",
        ],
        mergeStepIds: [
          "contribution",
          "claims",
          "exposition",
          "subject_mathematics_algebraic_geometry",
          "formal_proofs",
        ],
        mergeStepLabels: [
          "Contribution & Literature",
          "Claims & Consistency",
          "Exposition & Argument",
          "Mathematics — Algebraic Geometry",
          "Method — Formal Proofs",
        ],
        status: "active",
        passes: {
          "subject_mathematics_algebraic_geometry/claude": "pending",
          "subject_mathematics_algebraic_geometry/codex": "pending",
          "formal_proofs/claude": "pending",
          "formal_proofs/codex": "pending",
        },
      },
    ];
    rerender(
      <PipelineProgress
        state={{ kind: "dispatching", passes: {} }}
        plan={autoReviewPlan}
        stageHistory={stageHistory}
        reviewRouting={autoReviewRouting}
      />,
    );
    parallelWave = screen
      .getByText("Parallel agent wave")
      .closest("[data-status]");
    expect(parallelWave).toHaveTextContent(
      "Mathematics — Algebraic Geometry (Claude)",
    );
    expect(parallelWave).toHaveTextContent(
      "Mathematics — Algebraic Geometry (Codex)",
    );
    expect(parallelWave).toHaveTextContent("Method — Formal Proofs (Claude)");
    expect(parallelWave).toHaveTextContent("Method — Formal Proofs (Codex)");

    const mergeWave = screen
      .getByText("Merge parallel wave 1")
      .closest("[data-status]");
    expect(mergeWave).toHaveTextContent(
      "Merge: Mathematics — Algebraic Geometry",
    );
    expect(mergeWave).toHaveTextContent("Merge: Method — Formal Proofs");
  });

  it("shows the merge stage while merging", () => {
    render(<PipelineProgress state={{ kind: "merging", passes: {} }} />);
    expect(screen.getByText("Merge cross-agent reports")).toBeInTheDocument();
  });

  it("shows the auto-detected subject and selected specialists", () => {
    render(
      <PipelineProgress
        state={{ kind: "dispatching", passes: {} }}
        reviewRouting={{
          primaryDomain: "Economics",
          subject: "Quantitative macroeconomics",
          subjectIds: ["subject_economics_macro"],
          subjectLabels: ["Economics — Macroeconomics"],
          methodIds: ["formal_proofs"],
          methodLabels: ["Method — Formal Proofs"],
          specialistIds: ["subject_economics_macro", "formal_proofs"],
          specialistLabels: [
            "Economics — Macroeconomics",
            "Method — Formal Proofs",
          ],
        }}
      />,
    );
    expect(screen.getByText("Auto-detected review")).toBeInTheDocument();
    expect(
      screen.getByText("Economics · Quantitative macroeconomics"),
    ).toBeInTheDocument();
    expect(screen.getByText(/Subject:/).closest("p")).toHaveTextContent(
      "Subject: Economics — Macroeconomics",
    );
    expect(screen.getByText(/Methods:/).closest("p")).toHaveTextContent(
      "Methods: Method — Formal Proofs",
    );
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
    expect(screen.getAllByText("done").length).toBeGreaterThan(0);
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
    expect(screen.getAllByText("failed").length).toBeGreaterThan(0);
    expect(screen.getByText("Processing inputs")).toBeInTheDocument();
    expect(screen.getByText("Creating document bundle")).toBeInTheDocument();
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

  it("groups adjacent sequential steps under one progress stage", () => {
    const plan: ExecutionPlanStage[] = [
      {
        id: "extract",
        kind: "extracting",
        label: "Creating document bundle",
        stepIds: [],
      },
      {
        id: "consolidate",
        kind: "synthesizing",
        label: "Sequential agent wave",
        stepIds: ["consolidate"],
        stepLabels: ["Consolidate Feedback"],
      },
      {
        id: "validate",
        kind: "synthesizing",
        label: "Sequential agent wave",
        stepIds: ["validate"],
        stepLabels: ["Validate Feedback"],
      },
      { id: "done", kind: "done", label: "Complete", stepIds: [] },
    ];
    render(
      <PipelineProgress
        state={{ kind: "synthesizing", passes: { validate: "running" } }}
        plan={plan}
        stageHistory={[
          {
            id: "extract",
            kind: "extracting",
            label: "Creating document bundle",
            stepIds: [],
            status: "done",
            passes: {},
          },
          {
            id: "consolidate",
            kind: "synthesizing",
            label: "Sequential agent wave",
            stepIds: ["consolidate"],
            stepLabels: ["Consolidate Feedback"],
            status: "done",
            passes: { consolidate: "done" },
          },
          {
            id: "validate",
            kind: "synthesizing",
            label: "Sequential agent wave",
            stepIds: ["validate"],
            stepLabels: ["Validate Feedback"],
            status: "active",
            passes: { validate: "running" },
          },
        ]}
      />,
    );

    expect(screen.getAllByText("Sequential agent wave")).toHaveLength(1);
    const sequentialStage = screen
      .getByText("Sequential agent wave")
      .closest("[data-status]");
    expect(sequentialStage).toHaveAttribute("data-status", "active");
    expect(sequentialStage).toHaveTextContent("Consolidate Feedback");
    expect(sequentialStage).toHaveTextContent("Validate Feedback");
    expect(
      screen.getByText("Consolidate Feedback").parentElement,
    ).toHaveTextContent("done");
    expect(
      screen.getByText("Validate Feedback").parentElement,
    ).toHaveTextContent("running");
  });

  it("retains completed passes across alternating parallel and sequential waves", () => {
    const plan: ExecutionPlanStage[] = [
      {
        id: "extract",
        kind: "extracting",
        label: "Creating document bundle",
        stepIds: [],
      },
      {
        id: "p1",
        kind: "dispatching",
        label: "Parallel agent wave",
        stepIds: ["first"],
        stepLabels: ["First review"],
      },
      {
        id: "s1",
        kind: "synthesizing",
        label: "Sequential agent wave",
        stepIds: ["middle"],
        stepLabels: ["Editorial synthesis"],
      },
      {
        id: "p2",
        kind: "dispatching",
        label: "Parallel agent wave",
        stepIds: ["last"],
        stepLabels: ["Final check"],
      },
      { id: "done", kind: "done", label: "Complete", stepIds: [] },
    ];
    const stageHistory: RuntimeStage[] = [
      {
        id: "extract",
        kind: "extracting",
        label: "Creating document bundle",
        stepIds: [],
        status: "done",
        passes: {},
      },
      {
        id: "p1",
        kind: "dispatching",
        label: "Parallel agent wave",
        stepIds: ["first"],
        stepLabels: ["First review"],
        status: "done",
        passes: { first: "done" },
      },
      {
        id: "s1",
        kind: "synthesizing",
        label: "Sequential agent wave",
        stepIds: ["middle"],
        stepLabels: ["Editorial synthesis"],
        status: "done",
        passes: { middle: "done" },
      },
      {
        id: "p2",
        kind: "dispatching",
        label: "Parallel agent wave",
        stepIds: ["last"],
        stepLabels: ["Final check"],
        status: "active",
        passes: { last: "running" },
      },
    ];
    render(
      <PipelineProgress
        state={{ kind: "dispatching", passes: { last: "running" } }}
        plan={plan}
        stageHistory={stageHistory}
      />,
    );

    expect(screen.getAllByText("Parallel agent wave")).toHaveLength(2);
    expect(screen.getByText("Sequential agent wave")).toBeInTheDocument();
    expect(screen.getByText("First review")).toBeInTheDocument();
    expect(screen.getByText("Editorial synthesis")).toBeInTheDocument();
    expect(screen.getByText("Final check")).toBeInTheDocument();
    expect(
      screen.getByText("First review").closest("[data-status]"),
    ).toHaveAttribute("data-status", "done");
    expect(
      screen.getByText("Final check").closest("[data-status]"),
    ).toHaveAttribute("data-status", "active");
  });

  it("uses stable IDs when adjacent planned stages have the same kind", () => {
    const plan: ExecutionPlanStage[] = [
      {
        id: "extract",
        kind: "extracting",
        label: "Extract input",
        stepIds: [],
      },
      {
        id: "guarded",
        kind: "dispatching",
        label: "Conditional wave",
        stepIds: ["guarded"],
      },
      {
        id: "later",
        kind: "dispatching",
        label: "Later wave",
        stepIds: ["later"],
      },
      { id: "done", kind: "done", label: "Complete", stepIds: [] },
    ];
    render(
      <PipelineProgress
        state={{ kind: "dispatching", passes: {} }}
        plan={plan}
        stageHistory={[
          {
            id: "extract",
            kind: "extracting",
            label: "Extract input",
            stepIds: [],
            status: "done",
            passes: {},
          },
          {
            id: "guarded",
            kind: "dispatching",
            label: "Conditional wave",
            stepIds: ["guarded"],
            status: "done",
            passes: {},
          },
          {
            id: "later",
            kind: "dispatching",
            label: "Later wave",
            stepIds: ["later"],
            status: "active",
            passes: {},
          },
        ]}
      />,
    );
    expect(
      screen.getByText("Guarded").closest("[data-status]"),
    ).toHaveAttribute("data-status", "done");
    expect(screen.getByText("Later").closest("[data-status]")).toHaveAttribute(
      "data-status",
      "active",
    );
  });

  it("shows an explicitly skipped planned stage without shifting a later same-kind stage", () => {
    const plan: ExecutionPlanStage[] = [
      {
        id: "merge-1",
        kind: "merging",
        label: "Merge wave 1",
        stepIds: ["first"],
      },
      {
        id: "merge-2",
        kind: "merging",
        label: "Merge wave 2",
        stepIds: ["second"],
      },
      { id: "done", kind: "done", label: "Complete", stepIds: [] },
    ];
    render(
      <PipelineProgress
        state={{ kind: "merging", passes: {} }}
        plan={plan}
        stageHistory={[
          {
            id: "merge-1",
            kind: "merging",
            label: "Merge wave 1",
            stepIds: ["first"],
            status: "skipped",
            passes: {},
          },
          {
            id: "merge-2",
            kind: "merging",
            label: "Merge wave 2",
            stepIds: ["second"],
            status: "active",
            passes: {},
          },
        ]}
      />,
    );
    expect(
      screen.getByText("Merge wave 1").closest("[data-status]"),
    ).toHaveAttribute("data-status", "skipped");
    expect(
      screen.getByText("Merge wave 2").closest("[data-status]"),
    ).toHaveAttribute("data-status", "active");
  });
});

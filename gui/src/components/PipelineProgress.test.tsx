import { describe, it, expect, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import PipelineProgress from "./PipelineProgress";
import type { ExecutionPlanStage } from "../lib/pipelineHelpers";
import type { RuntimeStage } from "../lib/reviewRuntimeTypes";
import {
  applyPass,
  applyStage,
  completeRun,
  emptyRunView,
} from "../lib/runViews";
import type { PipelineReport, PipelineResult } from "../lib/types";

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
    expect(screen.getByText("Reviewers working together")).toBeInTheDocument();
    expect(screen.getByText("Next step")).toBeInTheDocument();
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
      .getByText("Reviewers working together")
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
      .getByText("Reviewers working together")
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
      .getByText("Reviewers working together")
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

  it("announces stage changes without reading the ticking clocks aloud", () => {
    vi.useFakeTimers();
    try {
      const startedAt = Date.now();
      const view = render(
        <PipelineProgress
          state={{ kind: "extracting" }}
          runStartedAt={startedAt}
        />,
      );
      const status = screen.getByRole("status");
      expect(status).toHaveTextContent("Processing inputs in progress.");
      expect(document.querySelector("[aria-live]")).toBeNull();
      expect(status).not.toContainElement(screen.getByText("0s"));

      act(() => {
        vi.advanceTimersByTime(3_000);
      });
      expect(screen.getByText("3s")).toBeInTheDocument();
      expect(screen.getByRole("status")).toHaveTextContent(
        "Processing inputs in progress.",
      );

      view.rerender(
        <PipelineProgress
          state={{ kind: "error", message: "boom", failedAt: "extracting" }}
          runStartedAt={startedAt}
        />,
      );
      expect(screen.getByRole("status")).toHaveTextContent(
        "Processing inputs failed.",
      );
    } finally {
      vi.useRealTimers();
    }
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

  describe("a stage whose reviewer failed", () => {
    const wave = (status: "active" | "done"): RuntimeStage => ({
      id: "wave-1",
      kind: "dispatching",
      label: "Parallel agent wave",
      stepIds: ["technical", "methods"],
      stepLabels: ["Technical", "Methods"],
      status,
      passes: { technical: "error", methods: "done" },
    });
    const next: RuntimeStage = {
      id: "wave-2",
      kind: "synthesizing",
      label: "Sequential agent wave",
      stepIds: ["synthesis"],
      stepLabels: ["Synthesis"],
      status: "active",
      passes: {},
    };
    const row = (label: string) =>
      screen.getByText(label).closest("[data-status]");
    const finished = {
      kind: "done",
      markdown: "",
      report: {} as PipelineReport,
      extractedText: "",
      runId: null,
    } as const;

    it("shows as failed once the run moves on, and says so aloud", () => {
      render(
        <PipelineProgress
          state={{ kind: "synthesizing", passes: {} }}
          stageHistory={[wave("done"), next]}
        />,
      );
      expect(row("Reviewers working together")).toHaveAttribute(
        "data-status",
        "failed",
      );
      expect(screen.getByRole("status")).toHaveTextContent(
        "Reviewers working together failed. Next step in progress.",
      );
    });

    it("stays failed when the researcher stops a later stage", () => {
      render(
        <PipelineProgress
          state={{
            kind: "error",
            message: "Pipeline cancelled",
            failedAt: "synthesizing",
          }}
          stageHistory={[wave("done"), { ...next, status: "failed" }]}
        />,
      );
      expect(row("Reviewers working together")).toHaveTextContent("failed");
      expect(row("Reviewers working together")).not.toHaveTextContent(
        "cancelled",
      );
      expect(row("Next step")).toHaveTextContent("cancelled");
      expect(screen.getByRole("status")).toHaveTextContent(
        "Reviewers working together failed. Next step cancelled.",
      );
    });

    it("stays failed when one row holds both the failure and the stop", () => {
      const first: RuntimeStage = {
        ...next,
        id: "wave-3",
        status: "done",
        passes: { synthesis: "error" },
      };
      const second: RuntimeStage = {
        ...next,
        id: "wave-4",
        stepIds: ["memo"],
        stepLabels: ["Memo"],
        status: "failed",
      };
      render(
        <PipelineProgress
          state={{
            kind: "error",
            message: "Pipeline cancelled",
            failedAt: "synthesizing",
          }}
          stageHistory={[first, second]}
        />,
      );
      expect(screen.getByRole("status")).toHaveTextContent(/failed\.$/);
      expect(screen.getByRole("status")).not.toHaveTextContent("cancelled");
    });

    it("stays failed after the run finishes", () => {
      render(
        <PipelineProgress state={finished} stageHistory={[wave("done")]} />,
      );
      expect(row("Reviewers working together")).toHaveAttribute(
        "data-status",
        "failed",
      );
      expect(screen.getByRole("status")).toHaveTextContent(
        "Reviewers working together failed. Review complete.",
      );
    });

    it("does not mark its other reviewers as failed", () => {
      render(
        <PipelineProgress
          state={{ kind: "synthesizing", passes: {} }}
          stageHistory={[
            {
              ...wave("done"),
              stepIds: ["technical", "methods", "planned"],
              stepLabels: ["Technical", "Methods", "Planned"],
            },
            next,
          ]}
        />,
      );
      expect(screen.getByText("Planned").parentElement).not.toHaveTextContent(
        "failed",
      );
    });

    it("leaves a clean stage done", () => {
      render(
        <PipelineProgress
          state={finished}
          stageHistory={[
            { ...wave("done"), passes: { technical: "done", methods: "done" } },
          ]}
        />,
      );
      expect(row("Reviewers working together")).toHaveAttribute(
        "data-status",
        "done",
      );
    });
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

    expect(screen.getAllByText("Next step")).toHaveLength(1);
    const sequentialStage = screen
      .getByText("Next step")
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

    expect(screen.getAllByText("Reviewers working together")).toHaveLength(2);
    expect(screen.getByText("Next step")).toBeInTheDocument();
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

  it("shows waves stopped by a Review limit as skipped after the run completes", () => {
    const plan: ExecutionPlanStage[] = [
      {
        id: "extracting",
        kind: "extracting",
        label: "Creating document bundle",
        stepIds: [],
      },
      {
        id: "wave-1-parallel",
        kind: "dispatching",
        label: "Parallel agent wave",
        stepIds: ["methods"],
        stepLabels: ["Methods review"],
      },
      {
        id: "wave-2-sequential",
        kind: "synthesizing",
        label: "Sequential agent wave",
        stepIds: ["consolidate"],
        stepLabels: ["Consolidate Feedback"],
      },
      {
        id: "wave-3-sequential",
        kind: "synthesizing",
        label: "Sequential agent wave",
        stepIds: ["validate"],
        stepLabels: ["Validate Feedback"],
      },
      { id: "done", kind: "done", label: "Complete", stepIds: [] },
    ];
    let view = emptyRunView(0);
    view = applyStage(view, {
      stage: "extracting",
      id: "extracting",
      label: "Creating document bundle",
      stepIds: [],
    });
    view = applyStage(view, {
      stage: "dispatching",
      id: "wave-1-parallel",
      label: "Parallel agent wave",
      stepIds: ["methods"],
      stepLabels: ["Methods review"],
    });
    view = applyPass(view, { name: "methods", status: "done" }, 1);
    // The executor's events when the limit stops scheduling: a skipped stage
    // for every unstarted wave, then the not-started errors.
    view = applyStage(view, {
      stage: "synthesizing",
      id: "wave-2-sequential",
      label: "Sequential agent wave",
      stepIds: ["consolidate"],
      stepLabels: ["Consolidate Feedback"],
      skipped: true,
    });
    view = applyStage(view, {
      stage: "synthesizing",
      id: "wave-3-sequential",
      label: "Sequential agent wave",
      stepIds: ["validate"],
      stepLabels: ["Validate Feedback"],
      skipped: true,
    });
    view = applyPass(view, { name: "consolidate", status: "error" }, 2);
    view = applyPass(view, { name: "validate", status: "error" }, 2);
    view = completeRun(view, {
      markdown: "",
      extracted_text: "",
      report: {},
    } as unknown as PipelineResult);

    render(
      <PipelineProgress
        state={view.state}
        plan={plan}
        stageHistory={view.stageHistory}
      />,
    );
    expect(
      screen.getByText("Methods review").closest("[data-status]"),
    ).toHaveAttribute("data-status", "done");
    expect(
      screen.getByText("Consolidate Feedback").closest("[data-status]"),
    ).toHaveAttribute("data-status", "skipped");
    expect(
      screen.getByText("Validate Feedback").closest("[data-status]"),
    ).toHaveAttribute("data-status", "skipped");
    // The errors did not land in the parallel wave that ran before them.
    expect(screen.queryByText("Consolidate")).toBeNull();
    expect(screen.queryByText("Validate")).toBeNull();
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

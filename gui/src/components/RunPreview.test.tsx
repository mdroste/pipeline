import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import RunPreview from "./RunPreview";
import type { PipelineConfig } from "../lib/types";

const config: PipelineConfig = {
  use_orientation: true,
  orientation_prompt: "",
  extraction: { method: "pdftotext", input_mode: "document" },
  parallel_context_template: "{step_prompt}",
  variables: [],
  context_cache: { enabled: false },
  merge: { enabled: false, prompt: "", agents: [] },
  steps: [
    {
      id: "analysis",
      label: "Analysis",
      prompt: "Review",
      enabled: true,
      phase: "parallel",
      tools: ["WebSearch"],
      agents: ["claude", "antigravity"],
      context: {
        include: [
          { kind: "primary", parts: ["text", "visuals"] },
          { kind: "survey" },
        ],
      },
      for_each: { glob: "*.tex", max: 3 },
    },
    {
      id: "synthesis",
      label: "Synthesis",
      prompt: "Combine",
      enabled: true,
      phase: "sequential",
      tools: [],
      agents: ["local"],
      context: {
        include: [{ kind: "step", step: "analysis", parts: ["report"] }],
      },
    },
  ],
};

describe("RunPreview", () => {
  it("summarizes the run and keeps per-step access in a disclosure", async () => {
    const user = userEvent.setup();
    render(
      <RunPreview
        config={config}
        inputPath="/papers/draft.pdf"
        plan={{
          profileId: "review",
          inputMode: "document",
          inputInterpretation: "document",
          stages: [
            {
              id: "extract",
              kind: "extracting",
              label: "Create bundle",
              stepIds: [],
            },
            {
              id: "wave",
              kind: "dispatching",
              label: "Parallel wave",
              stepIds: ["analysis"],
              stepLabels: ["Analysis"],
            },
            { id: "done", kind: "done", label: "Complete", stepIds: [] },
          ],
        }}
        batchCount={2}
        onCancel={() => {}}
        onRun={() => {}}
      />,
    );

    expect(
      screen.getByRole("dialog", { name: "Ready to start" }),
    ).toBeVisible();
    expect(screen.getByText(/2 documents · document/)).toBeVisible();
    expect(screen.getByText("4–16 model calls")).toBeVisible();
    expect(
      screen.getByText(/^Claude, Gemini, Local, default provider/),
    ).toBeVisible();

    const details = screen.getByText("Reads: Analysis results");
    expect(details).not.toBeVisible();
    await user.click(screen.getByText("Steps and what each can read"));
    expect(details).toBeVisible();
    expect(
      screen.getByText("Reads: Document text, visuals · Orientation map"),
    ).toBeVisible();
    expect(screen.getByText("Tools: WebSearch")).toBeVisible();
    expect(screen.getByText("Orientation")).toBeVisible();
  });

  it("requires an explicit confirmation", async () => {
    const onRun = vi.fn();
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <RunPreview
        config={{ ...config, steps: [] }}
        inputPath="/papers/draft.pdf"
        plan={{
          profileId: "review",
          inputMode: "document",
          inputInterpretation: "document",
          stages: [],
        }}
        onCancel={onCancel}
        onRun={onRun}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Generate report" }));
    expect(onRun).toHaveBeenCalledOnce();
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("counts conditional steps in the maximum but not the minimum work estimate", () => {
    render(
      <RunPreview
        config={{
          ...config,
          use_orientation: true,
          steps: [
            {
              ...config.steps[1],
              id: "conditional",
              label: "Conditional",
              run_if: {
                kind: "survey_path",
                pointer: "/review_plan/specialist_ids",
                contains: "formal_proofs",
              },
            },
          ],
        }}
        inputPath="/papers/draft.pdf"
        plan={{
          profileId: "auto-review",
          inputMode: "document",
          inputInterpretation: "document",
          stages: [],
        }}
        onCancel={() => {}}
        onRun={() => {}}
      />,
    );
    // The required orientation call contributes the fixed unit.
    expect(screen.getByText("1–2 model calls")).toBeVisible();
    expect(screen.getByText("0 steps + 1 conditional")).toBeVisible();
  });

  it("shows bounded runtime-assembled specialists for Auto Review", () => {
    const coreContext = {
      include: [
        {
          kind: "primary" as const,
          parts: ["text" as const, "visuals" as const],
        },
        { kind: "survey" as const },
      ],
    };
    const coreStep = (id: string, label: string) => ({
      ...config.steps[0],
      id,
      label,
      agents: [],
      for_each: undefined,
      tools: [],
      context: coreContext,
    });
    const autoConfig: PipelineConfig = {
      ...config,
      orientation_schema: { "x-pipeline-contract": "auto-review-v2" },
      steps: [
        coreStep("auto_contribution", "Contribution & Literature"),
        coreStep("auto_consistency", "Claims & Consistency"),
        coreStep("auto_exposition", "Exposition & Architecture"),
        {
          ...config.steps[1],
          id: "auto_synthesis",
          label: "Consolidate",
          context: {
            include: [
              { kind: "step", step: "auto_contribution", parts: ["report"] },
              { kind: "step", step: "auto_consistency", parts: ["report"] },
              { kind: "step", step: "auto_exposition", parts: ["report"] },
            ],
          },
        },
      ],
    };
    render(
      <RunPreview
        config={autoConfig}
        inputPath="/papers/draft.pdf"
        plan={{
          profileId: "auto-review",
          inputMode: "document",
          inputInterpretation: "document",
          stages: [
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
              id: "parallel-1",
              kind: "dispatching",
              label: "Parallel agent wave",
              stepIds: [
                "auto_contribution",
                "auto_consistency",
                "auto_exposition",
              ],
              stepLabels: [
                "Contribution & Literature",
                "Claims & Consistency",
                "Exposition & Architecture",
              ],
            },
            {
              id: "synthesis",
              kind: "synthesizing",
              label: "Sequential agent wave",
              stepIds: ["auto_synthesis"],
              stepLabels: ["Consolidate"],
            },
            { id: "done", kind: "done", label: "Complete", stepIds: [] },
          ],
        }}
        onCancel={() => {}}
        onRun={() => {}}
      />,
    );
    expect(
      screen.getByText("4 steps + 2–6 specialists chosen for each document"),
    ).toBeVisible();
    expect(screen.getByText("7–11 model calls")).toBeVisible();
    expect(screen.getByText("Specialists")).toBeInTheDocument();
    expect(screen.getByText(/Specialist results \(2–6\)/)).toBeInTheDocument();
    expect(
      screen.getByText(/Exposition & Architecture results/),
    ).toBeInTheDocument();
  });

  it("uses the Quick schema range in work estimates and routing details", () => {
    const parallel = {
      ...config.steps[0],
      agents: [],
      for_each: undefined,
      tools: [],
    };
    const quickConfig: PipelineConfig = {
      ...config,
      orientation_schema: {
        "x-pipeline-contract": "auto-review-v2",
        properties: {
          review_plan: {
            properties: {
              subject_specialist_ids: { minItems: 1, maxItems: 2 },
              method_specialist_ids: { minItems: 1, maxItems: 2 },
            },
          },
        },
      },
      steps: [
        { ...parallel, id: "auto_consistency", label: "Claims & Consistency" },
        {
          ...parallel,
          id: "auto_exposition",
          label: "Exposition & Architecture",
        },
        {
          ...config.steps[1],
          id: "auto_synthesis",
          label: "Consolidate Feedback",
        },
      ],
    };
    render(
      <RunPreview
        config={quickConfig}
        inputPath="/papers/draft.pdf"
        plan={{
          profileId: "auto-review-quick",
          inputMode: "document",
          inputInterpretation: "document",
          stages: [
            {
              id: "parallel",
              kind: "dispatching",
              label: "Parallel agent wave",
              stepIds: ["auto_consistency", "auto_exposition"],
              stepLabels: ["Claims & Consistency", "Exposition & Architecture"],
            },
          ],
        }}
        onCancel={() => {}}
        onRun={() => {}}
      />,
    );

    expect(
      screen.getByText("3 steps + 2–4 specialists chosen for each document"),
    ).toBeVisible();
    expect(screen.getByText("6–8 model calls")).toBeVisible();
    expect(screen.getByText(/2–4 calls/)).toBeInTheDocument();
  });
});

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
      agents: ["claude", "gemini"],
      context: { include: [
        { kind: "primary", parts: ["text", "visuals"] },
        { kind: "survey" },
      ] },
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
      context: { include: [{ kind: "step", step: "analysis", parts: ["report"] }] },
    },
  ],
};

describe("RunPreview", () => {
  it("shows execution, provider, workload, and artifact-access details", () => {
    render(
      <RunPreview
        config={config}
        inputPath="/papers/draft.pdf"
        plan={{
          profileId: "review",
          inputMode: "document",
          inputInterpretation: "document",
          stages: [
            { id: "extract", kind: "extracting", label: "Create bundle", stepIds: [] },
            { id: "wave", kind: "dispatching", label: "Parallel wave", stepIds: ["analysis"], stepLabels: ["Analysis"] },
            { id: "done", kind: "done", label: "Complete", stepIds: [] },
          ],
        }}
        batchCount={2}
        onCancel={() => {}}
        onRun={() => {}}
      />,
    );

    expect(screen.getByRole("dialog", { name: "Review the execution plan" })).toBeVisible();
    expect(screen.getByText(/2 documents · document/)).toBeVisible();
    expect(screen.getByTitle("4–16")).toBeVisible();
    expect(screen.getByTitle("claude, gemini, local, default provider")).toBeVisible();
    expect(screen.getByText("Primary text, visuals")).toBeVisible();
    expect(screen.getByText("Output: analysis")).toBeVisible();
    expect(screen.getAllByText("WebSearch").length).toBeGreaterThan(0);
  });

  it("requires an explicit confirmation", async () => {
    const onRun = vi.fn();
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <RunPreview
        config={{ ...config, steps: [] }}
        inputPath="/papers/draft.pdf"
        plan={{ profileId: "review", inputMode: "document", inputInterpretation: "document", stages: [] }}
        onCancel={onCancel}
        onRun={onRun}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Start run" }));
    expect(onRun).toHaveBeenCalledOnce();
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("counts conditional steps in the maximum but not the minimum work estimate", () => {
    render(
      <RunPreview
        config={{
          ...config,
          use_orientation: false,
          steps: [{
            ...config.steps[1],
            id: "conditional",
            label: "Conditional",
            run_if: {
              kind: "survey_path",
              pointer: "/review_plan/specialist_ids",
              contains: "formal_proofs",
            },
          }],
        }}
        inputPath="/papers/draft.pdf"
        plan={{ profileId: "auto-review", inputMode: "document", inputInterpretation: "document", stages: [] }}
        onCancel={() => {}}
        onRun={() => {}}
      />,
    );
    expect(screen.getByTitle("0–1")).toBeVisible();
    expect(screen.getByTitle("0 fixed + 1 conditional")).toBeVisible();
  });

  it("shows bounded runtime-assembled specialists for Auto Review", () => {
    const coreContext = {
      include: [
        { kind: "primary" as const, parts: ["text" as const, "visuals" as const] },
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
          label: "Consolidate Auto Review",
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
            { id: "extract", kind: "extracting", label: "Creating document bundle", stepIds: [] },
            { id: "orient", kind: "orienting", label: "Creating orientation map & review plan", stepIds: [] },
            {
              id: "parallel-1",
              kind: "dispatching",
              label: "Parallel agent wave",
              stepIds: ["auto_contribution", "auto_consistency", "auto_exposition"],
              stepLabels: ["Contribution & Literature", "Claims & Consistency", "Exposition & Architecture"],
            },
            { id: "synthesis", kind: "synthesizing", label: "Sequential agent wave", stepIds: ["auto_synthesis"], stepLabels: ["Consolidate Auto Review"] },
            { id: "done", kind: "done", label: "Complete", stepIds: [] },
          ],
        }}
        onCancel={() => {}}
        onRun={() => {}}
      />,
    );
    expect(screen.getByText("4 fixed")).toBeVisible();
    expect(screen.getByText("+ 2–6 adaptive at run time")).toBeVisible();
    expect(screen.getByTitle("7–11")).toBeVisible();
    expect(screen.getByText(/1–2 subject and 1–4 method specialists/)).toBeVisible();
    expect(screen.getByText("Subject specialists (1–2, auto-selected)")).toBeVisible();
    expect(screen.getByText("Method specialists (1–4, auto-selected)")).toBeVisible();
    expect(screen.getByText("Subject specialists")).toBeVisible();
    expect(screen.getByText("Method specialists")).toBeVisible();
    expect(screen.getByText("Reports: selected subject specialists (1–2)")).toBeVisible();
    expect(screen.getByText("Reports: selected method specialists (1–4)")).toBeVisible();
    expect(screen.getAllByText("Exposition & Architecture")[0]).not.toHaveClass("truncate");
    expect(screen.getByText("4 fixed")).not.toHaveClass("truncate");
  });
});

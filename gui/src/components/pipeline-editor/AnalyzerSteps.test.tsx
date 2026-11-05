import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import AddStepDialog from "./AddStepDialog";
import AnalyzerStepPanel from "./AnalyzerStepPanel";
import RunPreview from "../RunPreview";
import { analyzerStep, type AnalyzerInfo } from "../../lib/analyzers";
import type { PipelineConfig } from "../../lib/types";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const referenceCheck: AnalyzerInfo = {
  id: "reference_audit",
  version: 1,
  label: "Reference check",
  description: "Checks each reference against Crossref.",
  reads: ["Extracted document"],
  networkServices: ["Crossref"],
  networkPayload: "Each reference's text as written in the bibliography.",
};
const packageCheck: AnalyzerInfo = {
  id: "replication_static",
  version: 1,
  label: "Replication package check",
  description: "Checks a replication package folder.",
  reads: ["Source folder"],
  networkServices: [],
  networkPayload: "",
};

function config(steps: PipelineConfig["steps"]): PipelineConfig {
  return {
    use_orientation: true,
    orientation_prompt: "",
    extraction: { method: "", input_mode: "document" },
    parallel_context_template: "{step_prompt}",
    variables: [],
    context_cache: { enabled: false },
    merge: { enabled: false, prompt: "", agents: [] },
    steps,
  };
}

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation(async (command: string) => {
    if (command === "list_analyzers") return [referenceCheck, packageCheck];
    throw new Error(`unexpected ${command}`);
  });
});

describe("Pipeline checks in the designer", () => {
  it("adds a check with an explicit network choice", async () => {
    const onCreate = vi.fn();
    const user = userEvent.setup();
    render(<AddStepDialog onCreate={onCreate} onCancel={vi.fn()} />);
    await user.click(screen.getByRole("tab", { name: "Pipeline check" }));
    await user.click(
      await screen.findByRole("radio", { name: /Reference check/ }),
    );
    const lookup = screen.getByRole("checkbox", {
      name: /Look up at Crossref/,
    });
    expect(lookup).not.toBeChecked();
    expect(
      screen.getByText(/as written in the bibliography/),
    ).toBeInTheDocument();
    await user.click(lookup);
    await user.click(screen.getByRole("button", { name: "Create step" }));
    expect(onCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        mode: "check",
        analyzer: { info: referenceCheck, network: true },
      }),
    );
  });

  it("builds analyzer steps without a prompt, provider, or model", () => {
    const step = analyzerStep(packageCheck, "package", true);
    expect(step).toMatchObject({
      kind: "analyzer",
      prompt: "",
      agents: [],
      analyzer: { id: "replication_static", version: 1 },
    });
    // A check that cannot use the network carries no network option.
    expect(step.analyzer?.options).toBeUndefined();
    expect(
      analyzerStep(referenceCheck, "refs", false).analyzer?.options,
    ).toEqual({ network: false });
  });

  it("edits the network option and limits findings to sequential checks", async () => {
    const onUpdate = vi.fn();
    const user = userEvent.setup();
    const step = analyzerStep(referenceCheck, "refs", false);
    render(
      <AnalyzerStepPanel
        step={step}
        config={config([step])}
        onUpdate={onUpdate}
        onPhaseChange={vi.fn()}
        onOutputRoleChange={vi.fn()}
      />,
    );
    expect(
      await screen.findByText("Checks each reference against Crossref."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("checkbox", { name: /as the Review's findings/ }),
    ).toBeDisabled();
    await user.click(
      screen.getByRole("checkbox", { name: /Look up at Crossref/ }),
    );
    expect(onUpdate).toHaveBeenCalledWith("refs", {
      analyzer: {
        id: "reference_audit",
        version: 1,
        options: { network: true },
      },
    });
  });

  it("says when this Pipeline lacks a saved check", async () => {
    const step = { ...analyzerStep(packageCheck, "old", false) };
    step.analyzer = { id: "retired_check", version: 3 };
    render(
      <AnalyzerStepPanel
        step={step}
        config={config([step])}
        onUpdate={vi.fn()}
        onPhaseChange={vi.fn()}
        onOutputRoleChange={vi.fn()}
      />,
    );
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "does not provide this check",
      ),
    );
  });
});

describe("Pipeline checks in the run preview", () => {
  it("lists checks as local work and discloses their lookups", () => {
    const step = analyzerStep(referenceCheck, "refs", true);
    render(
      <RunPreview
        config={config([step])}
        inputPath="/papers/draft.pdf"
        plan={{
          profileId: "custom",
          inputMode: "document",
          inputInterpretation: "document",
          stages: [],
          hostChecks: [
            {
              stepId: "refs",
              stepLabel: "Reference check",
              analyzer: "Reference check",
              reads: ["Extracted document"],
              networkServices: ["Crossref"],
              networkPayload: "Each reference's text.",
            },
          ],
        }}
        onCancel={vi.fn()}
        onRun={vi.fn()}
      />,
    );
    expect(
      screen.getByText(
        /Pipeline check \(no model call\) · runs on this computer/,
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("region", {
        name: "Pipeline checks that use the network",
      }),
    ).toHaveTextContent("contacts Crossref");
  });
});

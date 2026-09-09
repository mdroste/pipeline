import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { StepConfig } from "../../lib/types";
import AdvancedStepOptions from "./AdvancedStepOptions";

function artifactFanOutStep(): StepConfig {
  return {
    id: "verify",
    label: "Verify",
    prompt: "Verify {item}",
    enabled: true,
    phase: "parallel",
    tools: [],
    agents: [],
    context: { include: [] },
    for_each: {
      glob: "",
      max: 20,
      artifact: { step: "synthesis", pointer: "/findings" },
    },
  };
}

describe("AdvancedStepOptions artifact fan-out", () => {
  it("preserves the artifact source when editing its pointer and cap", () => {
    const onChange = vi.fn();
    render(
      <AdvancedStepOptions
        step={artifactFanOutStep()}
        otherSteps={[{ id: "synthesis", label: "Synthesis" }]}
        defaultReportStepIds={[]}
        namedInputs={[]}
        inputMode="document"
        surveyEnabled
        conditionStepIds={[]}
        section="execution"
        onChange={onChange}
      />,
    );

    expect(screen.getByLabelText("Fan-out source type")).toHaveValue(
      "artifact",
    );
    fireEvent.change(screen.getByLabelText("Fan-out JSON pointer"), {
      target: { value: "/findings/0/evidence" },
    });
    expect(onChange).toHaveBeenLastCalledWith({
      for_each: {
        glob: "",
        max: 20,
        artifact: { step: "synthesis", pointer: "/findings/0/evidence" },
      },
    });

    fireEvent.change(screen.getByLabelText("Maximum fan-out items"), {
      target: { value: "7" },
    });
    expect(onChange).toHaveBeenLastCalledWith({
      for_each: {
        glob: "",
        max: 7,
        artifact: { step: "synthesis", pointer: "/findings" },
      },
    });
  });
});

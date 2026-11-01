import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
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
  const renderOptions = (step: StepConfig) => {
    const onChange = vi.fn();
    render(
      <AdvancedStepOptions
        step={step}
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
    return onChange;
  };

  it("offers no fan-out to a sequential step, and says why", () => {
    renderOptions({
      ...artifactFanOutStep(),
      phase: "sequential",
      for_each: null,
    });
    expect(
      screen.getByRole("checkbox", { name: "Run once for each item" }),
    ).toBeDisabled();
    expect(
      screen.getByText(
        "Only a step that runs independently can repeat for each item.",
      ),
    ).toBeVisible();
  });

  it("lets an imported sequential step turn its fan-out off", () => {
    const onChange = renderOptions({
      ...artifactFanOutStep(),
      phase: "sequential",
    });
    fireEvent.click(
      screen.getByRole("checkbox", { name: "Run once for each item" }),
    );
    expect(onChange).toHaveBeenLastCalledWith({ for_each: null });
  });

  it("keeps fan-out available to a parallel step", () => {
    renderOptions({ ...artifactFanOutStep(), for_each: null });
    expect(
      screen.getByRole("checkbox", { name: "Run once for each item" }),
    ).toBeEnabled();
    expect(
      screen.queryByText(/Only a step that runs independently/),
    ).not.toBeInTheDocument();
  });

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

    fireEvent.change(screen.getByLabelText("Maximum items"), {
      target: { value: "7" },
    });
    fireEvent.blur(screen.getByLabelText("Maximum items"));
    expect(onChange).toHaveBeenLastCalledWith({
      for_each: {
        glob: "",
        max: 7,
        artifact: { step: "synthesis", pointer: "/findings" },
      },
    });
  });
});

describe("AdvancedStepOptions typed values", () => {
  function Harness({
    initial,
    onChange,
  }: {
    initial: StepConfig;
    onChange: (step: StepConfig) => void;
  }) {
    const [step, setStep] = useState(initial);
    return (
      <AdvancedStepOptions
        step={step}
        otherSteps={[]}
        defaultReportStepIds={[]}
        namedInputs={[]}
        inputMode="document"
        surveyEnabled
        conditionStepIds={[]}
        section="execution"
        onChange={(patch) => {
          const next = { ...step, ...patch };
          setStep(next);
          onChange(next);
        }}
      />
    );
  }
  const surveyStep = (equals?: unknown): StepConfig => ({
    ...artifactFanOutStep(),
    for_each: null,
    run_if: { kind: "survey_path", pointer: "/score", equals },
  });
  const typeInto = async (label: string, text: string, initial: StepConfig) => {
    const user = userEvent.setup();
    const changed = vi.fn();
    render(<Harness initial={initial} onChange={changed} />);
    const field = screen.getByLabelText(label);
    await user.clear(field);
    await user.type(field, text);
    expect(field).toHaveValue(
      field.getAttribute("type") === "number" ? Number(text) : text,
    );
    await user.tab();
    return { changed, field };
  };

  it("commits a typed decimal whole", async () => {
    const { changed } = await typeInto(
      "Survey value to equal",
      "1.05",
      surveyStep(),
    );
    expect(changed).toHaveBeenCalledTimes(1);
    expect(changed.mock.lastCall![0].run_if.equals).toBe(1.05);
  });

  it("keeps a number-like string a string", async () => {
    const { changed, field } = await typeInto(
      "Survey value to equal",
      '"5"',
      surveyStep(),
    );
    expect(changed.mock.lastCall![0].run_if.equals).toBe("5");
    expect(field).toHaveValue('"5"');
  });

  it("treats null as no value, which is all the condition can hold", async () => {
    const { changed, field } = await typeInto(
      "Survey value to equal",
      "null",
      surveyStep(1),
    );
    const condition = changed.mock.lastCall![0].run_if;
    expect(condition).not.toHaveProperty("equals");
    expect(condition.exists).toBe(true);
    expect(field).toHaveValue("");
  });

  it("retypes the maximum without snapping to 1", async () => {
    const { changed } = await typeInto("Maximum items", "50", {
      ...artifactFanOutStep(),
      for_each: { glob: "*", max: 20 },
    });
    expect(changed).toHaveBeenCalledTimes(1);
    expect(changed.mock.lastCall![0].for_each.max).toBe(50);
  });

  it("restores the maximum when the field is left empty", async () => {
    const user = userEvent.setup();
    const changed = vi.fn();
    render(
      <Harness
        initial={{
          ...artifactFanOutStep(),
          for_each: { glob: "*", max: 20 },
        }}
        onChange={changed}
      />,
    );
    const field = screen.getByLabelText("Maximum items");
    await user.clear(field);
    await user.tab();
    expect(changed).not.toHaveBeenCalled();
    expect(field).toHaveValue(20);
  });
});

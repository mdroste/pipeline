import { describe, it, expect, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import WaveDiagram from "./WaveDiagram";
import type { MergeConfig, StepConfig } from "../lib/types";

function step(overrides: Partial<StepConfig> & { id: string }): StepConfig {
  return {
    label: overrides.id,
    prompt: "",
    enabled: true,
    phase: "parallel",
    tools: [],
    agents: [],
    context: { include: [] },
    ...overrides,
  } as StepConfig;
}

const merge: MergeConfig = { enabled: true, prompt: "", agents: [] };

describe("WaveDiagram", () => {
  it("renders preprocessing, step, and merge nodes for a multi-agent wave", () => {
    render(
      <WaveDiagram
        steps={[
          step({
            id: "technical",
            label: "Technical",
            agents: ["claude", "antigravity"],
          }),
          step({ id: "empirical", label: "Empirical" }),
          step({
            id: "consolidate",
            label: "Consolidate",
            phase: "sequential",
          }),
        ]}
        merge={merge}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByRole("button", { name: "Extract" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Orient" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Technical" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Merge" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Consolidate" }),
    ).toBeInTheDocument();
  });

  it("keeps the required orient node when merge is disabled", () => {
    render(
      <WaveDiagram
        steps={[step({ id: "technical", label: "Technical" })]}
        merge={{ ...merge, enabled: false }}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByRole("button", { name: "Orient" })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Merge" }),
    ).not.toBeInTheDocument();
  });

  it("reports selections for steps and pseudo-nodes", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <WaveDiagram
        steps={[step({ id: "technical", label: "Technical" })]}
        merge={merge}
        selectedId={null}
        onSelect={onSelect}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Technical" }));
    expect(onSelect).toHaveBeenLastCalledWith("technical");
    await user.click(screen.getByRole("button", { name: "Extract" }));
    expect(onSelect).toHaveBeenLastCalledWith("extraction");
    await user.click(screen.getByRole("button", { name: "Orient" }));
    expect(onSelect).toHaveBeenLastCalledWith("orientation");
  });

  it("shows a hint when no steps are enabled", () => {
    render(
      <WaveDiagram
        steps={[step({ id: "technical", enabled: false })]}
        merge={merge}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText(/No enabled review steps/)).toBeInTheDocument();
  });

  it("summarizes step count and provider calls", () => {
    render(
      <WaveDiagram
        steps={[step({ id: "technical", label: "Technical" })]}
        merge={merge}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    const overview = screen
      .getByRole("heading", { name: "Review overview" })
      .closest("section");
    expect(overview).toHaveTextContent("1 step");
    expect(overview).toHaveTextContent("≤ 1 model call");
    expect(
      screen.getByTitle("Up to 1 provider call before retries"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Technical" }),
    ).toBeInTheDocument();
  });

  it("numbers the combined preparation stage first and each wave after it", () => {
    render(
      <WaveDiagram
        steps={[
          step({ id: "first", label: "First", agents: ["claude", "codex"] }),
          step({ id: "second", label: "Second" }),
          step({ id: "report", label: "Report", phase: "sequential" }),
        ]}
        merge={merge}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    const stages = screen.getAllByRole("listitem");
    expect(stages[0]).toHaveTextContent("1Prepare document bundle");
    expect(stages[0]).toContainElement(
      screen.getByRole("button", { name: "Extract" }),
    );
    expect(stages[0]).toContainElement(
      screen.getByRole("button", { name: "Orient" }),
    );
    expect(stages[1]).toHaveTextContent("2In parallel");
    expect(stages[1]).toContainElement(
      screen.getByRole("button", { name: "First" }),
    );
    expect(stages[2]).toContainElement(
      screen.getByRole("button", { name: "Merge" }),
    );
    expect(stages[3]).toHaveTextContent("3Report");
  });

  it("names folder preparation as a source-tree inventory", () => {
    render(
      <WaveDiagram
        steps={[step({ id: "technical", label: "Technical" })]}
        merge={merge}
        inputMode="folder"
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    expect(
      screen.getByText("Prepare source-tree inventory"),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("Prepare document bundle"),
    ).not.toBeInTheDocument();
  });

  it("shows the caller's hint under the rail", () => {
    const { rerender } = render(
      <WaveDiagram
        steps={[step({ id: "technical", label: "Technical" })]}
        merge={merge}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    expect(
      screen.getByText("Select a stage to inspect or edit it."),
    ).toBeInTheDocument();
    rerender(
      <WaveDiagram
        steps={[step({ id: "technical", label: "Technical" })]}
        merge={merge}
        selectedId={null}
        onSelect={() => {}}
        hint="Select any stage to open it."
      />,
    );
    expect(
      screen.getByText("Select any stage to open it."),
    ).toBeInTheDocument();
  });

  it("groups a parallel wave and describes each card's agents", () => {
    render(
      <WaveDiagram
        steps={[
          step({
            id: "technical",
            label: "Technical",
            agents: ["claude", "codex"],
          }),
          step({ id: "empirical", label: "Empirical" }),
          step({
            id: "consolidate",
            label: "Consolidate",
            phase: "sequential",
          }),
        ]}
        merge={merge}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    const group = screen.getByRole("group", { name: "Parallel steps" });
    expect(
      within(group).getByRole("button", { name: "Technical" }),
    ).toHaveAccessibleDescription("Claude + ChatGPT");
    expect(
      within(group).getByRole("button", { name: "Empirical" }),
    ).toBeInTheDocument();
    expect(
      within(group).queryByRole("button", { name: "Consolidate" }),
    ).not.toBeInTheDocument();
  });

  it("does not count a Pipeline check as a model call", () => {
    render(
      <WaveDiagram
        steps={[
          step({ id: "review", label: "Review", agents: ["claude"] }),
          step({
            id: "references",
            label: "References",
            kind: "analyzer",
            analyzer: { id: "reference_audit", version: 1 },
          }),
        ]}
        merge={merge}
        selectedId={null}
        onSelect={vi.fn()}
      />,
    );
    expect(
      screen.getByTitle("Up to 1 provider call before retries"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "References" }),
    ).toHaveAccessibleDescription("Pipeline check");
  });

  it("represents Auto Review as a combined orientation call and two adaptive slots", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <WaveDiagram
        steps={[
          step({ id: "auto_contribution", label: "Contribution & Literature" }),
          step({ id: "auto_consistency", label: "Claims & Consistency" }),
          step({ id: "auto_exposition", label: "Exposition & Architecture" }),
          step({
            id: "auto_synthesis",
            label: "Consolidate",
            phase: "sequential",
          }),
        ]}
        merge={merge}
        adaptiveReview={true}
        selectedId={null}
        onSelect={onSelect}
      />,
    );

    expect(
      screen.getByRole("button", { name: "Orient + classify" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Adaptive agents (2–6)" }),
    ).toBeInTheDocument();
    const overview = screen
      .getByRole("heading", { name: "Review overview" })
      .closest("section");
    expect(overview).toHaveTextContent("4 saved steps");
    expect(overview).toHaveTextContent("2–6 auto-selected specialists");

    await user.click(
      screen.getByRole("button", { name: "Adaptive agents (2–6)" }),
    );
    expect(onSelect).toHaveBeenLastCalledWith("auto_adaptive_agents");
  });
});

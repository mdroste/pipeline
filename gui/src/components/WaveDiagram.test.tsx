import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
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
          step({ id: "technical", label: "Technical", agents: ["claude", "antigravity"] }),
          step({ id: "empirical", label: "Empirical" }),
          step({ id: "consolidate", label: "Consolidate", phase: "sequential" }),
        ]}
        merge={merge}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByRole("button", { name: "Extract" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Orient" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Technical" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Merge" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Consolidate" })).toBeInTheDocument();
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
    expect(screen.queryByRole("button", { name: "Merge" })).not.toBeInTheDocument();
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

  it("summarizes execution size and provider calls", () => {
    render(
      <WaveDiagram
        steps={[step({ id: "technical", label: "Technical" })]}
        merge={merge}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByRole("heading", { name: "Workflow overview" })).toBeInTheDocument();
    expect(screen.getByText(/1 enabled step in 1 execution wave/)).toBeInTheDocument();
    expect(screen.getByText(/up to 1 provider call before retries/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Technical" })).toBeInTheDocument();
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
          step({ id: "auto_synthesis", label: "Consolidate", phase: "sequential" }),
        ]}
        merge={merge}
        adaptiveReview={true}
        selectedId={null}
        onSelect={onSelect}
      />,
    );

    expect(screen.getByRole("button", { name: "Orient + classify" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Adaptive agents (2–6)" })).toBeInTheDocument();
    expect(screen.getByText(/4 saved steps plus 2–6 auto-selected specialists/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Adaptive agents (2–6)" }));
    expect(onSelect).toHaveBeenLastCalledWith("auto_adaptive_agents");
  });
});

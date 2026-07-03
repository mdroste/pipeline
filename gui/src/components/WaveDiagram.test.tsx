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
    ...overrides,
  } as StepConfig;
}

const merge: MergeConfig = { enabled: true, prompt: "", agents: [] };

describe("WaveDiagram", () => {
  it("renders preprocessing, step, and merge nodes for a multi-agent wave", () => {
    render(
      <WaveDiagram
        steps={[
          step({ id: "technical", label: "Technical", agents: ["claude", "gemini"] }),
          step({ id: "empirical", label: "Empirical" }),
          step({ id: "consolidate", label: "Consolidate", phase: "sequential" }),
        ]}
        merge={merge}
        useOrientation={true}
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

  it("omits the orient node and merge node when disabled", () => {
    render(
      <WaveDiagram
        steps={[step({ id: "technical", label: "Technical" })]}
        merge={{ ...merge, enabled: false }}
        useOrientation={false}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.queryByRole("button", { name: "Orient" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Merge" })).not.toBeInTheDocument();
  });

  it("reports selections for steps and pseudo-nodes", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <WaveDiagram
        steps={[step({ id: "technical", label: "Technical" })]}
        merge={merge}
        useOrientation={true}
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
        useOrientation={false}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText(/No enabled review steps/)).toBeInTheDocument();
  });

  it("collapses and expands the diagram", async () => {
    const user = userEvent.setup();
    render(
      <WaveDiagram
        steps={[step({ id: "technical", label: "Technical" })]}
        merge={merge}
        useOrientation={false}
        selectedId={null}
        onSelect={() => {}}
      />,
    );
    await user.click(screen.getByTitle("Hide diagram"));
    expect(screen.queryByRole("button", { name: "Technical" })).not.toBeInTheDocument();
    await user.click(screen.getByTitle("Show diagram"));
    expect(screen.getByRole("button", { name: "Technical" })).toBeInTheDocument();
  });
});

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import PipelinePage from "./PipelinePage";
import type { PipelineConfig, ProfileSummary } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: vi.fn(() => Promise.resolve(null)),
  open: vi.fn(() => Promise.resolve(null)),
}));

function makeConfig(): PipelineConfig {
  return {
    steps: [
      {
        id: "technical",
        label: "Technical",
        prompt: "Check the proofs.",
        enabled: true,
        phase: "parallel",
        tools: ["Read"],
        agents: ["claude"],
      },
      {
        id: "consolidate",
        label: "Consolidate Issues",
        prompt: "Merge {prior_outputs}.",
        enabled: true,
        phase: "sequential",
        tools: [],
        agents: ["claude"],
      },
    ],
    merge: { enabled: true, prompt: "", agents: [] },
    use_orientation: true,
    orientation_prompt: "",
    extraction: { method: "", marker_disable_ocr: null, marker_disable_images: null },
    parallel_context_template: "",
  };
}

const profiles: ProfileSummary[] = [
  { id: "deep-review", name: "Paper Review (Full)", step_count: 2, builtin: true },
];

function mockLoad(config: PipelineConfig) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_pipeline_config") return Promise.resolve(config);
    if (cmd === "list_profiles") return Promise.resolve(profiles);
    if (cmd === "get_active_profile") return Promise.resolve("deep-review");
    if (cmd === "save_pipeline_config") return Promise.resolve();
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

describe("PipelinePage", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("loads the active profile and renders its steps", async () => {
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);

    // Step labels appear in both the wave diagram and the step list.
    expect((await screen.findAllByText("Technical")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("Consolidate Issues").length).toBeGreaterThan(0);
    expect(invoke).toHaveBeenCalledWith("get_pipeline_config");
    expect(invoke).toHaveBeenCalledWith("list_profiles");
  });

  it("disables Save until the config is dirty", async () => {
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("adding a step marks the editor dirty and saving persists the config", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "+ Parallel" }));
    const save = screen.getByRole("button", { name: "Save" });
    expect(save).toBeEnabled();

    await user.click(save);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "save_pipeline_config",
        expect.objectContaining({
          config: expect.objectContaining({
            steps: expect.arrayContaining([
              expect.objectContaining({ id: "technical" }),
            ]),
          }),
        }),
      );
    });
    // The new step was included in the saved config.
    const saveCall = invoke.mock.calls.find(
      (c) => c[0] === "save_pipeline_config",
    );
    expect(saveCall?.[1].config.steps.length).toBe(3);
  });

  it("renders the execution-shape diagram for the loaded steps", async () => {
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    expect(screen.getByText("Execution shape")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Extract" })).toBeInTheDocument();
  });
});

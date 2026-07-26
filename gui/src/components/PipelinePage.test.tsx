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
        tools: [],
        agents: ["claude"],
        context: {
          include: [
            { kind: "primary", parts: ["text", "structure", "visuals", "source"] },
            { kind: "survey" },
          ],
        },
      },
      {
        id: "consolidate",
        label: "Consolidate Issues",
        prompt: "Merge {prior_outputs}.",
        enabled: true,
        phase: "sequential",
        tools: [],
        agents: ["claude"],
        context: {
          include: [{ kind: "step", step: "technical", parts: ["report"] }],
        },
      },
    ],
    merge: { enabled: true, prompt: "", agents: [] },
    use_orientation: true,
    orientation_prompt: "",
    extraction: { method: "" },
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
    expect(saveCall?.[1].config.steps[2]).toMatchObject({
      after: [],
      context: {
        include: [
          { kind: "primary", parts: ["text", "structure", "visuals", "source"] },
          { kind: "survey" },
        ],
      },
    });
  });

  it("edits the exact artifact allowlist for a step", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getAllByRole("button", { name: "Technical" })[0]);
    await user.click(screen.getByRole("button", { name: /Artifact access & execution rules/ }));
    expect(screen.queryByRole("button", { name: "Prior reports" })).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "Report" })).not.toBeInTheDocument();
    const readableText = screen.getByRole("checkbox", { name: "Readable text" });
    expect(readableText).toBeChecked();
    await user.click(readableText);

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      const technical = saveCall?.[1].config.steps.find(
        (step: { id: string }) => step.id === "technical",
      );
      expect(technical.context.include).toContainEqual({
        kind: "primary",
        parts: ["structure", "visuals", "source"],
      });
    });
  });

  it("removes step-output access when a step becomes parallel", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Consolidate Issues");

    await user.click(screen.getAllByRole("button", { name: "Consolidate Issues" })[0]);
    await user.click(screen.getByRole("button", { name: "Parallel" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      const consolidate = saveCall?.[1].config.steps.find(
        (step: { id: string }) => step.id === "consolidate",
      );
      expect(consolidate).toMatchObject({
        phase: "parallel",
        context: { include: [] },
      });
    });
  });

  it("reports dirty state to the shell and can hide its local back control", async () => {
    const user = userEvent.setup();
    const onDirtyChange = vi.fn();
    mockLoad(makeConfig());
    const { unmount } = render(
      <PipelinePage
        onClose={() => {}}
        onDirtyChange={onDirtyChange}
        showBack={false}
      />,
    );
    await screen.findAllByText("Technical");

    expect(onDirtyChange).toHaveBeenCalledWith(false);
    expect(screen.queryByRole("button", { name: "Back" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "+ Parallel" }));
    await waitFor(() => expect(onDirtyChange).toHaveBeenCalledWith(true));

    unmount();
    expect(onDirtyChange).toHaveBeenLastCalledWith(false);
  });

  it("offers an opt-in shared context cache in pipeline settings", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Pipeline Settings" }));
    const toggle = screen.getByRole("switch", { name: "Reuse shared input context" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-checked", "true");

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "save_pipeline_config",
        expect.objectContaining({
          config: expect.objectContaining({
            context_cache: { enabled: true },
          }),
        }),
      );
    });
  });

  it("renders the execution-shape diagram for the loaded steps", async () => {
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    expect(screen.getByText("Execution shape")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Extract" })).toBeInTheDocument();
  });
});

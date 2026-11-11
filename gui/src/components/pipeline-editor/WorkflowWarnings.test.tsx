import { expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import PipelinePage from "../PipelinePage";
import type { PipelineConfig, ProfileSummary } from "../../lib/types";
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
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
            {
              kind: "primary",
              parts: ["text", "structure", "visuals", "source"],
            },
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
  {
    id: "deep-review",
    name: "Paper Review (Full)",
    step_count: 2,
    builtin: true,
  },
];

function mockLoad(config: PipelineConfig, extra: Record<string, unknown> = {}) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd in extra) return Promise.resolve(extra[cmd]);
    if (cmd === "get_pipeline_config") return Promise.resolve(config);
    if (cmd === "list_profiles") return Promise.resolve(profiles);
    if (cmd === "get_active_profile") return Promise.resolve("deep-review");
    if (cmd === "save_pipeline_config") return Promise.resolve();
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

it("lists every reference warning by step name and focuses its field", async () => {
  localStorage.clear();
  const config = makeConfig();
  config.steps[1].prompt = Array.from(
    { length: 8 },
    (_, n) => `{step:missing${n}}`,
  ).join(" ");
  config.steps[1].run_if = {
    kind: "output_matches",
    step: "gone",
    pattern: "yes",
  };
  mockLoad(config);
  render(<PipelinePage onClose={vi.fn()} />);
  expect(await screen.findByText("Step references · 9 warnings")).toBeVisible();
  expect(
    screen.getByRole("heading", { name: "Consolidate Issues" }),
  ).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: /Prompt:.*missing7/ }));
  await waitFor(() =>
    expect(
      screen.getByLabelText("Prompt for Consolidate Issues"),
    ).toHaveFocus(),
  );
  fireEvent.click(screen.getByRole("button", { name: /Run condition:.*gone/ }));
  await waitFor(() =>
    expect(screen.getByLabelText("Run condition")).toHaveFocus(),
  );
  expect(screen.getByRole("tab", { name: "When it runs" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
});

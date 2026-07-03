import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import WorkflowPanel from "./WorkflowPanel";
import type { PipelineConfig, ProfileSummary } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

function makeConfig(): PipelineConfig {
  return {
    steps: [
      {
        id: "contribution",
        label: "Contribution",
        prompt: "",
        enabled: true,
        phase: "parallel",
        tools: ["Read"],
        agents: ["claude", "gemini"],
      },
      {
        id: "technical",
        label: "Technical",
        prompt: "",
        enabled: false,
        phase: "parallel",
        tools: ["Read"],
        agents: ["claude"],
      },
      {
        id: "consolidate",
        label: "Consolidate Issues",
        prompt: "",
        enabled: true,
        phase: "sequential",
        tools: [],
        agents: ["claude"],
      },
    ],
    merge: { enabled: true, prompt: "", agents: ["claude"] },
    use_orientation: true,
    orientation_prompt: "",
    extraction: { method: "", marker_disable_ocr: null, marker_disable_images: null },
    parallel_context_template: "",
  };
}

const profiles: ProfileSummary[] = [
  { id: "deep", name: "Deep Review", step_count: 3 },
  { id: "quick", name: "Quick Review", step_count: 2 },
];

function mockLoad(config: PipelineConfig) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_pipeline_config") return Promise.resolve(config);
    if (cmd === "list_profiles") return Promise.resolve(profiles);
    if (cmd === "get_active_profile") return Promise.resolve("deep");
    if (cmd === "switch_profile") return Promise.resolve(config);
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

function renderPanel(overrides: Partial<React.ComponentProps<typeof WorkflowPanel>> = {}) {
  return render(
    <WorkflowPanel
      disabled={false}
      editorOpen={false}
      onConfigure={() => {}}
      onProfileChange={() => {}}
      refreshKey={0}
      {...overrides}
    />
  );
}

describe("WorkflowPanel", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("renders the profile selector with the active profile selected", async () => {
    mockLoad(makeConfig());
    renderPanel();

    const select = await screen.findByRole("combobox");
    expect(select).toHaveValue("deep");
    expect(screen.getByRole("option", { name: "Deep Review" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Quick Review" })).toBeInTheDocument();
  });

  it("lists enabled steps grouped by phase, hiding disabled steps", async () => {
    mockLoad(makeConfig());
    renderPanel();

    expect(await screen.findByText("Contribution")).toBeInTheDocument();
    expect(screen.getByText("Consolidate Issues")).toBeInTheDocument();
    expect(screen.getByText("Parallel")).toBeInTheDocument();
    expect(screen.getByText("Sequential")).toBeInTheDocument();
    // Disabled steps are summarized, not listed
    expect(screen.queryByText("Technical")).not.toBeInTheDocument();
    expect(screen.getByText("+1 disabled step")).toBeInTheDocument();
  });

  it("renders concatenated agent initials next to multi-agent steps", async () => {
    mockLoad(makeConfig());
    renderPanel();
    expect(await screen.findByText("C+G")).toBeInTheDocument();
  });

  it("has no per-step toggles", async () => {
    mockLoad(makeConfig());
    renderPanel();
    await screen.findByText("Contribution");
    // The only buttons are Edit workflow (and none per step)
    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toHaveTextContent(/edit workflow/i);
  });

  it("switches profile via switch_profile and notifies the parent", async () => {
    mockLoad(makeConfig());
    const onProfileChange = vi.fn();
    renderPanel({ onProfileChange });

    const select = await screen.findByRole("combobox");
    await userEvent.setup().selectOptions(select, "quick");

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("switch_profile", { id: "quick" });
      expect(onProfileChange).toHaveBeenCalledTimes(1);
    });
    expect(select).toHaveValue("quick");
  });

  it("disables the profile selector while the editor is open", async () => {
    mockLoad(makeConfig());
    renderPanel({ editorOpen: true });

    const select = await screen.findByRole("combobox");
    expect(select).toBeDisabled();
  });

  it("calls onConfigure when Edit workflow is clicked", async () => {
    mockLoad(makeConfig());
    const onConfigure = vi.fn();
    renderPanel({ onConfigure });

    const link = await screen.findByRole("button", { name: /edit workflow/i });
    await userEvent.setup().click(link);
    expect(onConfigure).toHaveBeenCalledTimes(1);
  });
});

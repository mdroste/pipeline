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
        tools: [],
        agents: ["claude", "antigravity"],
        context: { include: [{ kind: "primary", parts: ["text", "source"] }] },
      },
      {
        id: "technical",
        label: "Technical",
        prompt: "",
        enabled: false,
        phase: "parallel",
        tools: [],
        agents: ["claude"],
        context: { include: [{ kind: "primary", parts: ["text", "source"] }] },
      },
      {
        id: "consolidate",
        label: "Consolidate Issues",
        prompt: "",
        enabled: true,
        phase: "sequential",
        tools: [],
        agents: ["claude"],
        context: {
          include: [{ kind: "step", step: "contribution", parts: ["report"] }],
        },
      },
      {
        id: "validate",
        label: "Validate Feedback",
        prompt: "",
        enabled: true,
        phase: "sequential",
        tools: [],
        agents: ["claude"],
        context: {
          include: [{ kind: "step", step: "consolidate", parts: ["report"] }],
        },
      },
    ],
    merge: { enabled: true, prompt: "", agents: ["claude"] },
    use_orientation: true,
    orientation_prompt: "",
    extraction: { method: "" },
    parallel_context_template: "",
  };
}

const profiles: ProfileSummary[] = [
  {
    id: "auto-paper-review",
    name: "Paper Review",
    step_count: 5,
    builtin: true,
  },
  {
    id: "auto-paper-review-fast",
    name: "Paper Review (Fast)",
    step_count: 4,
    builtin: true,
  },
  { id: "deep", name: "Paper Review (Full)", step_count: 3, builtin: false },
  { id: "quick", name: "Paper Review (Quick)", step_count: 2, builtin: false },
];

function mockLoad(config: PipelineConfig, active = "deep") {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_pipeline_config") return Promise.resolve(config);
    if (cmd === "list_profiles") return Promise.resolve(profiles);
    if (cmd === "get_active_profile") return Promise.resolve(active);
    if (cmd === "switch_profile") return Promise.resolve(config);
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

function renderPanel(
  overrides: Partial<React.ComponentProps<typeof WorkflowPanel>> = {},
) {
  return render(
    <WorkflowPanel
      disabled={false}
      onConfigure={() => {}}
      onProfileChange={() => {}}
      refreshKey={0}
      {...overrides}
    />,
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
    expect(
      screen.getByRole("option", { name: "Paper Review (Full)" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("option", { name: "Paper Review (Quick)" }),
    ).toBeInTheDocument();
  });

  it("keeps a saved design that needs repair in the list, with its reason", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(makeConfig());
      if (cmd === "list_profiles")
        return Promise.resolve([
          ...profiles,
          {
            id: "old-copy",
            name: "Old copy",
            step_count: 4,
            builtin: false,
            invalid: "Profile 'old-copy' is invalid: stale contract",
          },
        ]);
      if (cmd === "get_active_profile") return Promise.resolve("old-copy");
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    renderPanel();
    expect(
      await screen.findByRole("option", { name: "Old copy (needs repair)" }),
    ).toBeInTheDocument();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "stale contract",
    );
  });

  it("shows workflow choices with outcomes and step counts", async () => {
    mockLoad(makeConfig());
    renderPanel();

    await screen.findByRole("combobox");
    expect(screen.queryByText(/steps.*the full plan/)).not.toBeInTheDocument();
    // No per-step rows or toggles: the plan preview owns that.
    expect(screen.queryByText("Contribution")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /edit workflow/i }),
    ).toBeVisible();
    expect(screen.getByText("Compare workflows")).toBeVisible();
    expect(screen.getAllByText(/configured steps/).length).toBeGreaterThan(0);
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

  it("calls onConfigure when Edit workflow is clicked", async () => {
    mockLoad(makeConfig());
    const onConfigure = vi.fn();
    renderPanel({ onConfigure });

    const link = await screen.findByRole("button", { name: /edit workflow/i });
    await userEvent.setup().click(link);
    expect(onConfigure).toHaveBeenCalledTimes(1);
  });

  it.each(["auto-paper-review", "auto-paper-review-fast"])(
    "omits adaptive details for %s",
    async (profile) => {
      const config = makeConfig();
      config.orientation_schema = { "x-pipeline-contract": "auto-review-v2" };
      mockLoad(config, profile);
      renderPanel();
      await screen.findByRole("combobox");
      expect(screen.queryByText(/Adaptive agents:/)).not.toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: "Browse specialist catalog" }),
      ).not.toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /edit workflow/i }),
      ).toBeEnabled();
    },
  );
});

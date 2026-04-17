import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SidebarReferees from "./SidebarReferees";
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
    parallel_context_template: "",
  };
}

const profiles: ProfileSummary[] = [
  { id: "deep", name: "Deep Review", step_count: 3 },
];

function mockLoad(config: PipelineConfig) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_pipeline_config") return Promise.resolve(config);
    if (cmd === "list_profiles") return Promise.resolve(profiles);
    if (cmd === "get_active_profile") return Promise.resolve("deep");
    if (cmd === "save_pipeline_config") return Promise.resolve();
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

describe("SidebarReferees", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("renders parallel + sequential sections and the active profile name", async () => {
    mockLoad(makeConfig());
    render(<SidebarReferees disabled={false} onConfigure={() => {}} refreshKey={0} />);

    expect(await screen.findByText("Contribution")).toBeInTheDocument();
    expect(screen.getByText("Technical")).toBeInTheDocument();
    expect(screen.getByText("Consolidate Issues")).toBeInTheDocument();
    expect(screen.getByText("Parallel")).toBeInTheDocument();
    expect(screen.getByText("Sequential")).toBeInTheDocument();
    expect(screen.getByText("(Deep Review)")).toBeInTheDocument();
  });

  it("shows the merge indicator when merge is enabled and a parallel step has multiple agents", async () => {
    mockLoad(makeConfig());
    render(<SidebarReferees disabled={false} onConfigure={() => {}} refreshKey={0} />);
    expect(await screen.findByText("Cross-agent merge")).toBeInTheDocument();
  });

  it("hides the merge indicator when no parallel step uses multiple agents", async () => {
    const cfg = makeConfig();
    cfg.steps[0].agents = ["claude"];
    mockLoad(cfg);
    render(<SidebarReferees disabled={false} onConfigure={() => {}} refreshKey={0} />);

    await screen.findByText("Contribution");
    expect(screen.queryByText("Cross-agent merge")).not.toBeInTheDocument();
  });

  it("renders concatenated agent initials next to multi-agent steps", async () => {
    mockLoad(makeConfig());
    render(<SidebarReferees disabled={false} onConfigure={() => {}} refreshKey={0} />);
    expect(await screen.findByText("C+G")).toBeInTheDocument();
  });

  it("toggles a step and persists via save_pipeline_config", async () => {
    mockLoad(makeConfig());
    render(<SidebarReferees disabled={false} onConfigure={() => {}} refreshKey={0} />);

    const techRow = (await screen.findByText("Technical")).closest("div")!;
    const toggleBtn = techRow.querySelector("button")!;

    await userEvent.setup().click(toggleBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "save_pipeline_config",
        expect.objectContaining({
          config: expect.objectContaining({
            steps: expect.arrayContaining([
              expect.objectContaining({ id: "technical", enabled: true }),
            ]),
          }),
        })
      );
    });
  });

  it("reverts optimistic toggle when save_pipeline_config fails", async () => {
    const cfg = makeConfig();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(cfg);
      if (cmd === "list_profiles") return Promise.resolve(profiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep");
      if (cmd === "save_pipeline_config") return Promise.reject(new Error("write failed"));
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    render(<SidebarReferees disabled={false} onConfigure={() => {}} refreshKey={0} />);

    // Contribution starts enabled — clicking disables optimistically, then reverts.
    const contribRow = (await screen.findByText("Contribution")).closest("div")!;
    const toggleBtn = contribRow.querySelector("button")!;

    await userEvent.setup().click(toggleBtn);
    await waitFor(() => expect(errSpy).toHaveBeenCalled());

    // After revert, the saved config should still reflect enabled=true on Contribution.
    // The visual cue is the green bg; we verify via class on the toggle.
    await waitFor(() => {
      expect(toggleBtn.className).toContain("bg-green-500");
    });
    errSpy.mockRestore();
  });

  it("calls onConfigure when the customize link is clicked", async () => {
    mockLoad(makeConfig());
    const onConfigure = vi.fn();
    render(<SidebarReferees disabled={false} onConfigure={onConfigure} refreshKey={0} />);

    const link = await screen.findByRole("button", { name: /customize pipeline steps/i });
    await userEvent.setup().click(link);
    expect(onConfigure).toHaveBeenCalledTimes(1);
  });
});

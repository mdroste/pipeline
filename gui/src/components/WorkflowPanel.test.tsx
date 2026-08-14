import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import WorkflowPanel from "./WorkflowPanel";
import type { AutoReviewCatalog, PipelineConfig, ProfileSummary } from "../lib/types";

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
    ],
    merge: { enabled: true, prompt: "", agents: ["claude"] },
    use_orientation: true,
    orientation_prompt: "",
    extraction: { method: "" },
    parallel_context_template: "",
  };
}

const profiles: ProfileSummary[] = [
  { id: "auto-review", name: "Auto Paper Review", step_count: 4, builtin: true },
  { id: "deep", name: "Paper Review (Full)", step_count: 3, builtin: false },
  { id: "quick", name: "Paper Review (Quick)", step_count: 2, builtin: false },
];

const catalog: AutoReviewCatalog = {
  contract: "auto-review-v2",
  subjectCount: 2,
  methodCount: 1,
  disciplines: [{
    id: "mathematics",
    label: "Mathematics",
    roles: [
      {
        id: "subject_mathematics_general",
        label: "Mathematics — General",
        level: "discipline",
        description: "Mathematics spanning several subfields.",
        exclusions: "a listed subfield clearly fits.",
      },
      {
        id: "subject_mathematics_pde",
        label: "Mathematics — PDE & Calculus of Variations",
        level: "subfield",
        description: "Partial differential equations and variational problems.",
        exclusions: "the paper has no PDE or variational contribution.",
      },
    ],
  }],
  methodFamilies: [{
    id: "formal_conceptual",
    label: "Formal Theory & Conceptual Analysis",
    roles: [{
    id: "formal_proofs",
    label: "Method — Formal Proofs",
    level: "method",
    description: "Central theorems require proof verification.",
    exclusions: "proofs are routine and immaterial.",
    }],
  }],
};

function mockLoad(config: PipelineConfig, active = "deep") {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_pipeline_config") return Promise.resolve(config);
    if (cmd === "list_profiles") return Promise.resolve(profiles);
    if (cmd === "get_active_profile") return Promise.resolve(active);
    if (cmd === "get_auto_review_catalog") return Promise.resolve(catalog);
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
    expect(screen.getByRole("option", { name: "Paper Review (Full)" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Paper Review (Quick)" })).toBeInTheDocument();
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
    expect(await screen.findByText("C+A")).toBeInTheDocument();
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

  it("opens a searchable catalog modal instead of expanding Auto specialists inline", async () => {
    const autoConfig = makeConfig();
    autoConfig.orientation_schema = { "x-pipeline-contract": "auto-review-v2" };
    autoConfig.steps[0].run_if = {
      kind: "survey_path",
      pointer: "/review_plan/method_specialist_ids",
      contains: "formal_proofs",
    };
    mockLoad(autoConfig, "auto-review");
    renderPanel();

    expect(await screen.findByText("Adaptive agents")).toHaveClass("text-blue-700");
    expect(
      await screen.findByText(
        "Adaptive agents: 2-6 additional topic and methodology-specific review agents tailored for each document.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/conditional specialist/i)).not.toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Browse specialist catalog" }));
    expect(await screen.findByRole("dialog", { name: "Specialist catalog" })).toBeVisible();
    expect(screen.getByText("1 discipline")).toBeVisible();
    expect(screen.getByText("PDE & Calculus of Variations")).toBeVisible();

    await user.click(screen.getByRole("tab", { name: "Methods (1)" }));
    expect(screen.getByText("Formal Proofs")).toBeVisible();
    await user.type(screen.getByRole("searchbox", { name: "Search specialists" }), "unmatched role");
    expect(screen.getByText("No specialists match this search.")).toBeVisible();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Specialist catalog" })).not.toBeInTheDocument();
  });
});

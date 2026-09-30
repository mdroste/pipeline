import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import WorkflowPanel from "./WorkflowPanel";
import type {
  AutoReviewCatalog,
  PipelineConfig,
  ProfileSummary,
} from "../lib/types";

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
    id: "auto-review",
    name: "Automatic Paper Review (Full)",
    step_count: 5,
    builtin: true,
  },
  {
    id: "auto-review-quick",
    name: "Automatic Paper Review (Quick)",
    step_count: 4,
    builtin: true,
  },
  { id: "deep", name: "Paper Review (Full)", step_count: 3, builtin: false },
  { id: "quick", name: "Paper Review (Quick)", step_count: 2, builtin: false },
];

const catalog: AutoReviewCatalog = {
  contract: "auto-review-v2",
  revision: "sha256:test-catalog",
  subjectCount: 2,
  methodCount: 1,
  genreCount: 10,
  disciplines: [
    {
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
          description:
            "Partial differential equations and variational problems.",
          exclusions: "the paper has no PDE or variational contribution.",
        },
      ],
    },
  ],
  methodFamilies: [
    {
      id: "formal_conceptual",
      label: "Formal Theory & Conceptual Analysis",
      roles: [
        {
          id: "formal_proofs",
          label: "Method — Formal Proofs",
          level: "method",
          description: "Central theorems require proof verification.",
          exclusions: "proofs are routine and immaterial.",
        },
      ],
    },
  ],
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

  it("summarizes the workflow without duplicating the plan", async () => {
    mockLoad(makeConfig());
    renderPanel();

    expect(
      await screen.findByText(/3 steps · 1 disabled — the full plan/),
    ).toBeInTheDocument();
    // No per-step rows or toggles: the plan preview owns that.
    expect(screen.queryByText("Contribution")).not.toBeInTheDocument();
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

    expect(
      await screen.findByText(
        "Adaptive agents: 2–6 additional subject and method reviewers tailored to each document.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/conditional specialist/i),
    ).not.toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(
      screen.getByRole("button", { name: "Browse specialist catalog" }),
    );
    expect(
      await screen.findByRole("dialog", { name: "Specialist catalog" }),
    ).toBeVisible();
    expect(screen.getByText("1 discipline")).toBeVisible();
    expect(screen.getByText("PDE & Calculus of Variations")).toBeVisible();

    await user.click(screen.getByRole("tab", { name: "Methods (1)" }));
    expect(screen.getByText("Formal Proofs")).toBeVisible();
    await user.type(
      screen.getByRole("searchbox", { name: "Search specialists" }),
      "unmatched role",
    );
    expect(screen.getByText("No specialists match this search.")).toBeVisible();

    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("dialog", { name: "Specialist catalog" }),
    ).not.toBeInTheDocument();
  });

  it("shows the Quick profile's narrower adaptive range", async () => {
    const quickConfig = makeConfig();
    quickConfig.orientation_schema = {
      "x-pipeline-contract": "auto-review-v2",
      properties: {
        review_plan: {
          properties: {
            subject_specialist_ids: { minItems: 1, maxItems: 2 },
            method_specialist_ids: { minItems: 1, maxItems: 2 },
          },
        },
      },
    };
    mockLoad(quickConfig, "auto-review-quick");
    renderPanel();

    expect(
      await screen.findByText(
        /Adaptive agents: 2–4 additional subject and method reviewers/,
      ),
    ).toBeInTheDocument();
  });
});

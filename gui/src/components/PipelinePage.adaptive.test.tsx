import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import PipelinePage from "./PipelinePage";
import type {
  AutoReviewCatalog,
  PipelineConfig,
  ProfileSummary,
} from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
const confirmDialog = vi.hoisted(() => vi.fn());
const notify = vi.hoisted(() => vi.fn());
const dialogMocks = vi.hoisted(() => ({
  save: vi.fn(),
  open: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./DialogService", () => ({ confirmDialog, notify }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: dialogMocks.save,
  open: dialogMocks.open,
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

const adaptiveCatalog: AutoReviewCatalog = {
  contract: "auto-review-v2",
  revision: "sha256:test-catalog",
  subjectCount: 1,
  methodCount: 1,
  genreCount: 10,
  disciplines: [
    {
      id: "economics",
      label: "Economics",
      roles: [
        {
          id: "subject_economics_macro",
          label: "Economics — Macroeconomics",
          level: "subfield",
          description:
            "Macroeconomics, policy, growth, business cycles, and aggregate dynamics.",
          exclusions: "the contribution is primarily microeconomic.",
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
          description:
            "Central theorems and derivations require proof verification.",
          exclusions: "routine algebra.",
        },
      ],
    },
  ],
};

async function useAdvancedEditor(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "Advanced" }));
}

describe("PipelinePage", () => {
  beforeEach(() => {
    invoke.mockReset();
    confirmDialog.mockReset();
    notify.mockReset();
    dialogMocks.save.mockReset();
    dialogMocks.open.mockReset();
    dialogMocks.save.mockResolvedValue(null);
    dialogMocks.open.mockResolvedValue(null);
  });

  it("keeps Auto Review schemas compact while exposing the resolved provider contract", async () => {
    const user = userEvent.setup();
    const config = makeConfig();
    config.orientation_schema = {
      "x-pipeline-contract": "auto-review-v2",
      "x-pipeline-catalog-policy": "live",
      type: "object",
      properties: {
        review_plan: {
          type: "object",
          properties: {
            subject_specialist_ids: {
              type: "array",
              items: {
                type: "string",
                "x-pipeline-catalog": "auto-review.subjects",
              },
            },
          },
        },
      },
    };
    const resolved = {
      type: "object",
      properties: {
        review_plan: {
          type: "object",
          properties: {
            subject_specialist_ids: {
              type: "array",
              items: { type: "string", enum: ["subject_economics_macro"] },
            },
          },
        },
      },
    };
    mockLoad(config, {
      get_active_profile: "auto-review",
      get_auto_review_catalog: adaptiveCatalog,
      resolve_orientation_schema_catalogs: resolved,
    });
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");
    await useAdvancedEditor(user);

    await user.click(screen.getByRole("tab", { name: "Schemas" }));
    expect(screen.getByText("Catalog-backed")).toBeInTheDocument();
    expect(await screen.findByText("Live specialist catalog")).toBeVisible();
    expect(screen.getByText("1 subject roles")).toBeVisible();
    expect(screen.getByText("1 method roles")).toBeVisible();
    expect(screen.getByText("10 document genres")).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Browse catalog" }),
    ).toBeVisible();
    const editor = screen.getByRole("textbox", {
      name: "Orientation output JSON schema",
    });
    expect((editor as HTMLTextAreaElement).value).toContain(
      '"x-pipeline-catalog": "auto-review.subjects"',
    );
    expect((editor as HTMLTextAreaElement).value).not.toContain(
      "subject_economics_macro",
    );

    await user.click(
      screen.getByRole("button", { name: "View resolved provider schema" }),
    );
    const resolvedPreview = await screen.findByLabelText(
      "Resolved orientation provider schema",
    );
    expect(resolvedPreview).toHaveTextContent("subject_economics_macro");
    expect(invoke).toHaveBeenCalledWith("resolve_orientation_schema_catalogs", {
      schema: config.orientation_schema,
    });

    const withoutContract = structuredClone(config.orientation_schema);
    delete withoutContract["x-pipeline-contract"];
    fireEvent.change(editor, {
      target: { value: JSON.stringify(withoutContract) },
    });
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Automatic Paper Review must retain x-pipeline-contract",
    );
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("shows compact Auto Review slots and their direct synthesis contract", async () => {
    const user = userEvent.setup();
    const config = makeConfig();
    config.orientation_schema = { "x-pipeline-contract": "auto-review-v2" };
    config.orientation_prompt = [
      "SUBJECT SPECIALIST CATALOG",
      "{subject_catalog}",
      "METHOD SPECIALIST CATALOG",
      "{method_catalog}",
      "{genre_catalog}",
      "<paper>{paper_text}</paper>",
    ].join("\n\n");
    config.steps = [
      {
        ...config.steps[0],
        id: "auto_contribution",
        label: "Contribution & Literature",
      },
      {
        ...config.steps[0],
        id: "auto_consistency",
        label: "Claims & Consistency",
      },
      {
        ...config.steps[0],
        id: "auto_exposition",
        label: "Exposition & Architecture",
      },
      { ...config.steps[1], id: "auto_synthesis", label: "Consolidate" },
    ];
    mockLoad(config);
    render(<PipelinePage onClose={() => {}} />);

    const orientation = await screen.findByRole("button", {
      name: "Orientation & classification",
    });
    expect(
      screen.getByRole("button", {
        name: "Adaptive agents — Automatic, 2–6 agents",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getAllByText("Auto-filled from orientation").length,
    ).toBeGreaterThan(0);

    await user.click(orientation);
    expect(
      screen.getByRole("heading", { name: "Orientation & Classification" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/One LLM call builds the paper orientation map/),
    ).toBeInTheDocument();
    const promptEditor = screen.getByRole("textbox", {
      name: "Orientation map prompt",
    });
    expect((promptEditor as HTMLTextAreaElement).value).toContain(
      "{subject_catalog}",
    );
    expect((promptEditor as HTMLTextAreaElement).value).not.toContain(
      "subject_physics_general",
    );
    expect(
      screen.getByText(
        /Catalog placeholders are populated from the live specialist manifests/,
      ),
    ).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", {
        name: "Adaptive agents — Automatic, 2–6 agents",
      }),
    );
    expect(
      screen.getByRole("heading", { name: "Adaptive agents" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Every adaptive-agent report feeds directly/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/alongside Contribution & Literature/),
    ).toBeInTheDocument();
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Number of adaptive agents" }),
      "4",
    );
    expect(
      screen.getByRole("button", { name: "Adaptive agents — 4 agents" }),
    ).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Browse method catalog" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Specialist catalog" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Methods" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.click(
      screen.getByRole("button", { name: "Close specialist catalog" }),
    );

    await user.click(screen.getByRole("tab", { name: "Overview" }));
    expect(
      screen.getByRole("button", { name: "Orient + classify" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Adaptive agents (4)" }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Steps" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
      expect(saveCall?.[1].config.orientation_schema).toMatchObject({
        "x-pipeline-contract": "auto-review-v2",
        "x-pipeline-adaptive-agent-count": 4,
      });
    });
  });

  it("uses the Quick adaptive range and omits contribution from synthesis", async () => {
    const user = userEvent.setup();
    const config = makeConfig();
    config.orientation_schema = {
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
    config.steps = [
      {
        ...config.steps[0],
        id: "auto_consistency",
        label: "Claims & Consistency",
      },
      {
        ...config.steps[0],
        id: "auto_exposition",
        label: "Exposition & Architecture",
      },
      {
        ...config.steps[1],
        id: "auto_synthesis",
        label: "Consolidate Feedback",
      },
    ];
    mockLoad(config, { get_active_profile: "auto-review-quick" });
    render(<PipelinePage onClose={() => {}} />);

    const adaptive = await screen.findByRole("button", {
      name: "Adaptive agents — Automatic, 2–4 agents",
    });
    await user.click(adaptive);

    const count = screen.getByRole("combobox", {
      name: "Number of adaptive agents",
    });
    expect(
      screen.getByRole("option", { name: "Automatic (2–4)" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "5" })).not.toBeInTheDocument();
    expect(screen.getByText(/1 to 2 method reviewers/)).toBeInTheDocument();
    expect(
      screen.getByText(
        /alongside Claims & Consistency, Exposition & Architecture/,
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/Contribution & Literature/),
    ).not.toBeInTheDocument();

    await user.selectOptions(count, "4");
    expect(
      screen.getByRole("button", { name: "Adaptive agents — 4 agents" }),
    ).toBeInTheDocument();
  });
});

import { vi } from "vitest";
import { screen } from "@testing-library/react";
import type userEvent from "@testing-library/user-event";
import type { AutoReviewCatalog } from "../lib/reviewCatalogTypes";
import type {
  PipelineConfig,
  ProfileSummary,
  StepConfig,
} from "../lib/workflowTypes";

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

export function makeConfig(): PipelineConfig {
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

export const profiles: ProfileSummary[] = [
  {
    id: "deep-review",
    name: "Paper Review (Full)",
    step_count: 2,
    builtin: true,
  },
];

export function mockLoad(
  config: PipelineConfig,
  extra: Record<string, unknown> = {},
) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd in extra) return Promise.resolve(extra[cmd]);
    if (cmd === "get_pipeline_config") return Promise.resolve(config);
    if (cmd === "list_profiles") return Promise.resolve(profiles);
    if (cmd === "get_active_profile") return Promise.resolve("deep-review");
    if (cmd === "save_pipeline_config") return Promise.resolve();
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

export const macroSpecialist: StepConfig = {
  id: "subject_economics_macro",
  label: "Economics — Macroeconomics",
  prompt: "# Economics — Macroeconomics\n\nDefault host-owned macro prompt.",
  enabled: true,
  phase: "parallel",
  tools: [],
  agents: [],
  after: [],
  context: { include: [] },
};

export const adaptiveCatalog: AutoReviewCatalog = {
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

export async function addBlankStep(
  user: ReturnType<typeof userEvent.setup>,
  name = "Custom Step",
  phase: "parallel" | "sequential" = "parallel",
) {
  await user.click(screen.getByRole("button", { name: "+ Add step" }));
  await user.click(screen.getByRole("tab", { name: "Blank step" }));
  await user.type(screen.getByRole("textbox", { name: "Step name" }), name);
  if (phase === "sequential") {
    await user.click(
      screen.getByRole("radio", { name: /After earlier steps/ }),
    );
  }
  await user.click(screen.getByRole("button", { name: "Create step" }));
}

export async function useAdvancedEditor(
  user: ReturnType<typeof userEvent.setup>,
) {
  await user.click(screen.getByRole("button", { name: "Advanced" }));
}

export function resetPipelinePageMocks() {
  invoke.mockReset();
  confirmDialog.mockReset();
  notify.mockReset();
  dialogMocks.save.mockReset();
  dialogMocks.open.mockReset();
  dialogMocks.save.mockResolvedValue(null);
  dialogMocks.open.mockResolvedValue(null);
}

export { invoke, confirmDialog, notify, dialogMocks };

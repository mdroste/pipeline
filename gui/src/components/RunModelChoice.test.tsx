import { useState } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type {
  ModelCatalog,
  RosterEntry,
  RunModelOverrides,
} from "../lib/reviewModelTypes";
import type { PipelineConfig, StepConfig } from "../lib/workflowTypes";
import type { DepsReport, Settings } from "../lib/settingsTypes";
import RunModelChoice from "./RunModelChoice";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const settings = {
  preferred_provider: "codex",
  default_model: "",
  default_effort: "",
  default_parallel_agents: [],
  default_parallel_model_overrides: {},
  default_parallel_effort_overrides: {},
  anthropic_api_key: "",
  openai_api_key: "",
  google_api_key: "",
  local_base_url: "http://localhost:11434/v1",
  local_model: "",
  local_api_key: "",
} as unknown as Settings;

function catalog(provider: string): ModelCatalog {
  const models =
    provider === "claude"
      ? [{ id: "claude-opus-5-5", display_name: "Opus 5.5" }]
      : provider === "codex"
        ? [{ id: "gpt-5.6-sol", display_name: "GPT-5.6 Sol" }]
        : [];
  return {
    provider,
    transport: "cli",
    source: "test",
    source_version: "test",
    fetched_at: "2026-11-04T00:00:00Z",
    stale: false,
    default_model: models[0]?.id ?? null,
    models: models.map((model) => ({
      ...model,
      description: "",
      is_default: true,
      supported_efforts: ["high", "max"],
      capabilities: [],
      deprecated: false,
    })),
    roles: [],
  };
}

function step(id: string, patch: Partial<StepConfig> = {}): StepConfig {
  return {
    id,
    label: id,
    prompt: "",
    enabled: true,
    phase: "parallel",
    tools: [],
    agents: [],
    context: { include: [] },
    ...patch,
  };
}

function entry(patch: Partial<RosterEntry>): RosterEntry {
  return {
    stage: "orientation",
    label: "Document bundle preparation",
    step_ids: [],
    step_labels: [],
    provider: "codex",
    transport: "cli",
    model: { mode: "automatic" },
    effort: "",
    source: "default",
    note: null,
    ...patch,
  };
}

/** The backend's list: a Settings exception keeps orientation on ChatGPT. */
function roster(overrides: RunModelOverrides | null): RosterEntry[] {
  const review = overrides?.review;
  const follower = review
    ? {
        provider: review.provider,
        model: review.model
          ? { mode: "pinned" as const, model: review.model }
          : { mode: "automatic" as const },
        effort: review.effort,
        source: "review" as const,
      }
    : { source: "default" as const };
  return [
    overrides?.clear_settings_exceptions
      ? entry(follower)
      : entry({ provider: "codex", source: review ? "settings" : "default" }),
    entry({
      stage: "parallel",
      label: "Parallel steps",
      step_ids: ["review"],
      step_labels: ["Technical"],
      ...follower,
    }),
    entry({
      stage: "fallback",
      label: "Usage-limit backup",
      provider: "codex",
      effort: "high",
      source: "settings",
      note: "Only after a usage limit",
    }),
  ];
}

const calls: Array<RunModelOverrides | null> = [];

function mockWorkflow(
  config: Partial<PipelineConfig>,
  resolveRoster = roster,
  savedSettings = settings,
) {
  calls.length = 0;
  invoke.mockImplementation(
    (
      command: string,
      args?: { provider?: string; runParallelOverrides?: RunModelOverrides },
    ) => {
      if (command === "get_settings")
        return Promise.resolve({ settings: savedSettings, warnings: [] });
      if (command === "get_pipeline_config")
        return Promise.resolve({
          merge: { enabled: true, prompt: "", agents: [] },
          ...config,
        });
      if (command === "get_model_catalog")
        return Promise.resolve(catalog(args?.provider ?? "claude"));
      if (command === "get_model_roster") {
        calls.push(args?.runParallelOverrides ?? null);
        return Promise.resolve(
          resolveRoster(args?.runParallelOverrides ?? null),
        );
      }
      return Promise.reject(new Error(`Unexpected command ${command}`));
    },
  );
}

function Harness({
  initial = null,
  disabled = false,
  dependencies,
  localLlmActive,
  onConnectModel,
}: {
  initial?: RunModelOverrides | null;
  disabled?: boolean;
  dependencies?: DepsReport | null;
  localLlmActive?: boolean;
  onConnectModel?: () => void;
} = {}) {
  const [value, setValue] = useState<RunModelOverrides | null>(initial);
  return (
    <RunModelChoice
      value={value}
      onChange={setValue}
      disabled={disabled}
      dependencies={dependencies}
      localLlmActive={localLlmActive}
      onConnectModel={onConnectModel}
    />
  );
}

describe("Review models", () => {
  it("starts with one model for every stage and keeps backup details in Advanced", async () => {
    mockWorkflow({ steps: [step("review")] });
    render(<Harness />);
    expect(
      await screen.findByLabelText("Review models 1 provider"),
    ).toHaveValue("codex");
    await waitFor(() =>
      expect(calls.at(-1)).toMatchObject({
        review: { provider: "codex", model: "", effort: "" },
        agents: [],
        clear_settings_exceptions: true,
        override_workflow_pins: true,
      }),
    );
    expect(
      screen.queryByRole("tab", { name: "Parallel" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Model settings" }),
    ).toBeVisible();
    expect(
      screen.queryByText("One model for the whole review."),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Usage-limit backup")).not.toBeInTheDocument();
    expect(screen.queryByText("Your default model")).not.toBeInTheDocument();
    await userEvent
      .setup()
      .click(screen.getByRole("tab", { name: "Advanced" }));
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Next models" }));
    expect(screen.getByRole("list")).toHaveTextContent("Usage-limit backup");
  });

  it("adds opinions without changing primary and keeps primary edits out of parallel overrides", async () => {
    const user = userEvent.setup();
    mockWorkflow({ steps: [step("review")] });
    render(<Harness />);
    await user.click(await screen.findByRole("button", { name: /Add model/ }));
    await user.click(screen.getByRole("menuitem", { name: "Claude" }));
    await user.selectOptions(
      screen.getByLabelText("Review models Claude model"),
      "pinned:claude-opus-5-5",
    );
    await user.selectOptions(
      screen.getByLabelText("Review models ChatGPT model"),
      "pinned:gpt-5.6-sol",
    );
    await user.selectOptions(
      screen.getByLabelText("Review models ChatGPT thinking"),
      "max",
    );
    expect(calls.at(-1)).toMatchObject({
      review: { provider: "codex", model: "gpt-5.6-sol", effort: "max" },
      agents: ["codex", "claude"],
      model_overrides: {
        "claude:cli": { mode: "pinned", model: "claude-opus-5-5" },
      },
    });
    expect(calls.at(-1)?.model_overrides).not.toHaveProperty("codex:cli");
    expect(calls.at(-1)?.merge_agent).toBeUndefined();
    await user.selectOptions(
      screen.getByLabelText("Review models 1 provider"),
      "antigravity",
    );
    expect(calls.at(-1)).toMatchObject({
      review: { provider: "antigravity", model: "", effort: "" },
      agents: ["antigravity", "claude"],
    });
    expect(calls.at(-1)?.merge_agent).toBeUndefined();
    expect(screen.getByLabelText("Review models 1 provider")).toHaveFocus();
  });

  it("protects the primary, limits the team to three and restores focus after removal", async () => {
    const user = userEvent.setup();
    mockWorkflow({ steps: [step("review")] });
    render(<Harness />);
    await screen.findByLabelText("Review models 1 provider");
    expect(
      screen.queryByRole("button", { name: "Remove model 1" }),
    ).not.toBeInTheDocument();
    for (const provider of ["Claude", "Gemini"]) {
      await user.click(screen.getByRole("button", { name: /Add model/ }));
      await user.click(screen.getByRole("menuitem", { name: provider }));
    }
    expect(screen.getByRole("button", { name: /Add model/ })).toBeDisabled();
    expect(screen.getByLabelText("Review models 3 provider")).toHaveFocus();
    expect(
      within(screen.getByLabelText("Review models 2 provider")).getByRole(
        "option",
        { name: "ChatGPT" },
      ),
    ).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Remove model 2" }));
    expect(calls.at(-1)?.agents).toEqual(["codex", "antigravity"]);
    expect(screen.getByRole("button", { name: /Add model/ })).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Remove model 2" }));
    expect(calls.at(-1)).toMatchObject({
      agents: [],
      model_overrides: {},
      effort_overrides: {},
    });
    expect(screen.getByLabelText("Review models 1 provider")).toHaveValue(
      "codex",
    );
  });

  it("discards a saved combining override and keeps primary edits authoritative", async () => {
    const user = userEvent.setup();
    mockWorkflow({ steps: [step("review")] });
    render(
      <Harness
        initial={{
          review: { provider: "codex", model: "", effort: "" },
          agents: ["codex", "claude"],
          model_overrides: {
            "claude:cli": { mode: "pinned", model: "claude-opus-5-5" },
          },
          effort_overrides: { "claude:cli": "high" },
          clear_settings_exceptions: true,
          override_workflow_pins: true,
          extraction_method: "pdftotext",
          merge_agent: "antigravity",
          merge_model_overrides: {
            "antigravity:cli": { mode: "pinned", model: "old-combiner" },
          },
          merge_effort_overrides: { "antigravity:cli": "high" },
        }}
      />,
    );
    expect(
      await screen.findByText(
        "Primary leads the review and combines feedback.",
      ),
    ).toBeVisible();
    await user.click(await screen.findByRole("tab", { name: "Advanced" }));
    expect(
      screen.queryByRole("checkbox", { name: "Customize combining model" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByLabelText("Combine results 1 provider"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Models" }));
    await user.selectOptions(
      screen.getByLabelText("Review models 1 provider"),
      "antigravity",
    );
    expect(calls.at(-1)).toMatchObject({
      review: { provider: "antigravity", model: "", effort: "" },
      agents: ["antigravity", "claude"],
      model_overrides: {
        "claude:cli": { mode: "pinned", model: "claude-opus-5-5" },
      },
      effort_overrides: { "claude:cli": "high" },
      extraction_method: "pdftotext",
      clear_settings_exceptions: true,
      override_workflow_pins: true,
    });
    for (const call of calls) {
      expect(call).not.toHaveProperty("merge_agent");
      expect(call).not.toHaveProperty("merge_model_overrides");
      expect(call).not.toHaveProperty("merge_effort_overrides");
    }
  });

  it("initializes Primary when an older draft only has combining choices", async () => {
    mockWorkflow({ steps: [step("review")] });
    render(
      <Harness
        initial={{
          agents: [],
          model_overrides: {},
          effort_overrides: {},
          merge_agent: "claude",
          merge_model_overrides: {
            "claude:cli": { mode: "pinned", model: "old-combiner" },
          },
          merge_effort_overrides: { "claude:cli": "high" },
        }}
      />,
    );
    expect(
      await screen.findByLabelText("Review models 1 provider"),
    ).toHaveValue("codex");
    await waitFor(() =>
      expect(calls.at(-1)).toEqual({
        review: { provider: "codex", model: "", effort: "" },
        agents: [],
        model_overrides: {},
        effort_overrides: {},
        clear_settings_exceptions: true,
        override_workflow_pins: true,
      }),
    );
  });

  it("makes saved exceptions opt-in and resets the team without losing PDF options", async () => {
    const user = userEvent.setup();
    mockWorkflow({ steps: [step("review")] });
    render(
      <Harness
        initial={{
          agents: [],
          model_overrides: {},
          effort_overrides: {},
          extraction_method: "pdftotext",
        }}
      />,
    );
    await user.click(await screen.findByRole("tab", { name: "Advanced" }));
    await user.click(
      screen.getByRole("checkbox", {
        name: "Use saved Settings model choices",
      }),
    );
    await user.click(
      screen.getByRole("checkbox", { name: "Use design-pinned models" }),
    );
    expect(calls.at(-1)).toMatchObject({
      clear_settings_exceptions: false,
      override_workflow_pins: false,
    });
    await user.click(screen.getByRole("tab", { name: "Models" }));
    expect(
      screen.getByRole("button", {
        name: "Custom model assignments · Advanced",
      }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Reset" }));
    expect(calls.at(-1)).toMatchObject({
      extraction_method: "pdftotext",
      review: { provider: "codex" },
      agents: [],
      clear_settings_exceptions: true,
      override_workflow_pins: true,
    });
  });

  it("retains older separate parallel choices in Advanced until explicitly reset", async () => {
    const user = userEvent.setup();
    mockWorkflow({ steps: [step("review")] });
    const initial: RunModelOverrides = {
      review: { provider: "codex", model: "", effort: "" },
      agents: ["claude"],
      model_overrides: {
        "claude:cli": { mode: "pinned", model: "older-saved-model" },
      },
      effort_overrides: {},
    };
    render(<Harness initial={initial} />);
    await user.click(
      await screen.findByRole("button", {
        name: "Custom model assignments · Advanced",
      }),
    );
    expect(
      screen.getByLabelText("Custom review models Claude model"),
    ).toHaveValue("pinned:older-saved-model");
    expect(
      calls.every((value) => JSON.stringify(value) === JSON.stringify(initial)),
    ).toBe(true);
    await user.click(screen.getByRole("button", { name: "Use primary model" }));
    expect(calls.at(-1)?.agents).toEqual([]);
  });

  it("supports keyboard navigation and disables edits during preparation", async () => {
    mockWorkflow({ steps: [step("review")] });
    render(<Harness disabled />);
    const tab = await screen.findByRole("tab", { name: "Models" });
    expect(screen.getByLabelText("Review models 1 provider")).toBeDisabled();
    expect(screen.getByRole("button", { name: /Add model/ })).toBeDisabled();
    tab.focus();
    await userEvent.setup().keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Advanced" })).toHaveFocus();
    expect(
      screen.getByRole("checkbox", { name: "Use design-pinned models" }),
    ).toBeDisabled();
  });

  it("does not offer extra reviewers when the workflow has no parallel calls", async () => {
    mockWorkflow({ steps: [step("review", { phase: "sequential" })] });
    render(<Harness />);
    await screen.findByLabelText("Review models 1 provider");
    expect(
      screen.queryByRole("button", { name: /Add model/ }),
    ).not.toBeInTheDocument();
  });
});

it("keeps failed model resolution visible and retries it", async () => {
  mockWorkflow({ steps: [step("review")] });
  const original = invoke.getMockImplementation()!;
  let failing = true;
  invoke.mockImplementation((command, args) =>
    command === "get_model_roster" && failing
      ? Promise.reject(new Error("model service unavailable"))
      : original(command, args),
  );
  render(<Harness />);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "model service unavailable",
  );
  failing = false;
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "Retry model check" }));
  await waitFor(() =>
    expect(screen.queryByRole("alert")).not.toBeInTheDocument(),
  );
  expect(screen.getByLabelText("Review models 1 provider")).toHaveValue(
    "codex",
  );
});
const disconnected: DepsReport = {
  ready: false,
  deps: [
    {
      name: "Claude CLI",
      found: true,
      cli_auth_status: "signed_out",
      version: "",
      path: "",
      required: false,
      hint: "",
    },
    {
      name: "ChatGPT (reviews)",
      found: true,
      authenticated: false,
      version: "",
      path: "",
      required: false,
      hint: "",
    },
  ],
};

it("replaces disconnected model controls with a concise connection action and recovers when connected", async () => {
  mockWorkflow({ steps: [step("review")] });
  const onConnectModel = vi.fn();
  const { rerender } = render(
    <Harness dependencies={disconnected} onConnectModel={onConnectModel} />,
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "No models connected",
  );
  expect(
    screen.queryByLabelText("Review models 1 provider"),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByRole("tab", { name: "Parallel" }),
  ).not.toBeInTheDocument();
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "Connect a model" }));
  expect(onConnectModel).toHaveBeenCalledOnce();
  expect(calls.at(-1)?.clear_settings_exceptions).toBe(true);
  rerender(
    <Harness
      dependencies={{
        ...disconnected,
        deps: disconnected.deps.map((dep) =>
          dep.name === "ChatGPT (reviews)"
            ? { ...dep, authenticated: true }
            : dep,
        ),
      }}
      onConnectModel={onConnectModel}
    />,
  );
  expect(screen.queryByText("No models connected")).not.toBeInTheDocument();
  expect(screen.getByLabelText("Review models 1 provider")).toHaveValue(
    "codex",
  );
});

it.each([
  ["pending readiness", settings, null, false],
  [
    "a saved Gemini API key",
    { ...settings, google_api_key: "test-key" },
    disconnected,
    false,
  ],
  [
    "a ready local model",
    { ...settings, local_model: "test-local" },
    {
      ...disconnected,
      deps: [...disconnected.deps, { name: "Local LLM server", found: true }],
    } as DepsReport,
    true,
  ],
] as const)(
  "does not claim no connections with %s",
  async (_, savedSettings, dependencies, localLlmActive) => {
    mockWorkflow({ steps: [step("review")] }, roster, savedSettings);
    render(
      <Harness dependencies={dependencies} localLlmActive={localLlmActive} />,
    );
    expect(
      await screen.findByLabelText("Review models 1 provider"),
    ).toBeVisible();
    expect(screen.queryByText("No models connected")).not.toBeInTheDocument();
  },
);

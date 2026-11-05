import { useState } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type {
  ModelCatalog,
  PipelineConfig,
  RunParallelOverrides,
  Settings,
  StepConfig,
} from "../lib/types";
import RunParallelAgents from "./RunParallelAgents";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const settings = {
  preferred_provider: "claude",
  default_parallel_agents: ["claude"],
  default_parallel_model_overrides: {},
  default_parallel_effort_overrides: {},
  claude_model: "",
  claude_effort: "",
  codex_model: "",
  codex_effort: "",
  antigravity_effort: "",
  anthropic_api_key: "",
  openai_api_key: "",
  google_api_key: "",
  local_base_url: "http://localhost:11434/v1",
  local_model: "",
  local_api_key: "",
} as Settings;

function catalog(provider: string): ModelCatalog {
  return {
    provider,
    transport: "cli",
    source: "test",
    source_version: "test",
    fetched_at: "2026-08-11T00:00:00Z",
    stale: false,
    models:
      provider === "codex"
        ? [
            {
              id: "gpt-exact",
              display_name: "GPT Exact",
              description: "",
              is_default: false,
              supported_efforts: ["high"],
              capabilities: [],
              deprecated: false,
            },
          ]
        : [],
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

function mockWorkflow(config: Partial<PipelineConfig>) {
  invoke.mockImplementation((command: string, args?: { provider?: string }) => {
    if (command === "get_settings")
      return Promise.resolve({ settings, warnings: [] });
    if (command === "get_pipeline_config")
      return Promise.resolve({
        merge: { enabled: true, prompt: "", agents: [] },
        ...config,
      });
    if (command === "get_model_catalog")
      return Promise.resolve(catalog(args?.provider ?? "claude"));
    return Promise.reject(new Error(`Unexpected command ${command}`));
  });
}

function Harness() {
  const [value, setValue] = useState<RunParallelOverrides | null>(null);
  return <RunParallelAgents value={value} onChange={setValue} />;
}

describe("RunParallelAgents", () => {
  it("inherits Settings, hides an unconfigured local provider, and pins a per-run model", async () => {
    const user = userEvent.setup();
    mockWorkflow({ steps: [step("review")] });

    render(<Harness />);
    expect(await screen.findByText(/Claude · Settings default/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Change" }));
    expect(
      screen.queryByRole("button", { name: "Local" }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "ChatGPT" }));
    const model = await screen.findByLabelText(
      "Agents for this report ChatGPT model",
    );
    await waitFor(() =>
      expect(
        screen.getByRole("option", {
          name: "GPT Exact",
        }),
      ).toBeInTheDocument(),
    );
    await user.selectOptions(model, "pinned:gpt-exact");

    expect(model).toHaveValue("pinned:gpt-exact");
    expect(screen.getByText("Claude + ChatGPT")).toBeVisible();
    expect(screen.getByText("Merge · Claude · Settings default")).toBeVisible();
    expect(
      screen.getByLabelText("Merge for this report Claude model"),
    ).toBeVisible();
    const mergeProviders = screen.getByRole("group", {
      name: "Merge for this report providers",
    });
    await user.click(
      within(mergeProviders).getByRole("button", { name: "ChatGPT" }),
    );
    expect(screen.getByText("Merge · ChatGPT · This report")).toBeVisible();
    expect(screen.getByRole("button", { name: "Reset" })).toBeVisible();
  });

  it("names the workflow when its parallel steps choose their own providers", async () => {
    const user = userEvent.setup();
    mockWorkflow({
      steps: [step("review", { agents: ["codex", "antigravity"] })],
    });

    render(<Harness />);
    expect(
      await screen.findByText("ChatGPT + Gemini · Workflow"),
    ).toBeVisible();
    expect(screen.getByText("Merge · Claude · Settings default")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Change" }));
    expect(
      screen.getByText(/replacing this workflow’s choices for this report/),
    ).toBeVisible();
  });

  it("does not offer one list when parallel steps differ", async () => {
    mockWorkflow({
      steps: [step("first", { agents: ["codex"] }), step("second")],
    });

    render(<Harness />);
    expect(await screen.findByText("Varies by step")).toBeVisible();
  });

  it("stays out of the way when no step can use the choice", async () => {
    mockWorkflow({
      steps: [
        step("draft", { phase: "sequential" }),
        step("check", { kind: "analyzer" }),
        step("off", { enabled: false }),
      ],
    });

    const { container } = render(<Harness />);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("get_pipeline_config"),
    );
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(screen.queryByText("Parallel agents")).not.toBeInTheDocument();
  });

  it("stacks its menus so model names stay readable in the sidebar", async () => {
    const user = userEvent.setup();
    mockWorkflow({ steps: [step("review", { agents: ["claude"] })] });

    render(<Harness />);
    await user.click(await screen.findByRole("button", { name: "Change" }));
    const model = screen.getByLabelText("Agents for this report Claude model");
    expect(model.parentElement).toHaveClass("grid-cols-1");
    expect(model.parentElement).not.toHaveClass("grid-cols-2");
  });
});

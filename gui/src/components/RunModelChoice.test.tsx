import { useState } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type {
  ModelCatalog,
  PipelineConfig,
  RosterEntry,
  RunModelOverrides,
  Settings,
  StepConfig,
} from "../lib/types";
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
    label: "Orientation map",
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

function mockWorkflow(config: Partial<PipelineConfig>) {
  calls.length = 0;
  invoke.mockImplementation(
    (
      command: string,
      args?: { provider?: string; runParallelOverrides?: RunModelOverrides },
    ) => {
      if (command === "get_settings")
        return Promise.resolve({ settings, warnings: [] });
      if (command === "get_pipeline_config")
        return Promise.resolve({
          merge: { enabled: true, prompt: "", agents: [] },
          ...config,
        });
      if (command === "get_model_catalog")
        return Promise.resolve(catalog(args?.provider ?? "claude"));
      if (command === "get_model_roster") {
        calls.push(args?.runParallelOverrides ?? null);
        return Promise.resolve(roster(args?.runParallelOverrides ?? null));
      }
      return Promise.reject(new Error(`Unexpected command ${command}`));
    },
  );
}

function Harness() {
  const [value, setValue] = useState<RunModelOverrides | null>(null);
  return <RunModelChoice value={value} onChange={setValue} />;
}

describe("RunModelChoice", () => {
  it("names the default model the review starts from", async () => {
    mockWorkflow({ steps: [step("review")] });
    render(<Harness />);
    expect(
      await screen.findByText("GPT-5.6 Sol (ChatGPT's choice)"),
    ).toBeVisible();
    expect(screen.getByText(/Your default model/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Change" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(await screen.findByText(/ChatGPT · GPT-5.6 Sol/)).toBeVisible();
  });

  it("covers the whole review and lists what does not follow it", async () => {
    const user = userEvent.setup();
    mockWorkflow({ steps: [step("review")] });
    render(<Harness />);
    await user.click(await screen.findByRole("button", { name: "Change" }));

    const providers = screen.getByRole("group", {
      name: "Review model providers",
    });
    await user.click(within(providers).getByRole("button", { name: "Claude" }));
    const model = screen.getByLabelText("Review model Claude model");
    await waitFor(() =>
      expect(
        within(model).getByRole("option", { name: "Opus 5.5" }),
      ).toBeInTheDocument(),
    );
    await user.selectOptions(model, "pinned:claude-opus-5-5");
    await user.selectOptions(
      screen.getByLabelText("Review model Claude thinking"),
      "max",
    );

    await waitFor(() =>
      expect(calls.at(-1)?.review).toEqual({
        provider: "claude",
        model: "claude-opus-5-5",
        effort: "max",
      }),
    );
    // Orientation still follows a Settings exception, and says so.
    const note = await screen.findByRole("note", {
      name: "Calls on other models",
    });
    expect(note).toHaveTextContent(
      "Orientation map → ChatGPT · GPT-5.6 Sol (ChatGPT's choice) · ChatGPT default thinking (Settings exception)",
    );

    await user.click(
      within(note).getByRole("button", {
        name: "Use the review model for everything",
      }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("note", { name: "Calls on other models" }),
      ).not.toBeInTheDocument(),
    );
    expect(calls.at(-1)).toMatchObject({
      clear_settings_exceptions: true,
      override_workflow_pins: true,
    });
    await user.click(screen.getByText("Advanced options"));
    const list = screen.getByRole("list", {
      name: "Every model this review calls",
    });
    expect(list).toHaveTextContent("Claude · Opus 5.5 · Max");
    expect(screen.getByRole("button", { name: "Reset" })).toBeVisible();
  });

  it("offers second-opinion models for parallel steps", async () => {
    const user = userEvent.setup();
    mockWorkflow({ steps: [step("review")] });
    render(<Harness />);
    await user.click(await screen.findByRole("button", { name: "Change" }));
    await user.click(screen.getByText("Advanced options"));
    await user.click(
      screen.getByRole("checkbox", {
        name: /Choose the models for parallel steps/,
      }),
    );
    const parallel = screen.getByRole("group", {
      name: "Parallel steps providers",
    });
    await user.click(within(parallel).getByRole("button", { name: "Claude" }));
    await waitFor(() =>
      expect(calls.at(-1)?.agents).toEqual(["codex", "claude"]),
    );
    expect(
      screen.getByLabelText("Combine results ChatGPT model"),
    ).toBeVisible();
  });
});

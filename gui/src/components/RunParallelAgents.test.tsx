import { useState } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ModelCatalog, RunParallelOverrides, Settings } from "../lib/types";
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
    models: provider === "codex" ? [{
      id: "gpt-exact",
      display_name: "GPT Exact",
      description: "",
      is_default: false,
      supported_efforts: ["high"],
      capabilities: [],
      deprecated: false,
    }] : [],
    roles: [],
  };
}

function Harness() {
  const [value, setValue] = useState<RunParallelOverrides | null>(null);
  return <RunParallelAgents value={value} onChange={setValue} />;
}

describe("RunParallelAgents", () => {
  it("inherits Settings, hides an unconfigured local provider, and pins a per-run model", async () => {
    const user = userEvent.setup();
    invoke.mockImplementation((command: string, args?: { provider?: string }) => {
      if (command === "get_settings") return Promise.resolve({ settings, warnings: [] });
      if (command === "get_model_catalog") return Promise.resolve(catalog(args?.provider ?? "claude"));
      return Promise.reject(new Error(`Unexpected command ${command}`));
    });

    render(<Harness />);
    expect(await screen.findByText(/Claude · Settings default/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Change" }));
    expect(screen.queryByRole("button", { name: "Local" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "ChatGPT" }));
    const model = await screen.findByLabelText("Agents for this report ChatGPT model");
    await waitFor(() => expect(
      screen.getByRole("option", { name: "GPT Exact · Agents for this report" }),
    ).toBeInTheDocument());
    await user.selectOptions(model, "pinned:gpt-exact");

    expect(model).toHaveValue("pinned:gpt-exact");
    expect(screen.getByText("Claude + ChatGPT")).toBeVisible();
    expect(screen.getByRole("button", { name: "Reset" })).toBeVisible();
  });
});

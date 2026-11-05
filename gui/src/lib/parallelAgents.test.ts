import { describe, expect, it } from "vitest";
import { workflowParallelAgents } from "./parallelAgents";
import type { PipelineConfig, Settings, StepConfig } from "./types";

const settings = {
  preferred_provider: "claude",
  default_parallel_agents: ["claude", "codex"],
  default_parallel_model_overrides: {
    "claude:cli": { mode: "role", role: "quality" },
  },
  default_parallel_effort_overrides: { "claude:cli": "high" },
} as unknown as Settings;

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

const workflow = (...steps: StepConfig[]) =>
  ({ steps }) as unknown as PipelineConfig;

describe("workflowParallelAgents", () => {
  it("uses the Settings defaults for steps that leave agents empty", () => {
    expect(workflowParallelAgents(workflow(step("a")), settings)).toEqual({
      applies: true,
      source: "settings",
      agents: ["claude", "codex"],
      multiple: true,
      modelOverrides: { "claude:cli": { mode: "role", role: "quality" } },
      effortOverrides: { "claude:cli": "high" },
    });
  });

  it("reports a workflow's own providers and model choices", () => {
    const pinned = {
      agents: ["antigravity"],
      model_overrides: {
        "antigravity:api": { mode: "pinned", model: "gemini" },
      },
      effort_overrides: {},
    } satisfies Partial<StepConfig>;
    const result = workflowParallelAgents(
      workflow(step("a", pinned), step("b", pinned)),
      settings,
    );
    expect(result).toMatchObject({
      source: "workflow",
      agents: ["antigravity"],
      multiple: false,
      modelOverrides: pinned.model_overrides,
      effortOverrides: {},
    });
  });

  it("treats steps as a shared choice regardless of provider order", () => {
    expect(
      workflowParallelAgents(
        workflow(
          step("a", { agents: ["codex", "claude"] }),
          step("b", { agents: ["claude", "codex"] }),
        ),
        settings,
      ).source,
    ).toBe("workflow");
  });

  it("does not show one model when steps choose different ones", () => {
    const result = workflowParallelAgents(
      workflow(
        step("a", { agents: ["codex"] }),
        step("b", {
          agents: ["codex"],
          model_overrides: { "codex:cli": { mode: "pinned", model: "sol" } },
        }),
      ),
      settings,
    );
    expect(result.source).toBe("workflow");
    expect(result.modelOverrides).toEqual({});
  });

  it("marks differing provider lists as mixed", () => {
    const result = workflowParallelAgents(
      workflow(step("a", { agents: ["antigravity"] }), step("b")),
      settings,
    );
    expect(result).toMatchObject({
      source: "mixed",
      agents: ["antigravity", "claude", "codex"],
      multiple: true,
    });
  });

  it("does not apply to disabled, sequential, or analyzer steps", () => {
    expect(
      workflowParallelAgents(
        workflow(
          step("off", { enabled: false }),
          step("next", { phase: "sequential" }),
          step("check", { kind: "analyzer" }),
        ),
        settings,
      ).applies,
    ).toBe(false);
  });
});

import { defaultParallelAgents } from "./providers";
import type { ModelSelection, PipelineConfig, Settings } from "./types";

/** What a workflow's parallel steps run on before any one-review change. */
export interface WorkflowParallelAgents {
  /** False when the workflow has no step a provider choice can reach. */
  applies: boolean;
  /** "mixed" means steps differ, so one provider list can't describe them. */
  source: "settings" | "workflow" | "mixed";
  /** Every provider in use across the steps. */
  agents: string[];
  /** At least one step runs several providers, so a merge may follow it. */
  multiple: boolean;
  modelOverrides: Record<string, ModelSelection>;
  effortOverrides: Record<string, string>;
}

const canonical = (value: Record<string, unknown>) =>
  JSON.stringify(
    Object.entries(value).sort(([left], [right]) => left.localeCompare(right)),
  );

/** One control can show a choice only when every step shares it. */
function shared<T>(choices: Record<string, T>[]): Record<string, T> {
  const first = choices[0] ?? {};
  return choices.every((choice) => canonical(choice) === canonical(first))
    ? first
    : {};
}

/**
 * Mirrors the backend's `apply_agent_defaults`: a step's own agents win, and a
 * step that leaves them empty takes the Settings defaults. Analyzer steps make
 * no provider call, so they never count.
 */
export function workflowParallelAgents(
  config: PipelineConfig,
  settings: Settings,
): WorkflowParallelAgents {
  const steps = config.steps.filter(
    (step) =>
      step.enabled && step.phase === "parallel" && step.kind !== "analyzer",
  );
  const inherited = defaultParallelAgents(settings);
  const lists = steps.map((step) =>
    step.agents.length ? step.agents : inherited,
  );
  const agents = [...new Set(lists.flat())];
  const uniform = lists.every((list) => list.length === agents.length);
  return {
    applies: steps.length > 0,
    source: !uniform
      ? "mixed"
      : steps.some((step) => step.agents.length)
        ? "workflow"
        : "settings",
    agents,
    multiple: lists.some((list) => list.length > 1),
    modelOverrides: shared(
      steps.map((step) =>
        step.agents.length
          ? (step.model_overrides ?? {})
          : {
              ...settings.default_parallel_model_overrides,
              ...step.model_overrides,
            },
      ),
    ),
    effortOverrides: shared(
      steps.map((step) =>
        step.agents.length
          ? (step.effort_overrides ?? {})
          : {
              ...settings.default_parallel_effort_overrides,
              ...step.effort_overrides,
            },
      ),
    ),
  };
}

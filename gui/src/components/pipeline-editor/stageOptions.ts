import {
  adaptiveAgentCount,
  adaptiveAgentCountLabel,
  adaptiveAgentRange,
  isAutoReview,
} from "../../lib/autoReview";
import type { PipelineConfig } from "../../lib/workflowTypes";
import type { SelectOption } from "../../ui/Select";

/** The picker also lists stages absent from the execution graph, including disabled reviewers. */
export function stageOptions(config: PipelineConfig): SelectOption[] {
  const options: SelectOption[] = [
    {
      value: "extraction",
      label: "Input & extraction",
      description: "Review setup",
    },
    {
      value: "orientation",
      label: "Document bundle preparation",
      description: "Review setup",
    },
  ];
  for (const phase of ["parallel", "sequential"] as const) {
    const steps = config.steps.filter((s) => s.phase === phase);
    for (const step of steps)
      options.push({
        value: step.id,
        label: step.label,
        description: [
          phase === "parallel" ? "Parallel" : "Sequential",
          !step.enabled && "Disabled",
        ]
          .filter(Boolean)
          .join(" · "),
      });
    if (phase === "parallel") {
      if (isAutoReview(config))
        options.push({
          value: "auto_adaptive_agents",
          label: "Adaptive agents",
          description:
            adaptiveAgentCountLabel(
              adaptiveAgentCount(config),
              adaptiveAgentRange(config),
            ) + " agents",
        });
      if (steps.some((s) => s.agents.length > 1))
        options.push({
          value: "merge",
          label: "Merge (auto)",
          description: config.merge.enabled ? "Combine results" : "Disabled",
        });
    }
  }
  return options;
}

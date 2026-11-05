// Presentation model for the Review overview. It turns a review's steps into
// the numbered stages the overview draws, using the same wave scheduler
// as the backend plan. Run progress numbers its rows the same way.

import { computeWaves } from "./pipelineHelpers";
import { adaptiveAgentCountLabel, type AdaptiveAgentRange } from "./autoReview";
import { PROVIDER_LABELS, type Provider } from "./providers";
import type { MergeConfig, StepConfig } from "./types";

/** A selectable overview node: a step id or one of the pseudo-stages. */
export type WaveSelection =
  | string
  | "merge"
  | "pipeline_settings"
  | "extraction"
  | "orientation"
  | "auto_adaptive_agents";

export interface OverviewNode {
  id: WaveSelection;
  label: string;
  /** Short description shown under or beside the label. */
  caption: string;
  agents: string[];
  tools: string[];
  /** Upper bound of a for_each fan-out, when the step fans out. */
  fanOut: number | null;
  /** Human description of a run_if guard, when the step has one. */
  condition: string | null;
  /** Agent-count label for the auto-selected specialists node. */
  adaptive?: string;
}

export type OverviewStageKind = "prepare" | "parallel" | "single" | "merge";

export interface OverviewStage {
  key: string;
  kind: OverviewStageKind;
  /** Position in the run (1 = preparation); null for merge. */
  number: number | null;
  title: string;
  caption: string;
  nodes: OverviewNode[];
}

export interface OverviewModel {
  stages: OverviewStage[];
  stepCount: number;
  providerCalls: number;
  adaptive: boolean;
  adaptiveLabel: string;
}

export interface OverviewInput {
  steps: StepConfig[];
  merge: MergeConfig;
  /** Extraction input mode: "" | "document" | "folder" | "none". */
  inputMode?: string;
  adaptiveReview?: boolean;
  adaptiveAgentCount?: number | null;
  adaptiveAgentRange?: AdaptiveAgentRange;
}

export type PreparationKind = "document" | "source_tree" | "context";

export function preparationKind(inputMode?: string): PreparationKind {
  const mode = (inputMode ?? "").trim();
  if (mode === "folder" || mode === "source_tree") return "source_tree";
  if (mode === "none") return "context";
  return "document";
}

/** Title of the combined extract + orient stage. A folder is never called a
 *  document bundle, matching the backend's input_processing_label. */
export function preparationTitle(kind: PreparationKind): string {
  if (kind === "source_tree") return "Prepare source-tree inventory";
  if (kind === "context") return "Prepare review context";
  return "Prepare document bundle";
}

export function agentName(agent: string): string {
  return PROVIDER_LABELS[agent as Provider] ?? agent;
}

function conditionText(
  step: StepConfig,
  labels: Map<string, string>,
): string | null {
  const condition = step.run_if;
  if (!condition) return null;
  if (condition.kind === "output_matches") {
    const source = labels.get(condition.step) ?? condition.step;
    return `Runs only if ${source} ${condition.negate ? "does not match" : "matches"} “${condition.pattern}”`;
  }
  return `Runs only if the survey field ${condition.pointer} matches`;
}

function systemNode(
  id: WaveSelection,
  label: string,
  caption: string,
  agents: string[] = [],
): OverviewNode {
  return {
    id,
    label,
    caption,
    agents,
    tools: [],
    fanOut: null,
    condition: null,
  };
}

export function buildOverview({
  steps,
  merge,
  inputMode,
  adaptiveReview = false,
  adaptiveAgentCount = null,
  adaptiveAgentRange,
}: OverviewInput): OverviewModel {
  const waves = computeWaves(steps, false);
  const enabled = steps.filter((step) => step.enabled);
  const labels = new Map(steps.map((step) => [step.id, step.label]));
  // A Pipeline check makes no provider call.
  const modelSteps = enabled.filter((step) => step.kind !== "analyzer");
  const stepCalls = modelSteps.reduce(
    (total, step) =>
      total + Math.max(1, step.agents?.length ?? 0) * (step.for_each?.max ?? 1),
    0,
  );
  const mergeCalls = merge.enabled
    ? modelSteps.reduce(
        (total, step) =>
          total +
          ((step.agents?.length ?? 0) > 1 ? (step.for_each?.max ?? 1) : 0),
        0,
      )
    : 0;
  const adaptiveLabel = adaptiveAgentCountLabel(
    adaptiveAgentCount,
    adaptiveAgentRange,
  );

  const preparation = preparationKind(inputMode);
  const stages: OverviewStage[] = [
    {
      key: "prepare",
      kind: "prepare",
      number: 1,
      title: preparationTitle(preparation),
      caption: "",
      nodes: [
        systemNode(
          "extraction",
          "Extract",
          preparation === "source_tree"
            ? "Inventory the source files"
            : preparation === "context"
              ? "Collect review inputs"
              : "Text, tables, and layout",
        ),
        adaptiveReview
          ? systemNode(
              "orientation",
              "Orient + classify",
              "Map the paper, choose specialists",
            )
          : systemNode("orientation", "Orient", "Shared map of the paper"),
      ],
    },
  ];

  const stepNode = (step: StepConfig): OverviewNode => {
    const analyzer = step.kind === "analyzer";
    const agents = analyzer ? [] : (step.agents ?? []);
    return {
      id: step.id,
      label: step.label,
      caption: analyzer ? "Pipeline check" : agents.map(agentName).join(" + "),
      agents,
      tools: step.tools ?? [],
      fanOut: step.for_each ? step.for_each.max : null,
      condition: conditionText(step, labels),
    };
  };

  let firstParallel = true;
  waves.forEach((wave, index) => {
    const number = index + 2;
    const key = `wave-${index + 1}`;
    if (wave.kind === "parallel") {
      const nodes = wave.steps.map(stepNode);
      if (adaptiveReview && firstParallel) {
        nodes.push({
          ...systemNode(
            "auto_adaptive_agents",
            "Adaptive agents",
            `${adaptiveLabel} auto-selected after orientation`,
          ),
          adaptive: adaptiveLabel,
        });
      }
      firstParallel = false;
      stages.push(
        nodes.length > 1
          ? {
              key,
              kind: "parallel",
              number,
              title: "In parallel",
              caption: `${nodes.length} at once`,
              nodes,
            }
          : {
              key,
              kind: "single",
              number,
              title: nodes[0].label,
              caption: "",
              nodes,
            },
      );
      if (wave.hasMultiAgent && merge.enabled) {
        stages.push({
          key: `merge-${index + 1}`,
          kind: "merge",
          number: null,
          title: "Merge",
          caption: "Reconcile each step's reviewers",
          nodes: [
            systemNode(
              "merge",
              "Merge",
              "Reconcile each step's reviewers",
              merge.agents ?? [],
            ),
          ],
        });
      }
    } else {
      stages.push({
        key,
        kind: "single",
        number,
        title: wave.step.label,
        caption: "",
        nodes: [stepNode(wave.step)],
      });
    }
  });

  return {
    stages,
    stepCount: enabled.length,
    providerCalls: stepCalls + mergeCalls,
    adaptive: adaptiveReview,
    adaptiveLabel,
  };
}

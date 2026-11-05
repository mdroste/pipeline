import { stepDependencies } from "../../lib/pipelineHelpers";
import type {
  PipelineConfig,
  StepConfig,
  ArtifactSelector,
  DependencyPolicy,
  ExtractionConfig,
  OutputConfig,
} from "../../lib/types";
export const DEFAULT_EXTRACTION: ExtractionConfig = { method: "" };
const PRESERVE_FINDINGS_KEY = "x-pipeline-preserve-findings-from";
/** The save validator's limit. Labels may be longer, so ids are cut to fit. */
export const MAX_STEP_ID_CHARS = 64;
export const MAX_STEP_LABEL_CHARS = 200;

/** `base`, shortened to the id limit and numbered when already in use. */
export function uniqueStepId(base: string, steps: StepConfig[]) {
  const fitted = (suffix: string) => {
    const room = MAX_STEP_ID_CHARS - suffix.length;
    return base.length > room
      ? `${base.slice(0, room).replace(/_+$/, "")}${suffix}`
      : `${base}${suffix}`;
  };
  const taken = new Set(steps.map((step) => step.id));
  let id = fitted("");
  for (let number = 2; taken.has(id); number += 1) id = fitted(`_${number}`);
  return id;
}

export function prepareExtractionChange(
  config: PipelineConfig,
  patch: Partial<ExtractionConfig>,
) {
  const previous = config.extraction ?? DEFAULT_EXTRACTION;
  const extraction = { ...previous, ...patch };
  const previousInputs = previous.extra_inputs ?? [];
  const nextInputs = extraction.extra_inputs ?? [];
  const renamedInputs = new Map<string, string>();
  if (previousInputs.length === nextInputs.length) {
    previousInputs.forEach((slot, index) => {
      const nextKey = nextInputs[index]?.key;
      if (nextKey !== undefined && slot.key !== nextKey) {
        renamedInputs.set(slot.key, nextKey);
      }
    });
  }
  const namedKeys = new Set(
    (extraction.extra_inputs ?? []).map((slot) => slot.key),
  );
  let removedSelectors = 0;
  const steps = config.steps.map((step) => {
    const include = step.context.include
      .map((selector): ArtifactSelector => {
        if (
          selector.kind === "named_input" &&
          renamedInputs.has(selector.key)
        ) {
          return { ...selector, key: renamedInputs.get(selector.key)! };
        }
        return selector;
      })
      .filter((selector) => {
        const remove =
          (selector.kind === "primary" && extraction.input_mode === "none") ||
          (selector.kind === "named_input" && !namedKeys.has(selector.key));
        if (remove) removedSelectors += 1;
        return !remove;
      });
    return { ...step, context: { include } };
  });
  return { config: { ...config, extraction, steps }, removedSelectors };
}

export function referencingSteps(config: PipelineConfig, id: string) {
  return config.steps.filter(
    (step) =>
      stepDependencies(step).includes(id) ||
      (step.run_if?.kind === "output_matches" && step.run_if.step === id) ||
      step.output_schema?.[PRESERVE_FINDINGS_KEY] === id,
  );
}
/**
 * Steps that repeat over this step's results. Without that source the fan-out
 * has no items and its prompt no `{item}`, so the researcher chooses another
 * source rather than the editor quietly turning it into a single run.
 */
export function fanOutSteps(steps: StepConfig[], id: string) {
  return steps.filter((step) => step.for_each?.artifact?.step === id);
}
export function namedOutputs(config: PipelineConfig, id: string) {
  return (config.outputs?.named ?? []).filter((product) => product.step === id);
}
function withoutPolicyReference(
  policy: DependencyPolicy,
  id: string,
): DependencyPolicy {
  const quorum = (policy.quorum ?? []).filter((member) => member !== id);
  return {
    required: (policy.required ?? []).filter((member) => member !== id),
    quorum,
    minimum_successes: Math.min(policy.minimum_successes ?? 0, quorum.length),
  };
}
export function withoutStepReferences(
  step: StepConfig,
  id: string,
): StepConfig {
  const { [PRESERVE_FINDINGS_KEY]: preserved, ...schema } =
    step.output_schema ?? {};
  return {
    ...step,
    after: (step.after ?? []).filter((dependency) => dependency !== id),
    ...(step.dependency_policy && {
      dependency_policy: withoutPolicyReference(step.dependency_policy, id),
    }),
    context: {
      include: step.context.include.filter(
        (selector) => selector.kind !== "step" || selector.step !== id,
      ),
    },
    run_if:
      step.run_if?.kind === "output_matches" && step.run_if.step === id
        ? null
        : step.run_if,
    ...(preserved === id && { output_schema: schema }),
  };
}
export function withoutStepOutputs(
  outputs: OutputConfig | undefined,
  id: string,
): OutputConfig {
  return {
    ...(outputs ?? {}),
    primary_step: outputs?.primary_step === id ? "" : outputs?.primary_step,
    findings_step: outputs?.findings_step === id ? "" : outputs?.findings_step,
    ...(outputs?.named && {
      named: outputs.named.filter((product) => product.step !== id),
    }),
  };
}

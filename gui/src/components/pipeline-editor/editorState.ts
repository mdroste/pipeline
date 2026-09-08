import type {
  PipelineConfig,
  StepConfig,
  ArtifactSelector,
  ExtractionConfig,
} from "../../lib/types";
export const DEFAULT_EXTRACTION: ExtractionConfig = { method: "" };

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
      (step.after ?? []).includes(id) ||
      step.context.include.some(
        (selector) => selector.kind === "step" && selector.step === id,
      ) ||
      (step.run_if?.kind === "output_matches" && step.run_if.step === id),
  );
}
export function withoutStepReferences(
  step: StepConfig,
  id: string,
): StepConfig {
  return {
    ...step,
    after: (step.after ?? []).filter((dependency) => dependency !== id),
    context: {
      include: step.context.include.filter(
        (selector) => selector.kind !== "step" || selector.step !== id,
      ),
    },
    run_if:
      step.run_if?.kind === "output_matches" && step.run_if.step === id
        ? null
        : step.run_if,
  };
}

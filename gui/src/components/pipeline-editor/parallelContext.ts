import type { ArtifactSelector, StepContext } from "../../lib/types";

import { primaryPartsForMode } from "./utils";

/**
 * Context for a step that is switching to the parallel phase. Parallel steps
 * run independently, so validation rejects any selection of another step's
 * output, and the context editor hides those selections. Removing them keeps
 * the workflow saveable. When a removal would leave the step without its
 * primary input, the primary input is added, as in a new parallel step.
 */
export function contextForParallel(context: StepContext, inputMode: string) {
  const kept = context.include.filter((selector) => selector.kind !== "step");
  const removed = context.include.length - kept.length;
  const primary = primaryPartsForMode(inputMode);
  const addsPrimary =
    removed > 0 &&
    primary.length > 0 &&
    !kept.some((selector) => selector.kind === "primary");
  const include: ArtifactSelector[] = addsPrimary
    ? [{ kind: "primary", parts: primary }, ...kept]
    : kept;
  return { context: { ...context, include }, removed, addsPrimary };
}

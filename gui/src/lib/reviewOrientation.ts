import type { OrientationMap } from "./reviewOrientationTypes";

/** True when a survey JSON has the paper-review orientation shape. */
export function isPaperOrientation(o: unknown): o is OrientationMap {
  if (!o || typeof o !== "object") return false;
  const v = o as Record<string, unknown>;
  const meta = v.metadata as Record<string, unknown> | undefined;
  return (
    (typeof meta === "object" && meta !== null && "paper_type" in meta) ||
    "formal_results" in v ||
    "stated_contribution" in v
  );
}

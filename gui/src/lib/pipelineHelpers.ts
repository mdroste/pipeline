// Pipeline editor helpers: placeholder catalog, lint detection, wave structure.
// Kept separate from PipelinePage.tsx so the rules can be unit-tested or reused
// by future UI surfaces (e.g. a step-test runner) without lifting the whole page.

import type { Phase, StepConfig } from "./types";

// ── Placeholder catalog ─────────────────────────────────────────────

export interface PlaceholderEntry {
  /** Literal token to insert, e.g. "{prior_outputs}" or "{step:technical}". */
  token: string;
  /** Short user-facing description shown in tooltips/chips. */
  description: string;
}

/** Editor surface that determines which placeholders are valid. */
export type PromptContext =
  | { kind: "parallel" }                     // step.prompt for a Parallel step (no auto-expansion)
  | { kind: "sequential"; otherStepIds: string[] }  // step.prompt for a Sequential step
  | { kind: "parallel_template" }            // pipeline-level wrapping template
  | { kind: "merge" }                        // merge.prompt
  | { kind: "orientation" };                 // orientation-map prompt

/** Return the placeholders that the executor will substitute in this context. */
export function placeholdersFor(ctx: PromptContext): PlaceholderEntry[] {
  switch (ctx.kind) {
    case "parallel":
      // The step's own prompt is dropped into {step_prompt} in the wrapping
      // template; no per-step substitution is performed on it.
      return [];
    case "sequential": {
      const base: PlaceholderEntry[] = [
        { token: "{orientation}", description: "Path to orientation map JSON" },
        { token: "{prior_outputs}", description: "All prior step outputs concatenated" },
        { token: "{last_output}", description: "Most recent prior step's output" },
        { token: "{input_path}", description: "Path to the extracted input text" },
        { token: "{paper_path}", description: "Alias of {input_path} (legacy)" },
        { token: "{source_path}", description: "Path to the original input (PDF, .tex, folder)" },
      ];
      const stepRefs: PlaceholderEntry[] = ctx.otherStepIds.map((id) => ({
        token: `{step:${id}}`,
        description: `Output of step "${id}" only`,
      }));
      return [...base, ...stepRefs];
    }
    case "parallel_template":
      return [
        { token: "{step_prompt}", description: "The individual step's prompt" },
        { token: "{orientation}", description: "Survey / orientation map reference block" },
        { token: "{input_path}", description: "Path to the extracted input text" },
        { token: "{paper_type}", description: "theory / empirical / mixed (paper surveys only; empty otherwise)" },
        { token: "{figure_hint}", description: "Figure/table access instructions" },
        { token: "{output_format}", description: "Output delivery instructions (file write or markers)" },
        { token: "{paper_path}", description: "Alias of {input_path} (legacy)" },
      ];
    case "merge":
      return [
        { token: "{topic}", description: "Step label being merged" },
        { token: "{agent_reports}", description: "All agent analyses for this step" },
      ];
    case "orientation":
      return [
        { token: "{input_text}", description: "The extracted input text (truncated to 250k chars)" },
        { token: "{paper_text}", description: "Alias of {input_text} (legacy)" },
      ];
  }
}

// ── Linting ─────────────────────────────────────────────────────────

const PLACEHOLDER_RE = /\{([a-zA-Z][a-zA-Z0-9_]*(?::[a-zA-Z0-9_./-]+)?)\}/g;

export interface LintHit {
  match: string;     // The full "{...}" token as it appears.
  index: number;     // Character offset in the source text.
  line: number;      // 1-based line number of the match.
}

/**
 * Find placeholder-shaped tokens that won't be substituted in the given context.
 *
 * We're deliberately permissive: the regex only flags `{name}` or `{name:value}`
 * tokens that look intentional. Unrelated curly-brace content (LaTeX, JSON
 * snippets, etc.) is preserved as-is by the executor and doesn't need warning.
 */
export function findUnknownPlaceholders(
  text: string,
  ctx: PromptContext,
): LintHit[] {
  const valid = new Set(placeholdersFor(ctx).map((p) => p.token));
  // Backward-compatible aliases the executor still accepts:
  if (ctx.kind === "sequential") {
    valid.add("{referee_reports}");
    valid.add("{editor_synthesis}");
  }
  const hits: LintHit[] = [];
  for (const m of text.matchAll(PLACEHOLDER_RE)) {
    const token = m[0];
    if (valid.has(token)) continue;
    // For sequential, accept any {step:<id>} even if id isn't currently present —
    // the user might be drafting before the referenced step exists. We only flag
    // it if it doesn't look like a known token shape at all.
    if (ctx.kind === "sequential" && token.startsWith("{step:")) continue;
    const idx = m.index ?? 0;
    const line = text.slice(0, idx).split("\n").length;
    hits.push({ match: token, index: idx, line });
  }
  return hits;
}

// ── Wave structure ──────────────────────────────────────────────────
//
// Mirrors the grouping logic in src-tauri/src/pipeline/executor.rs::group_into_waves.
// Adjacent enabled Parallel steps form one wave; each enabled Sequential step is its
// own wave. We only consider enabled steps because disabled ones don't affect the
// runtime shape — but for editor visibility we still surface them as ghosts via the
// `disabled` flag so users can see what's switched off.

export interface ParallelWave {
  kind: "parallel";
  /** Steps that will run concurrently (one entry per StepConfig, regardless of agent count). */
  steps: StepConfig[];
  /** True if any step in the wave runs on multiple agents — implies an auto-merge node. */
  hasMultiAgent: boolean;
}

export interface SequentialWave {
  kind: "sequential";
  step: StepConfig;
}

export type Wave = ParallelWave | SequentialWave;

export function computeWaves(steps: StepConfig[], includeDisabled = false): Wave[] {
  const eligible = includeDisabled ? steps : steps.filter((s) => s.enabled);
  const waves: Wave[] = [];
  let buffer: StepConfig[] = [];
  const flush = () => {
    if (buffer.length === 0) return;
    waves.push({
      kind: "parallel",
      steps: buffer,
      hasMultiAgent: buffer.some((s) => (s.agents?.length ?? 0) > 1),
    });
    buffer = [];
  };
  for (const step of eligible) {
    if (step.phase === "parallel") {
      buffer.push(step);
    } else {
      flush();
      waves.push({ kind: "sequential", step });
    }
  }
  flush();
  return waves;
}

// ── Drag-drop helpers ───────────────────────────────────────────────

/**
 * Compute the new step list after a drag-drop. `fromIdx` is the original
 * position of the dragged step; `toIdx` is the index it should occupy
 * AFTER removal of the source. `targetPhase` lets the drop site override
 * the dragged step's phase (e.g. when crossing the parallel/sequential
 * section boundary).
 */
export function reorderSteps(
  steps: StepConfig[],
  fromIdx: number,
  toIdx: number,
  targetPhase: Phase,
): StepConfig[] {
  if (fromIdx < 0 || fromIdx >= steps.length) return steps;
  const next = steps.slice();
  const [moved] = next.splice(fromIdx, 1);
  const adjusted = { ...moved, phase: targetPhase };
  const clampedTo = Math.max(0, Math.min(toIdx, next.length));
  next.splice(clampedTo, 0, adjusted);
  return next;
}

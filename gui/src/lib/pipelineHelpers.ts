// Pipeline editor helpers: placeholder catalog, lint detection, wave structure.
// Kept separate from PipelinePage.tsx so the rules can be unit-tested or reused
// by future UI surfaces (e.g. a step-test runner) without lifting the whole page.

import type { StepConfig } from "./types";

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
        { token: "{prior_outputs}", description: "Selected upstream reports concatenated" },
        { token: "{last_output}", description: "Most recent selected upstream report" },
        { token: "{input_path}", description: "Path to the extracted input text" },
        { token: "{document_bundle}", description: "Path to canonical DocumentBundle JSON" },
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
        { token: "{document_bundle}", description: "Path to canonical DocumentBundle JSON" },
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
// Mirrors executor readiness: explicit `after` edges and selected upstream
// artifacts form the dependency graph. All ready Parallel steps run together;
// when no Parallel step is ready, the first ready Sequential step runs alone.

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
  const eligibleIds = new Set(eligible.map((step) => step.id));
  const dependencies = eligible.map((step) =>
    new Set([
      ...(step.after ?? []),
      ...(step.context?.include ?? [])
        .filter((selector) => selector.kind === "step")
        .map((selector) => selector.step),
    ].filter((id) => eligibleIds.has(id))),
  );
  const waves: Wave[] = [];
  const done = new Set<string>();
  let remaining = eligible.map((_, index) => index);

  while (remaining.length) {
    const ready = remaining.filter((index) =>
      [...dependencies[index]].every((dependency) => done.has(dependency)),
    );
    if (!ready.length) {
      // Keep invalid/cyclic drafts visible. Backend validation will explain the
      // graph error when the user saves.
      const index = remaining[0];
      const step = eligible[index];
      waves.push(step.phase === "parallel"
        ? { kind: "parallel", steps: [step], hasMultiAgent: (step.agents?.length ?? 0) > 1 }
        : { kind: "sequential", step });
      done.add(step.id);
      remaining = remaining.filter((candidate) => candidate !== index);
      continue;
    }

    const readyParallel = ready.filter((index) => eligible[index].phase === "parallel");
    if (readyParallel.length) {
      const waveSteps = readyParallel.map((index) => eligible[index]);
      waves.push({
        kind: "parallel",
        steps: waveSteps,
        hasMultiAgent: waveSteps.some((step) => (step.agents?.length ?? 0) > 1),
      });
      waveSteps.forEach((step) => done.add(step.id));
      remaining = remaining.filter((index) => !readyParallel.includes(index));
      continue;
    }

    const index = ready[0];
    const step = eligible[index];
    waves.push({
      kind: "sequential",
      step,
    });
    done.add(step.id);
    remaining = remaining.filter((candidate) => candidate !== index);
  }

  return waves;
}

// ── Canonical execution plan ───────────────────────────────────────

export type ExecutionStageKind =
  | "extracting"
  | "orienting"
  | "dispatching"
  | "merging"
  | "synthesizing"
  | "done";

export interface ExecutionPlanStage {
  id: string;
  kind: ExecutionStageKind;
  label: string;
  stepIds: string[];
  stepLabels?: string[];
}

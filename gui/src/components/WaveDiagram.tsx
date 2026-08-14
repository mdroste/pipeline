// Compact pipeline shape visualization rendered above the step list.
//
// Layout: vertical stack of dependency-ready waves, top-to-bottom.
// Within a parallel wave, branches flow horizontally and wrap to additional
// rows when they exceed the sidebar width — so the diagram grows downward,
// never sideways. A single thin vertical line between waves communicates
// "these are the next ready calls". Clicking any node selects the
// corresponding step in the editor.

import { computeWaves } from "../lib/pipelineHelpers";
import type { StepConfig, MergeConfig } from "../lib/types";
import { adaptiveAgentCountLabel } from "../lib/autoReview";

export type WaveSelection =
  | string
  | "merge"
  | "pipeline_settings"
  | "extraction"
  | "orientation"
  | "auto_adaptive_agents";

interface Props {
  steps: StepConfig[];
  merge: MergeConfig;
  adaptiveReview?: boolean;
  adaptiveAgentCount?: number | null;
  selectedId: WaveSelection | null;
  onSelect: (id: WaveSelection) => void;
}

export default function WaveDiagram({ steps, merge, adaptiveReview = false, adaptiveAgentCount = null, selectedId, onSelect }: Props) {
  const waves = computeWaves(steps, false);
  const noEnabledSteps = waves.length === 0;
  const enabledSteps = steps.filter((step) => step.enabled);
  const stepCalls = enabledSteps.reduce(
    (total, step) => total + Math.max(1, step.agents?.length ?? 0) * (step.for_each?.max ?? 1),
    0,
  );
  const mergeCalls = merge.enabled
    ? enabledSteps.reduce(
        (total, step) => total + ((step.agents?.length ?? 0) > 1 ? step.for_each?.max ?? 1 : 0),
        0,
      )
    : 0;
  const providerCalls = stepCalls + mergeCalls;

  // Build a flat row list so the connector logic stays simple: we render a
  // row, then a connector, then the next row. Pre-processing (extract +
  // required orient) sits at the top so users can edit those stages too.
  // Merge is a one-node row that sits between a multi-agent parallel wave
  // and whatever follows.
  type Row =
    | { kind: "extract" }
    | { kind: "orient" }
    | { kind: "parallel"; steps: StepConfig[]; adaptive: boolean }
    | { kind: "sequential"; step: StepConfig }
    | { kind: "merge" };
  const rows: Row[] = [{ kind: "extract" }, { kind: "orient" }];
  let foundParallelWave = false;
  for (const w of waves) {
    if (w.kind === "parallel") {
      rows.push({ kind: "parallel", steps: w.steps, adaptive: adaptiveReview && !foundParallelWave });
      foundParallelWave = true;
      if (w.hasMultiAgent && merge.enabled) rows.push({ kind: "merge" });
    } else {
      rows.push({ kind: "sequential", step: w.step });
    }
  }

  return (
    <div className="px-4 py-4 bg-gray-50/40 dark:bg-gray-900/30">
      <div className="mb-4">
        <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200">
          Workflow overview
        </h3>
        <p className="mt-1 text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">
          {adaptiveReview
            ? `${enabledSteps.length} saved steps plus ${adaptiveAgentCountLabel(adaptiveAgentCount)} auto-selected specialists in ${waves.length} execution ${waves.length === 1 ? "wave" : "waves"}.`
            : `${enabledSteps.length} enabled ${enabledSteps.length === 1 ? "step" : "steps"} in ${waves.length} execution ${waves.length === 1 ? "wave" : "waves"}; up to ${providerCalls} provider ${providerCalls === 1 ? "call" : "calls"} before retries.`}
        </p>
      </div>
      <div className="mb-3 flex flex-wrap gap-x-3 gap-y-1 text-[10px] text-gray-500 dark:text-gray-400" aria-label="Overview legend">
        <span><span className="mr-1 inline-block h-2 w-2 rounded-sm bg-blue-200 dark:bg-blue-800" />Parallel</span>
        <span><span className="mr-1 inline-block h-2 w-2 rounded-sm bg-orange-200 dark:bg-orange-800" />Sequential</span>
        <span><span className="mr-1 inline-block h-2 w-2 rounded-sm bg-amber-200 dark:bg-amber-800" />Merge</span>
      </div>
      {noEnabledSteps && (
        <div className="px-1 pb-2 text-[10px] text-gray-600 dark:text-gray-400 italic">
          No enabled review steps — toggle one on below.
        </div>
      )}
      <div className="flex flex-col items-stretch gap-0">
        {rows.map((row, i) => (
          <div key={i} className="flex flex-col items-stretch">
            {row.kind === "extract" && (
              <PreprocessRow
                label="Extract"
                selected={selectedId === "extraction"}
                onClick={() => onSelect("extraction")}
              />
            )}
            {row.kind === "orient" && (
              <PreprocessRow
                label={adaptiveReview ? "Orient + classify" : "Orient"}
                selected={selectedId === "orientation"}
                onClick={() => onSelect("orientation")}
              />
            )}
            {row.kind === "parallel" && (
              <ParallelRow
                steps={row.steps}
                adaptive={row.adaptive}
                adaptiveAgentCount={adaptiveAgentCount}
                selectedId={selectedId}
                onSelect={onSelect}
              />
            )}
            {row.kind === "sequential" && (
              <SequentialRow
                step={row.step}
                selected={selectedId === row.step.id}
                onClick={() => onSelect(row.step.id)}
              />
            )}
            {row.kind === "merge" && (
              <MergeRow
                selected={selectedId === "merge"}
                onClick={() => onSelect("merge")}
              />
            )}
            {i < rows.length - 1 && <Connector />}
          </div>
        ))}
      </div>
      <p className="mt-4 text-center text-[10px] text-gray-500 dark:text-gray-400">
        Select a stage to inspect or edit it.
      </p>
    </div>
  );
}

// ── Rows ────────────────────────────────────────────────────────────

function ParallelRow({
  steps,
  adaptive,
  adaptiveAgentCount,
  selectedId,
  onSelect,
}: {
  steps: StepConfig[];
  adaptive: boolean;
  adaptiveAgentCount: number | null;
  selectedId: WaveSelection | null;
  onSelect: (id: string) => void;
}) {
  // A single-step parallel wave isn't actually fan-out; render it like a
  // sequential row so the user isn't misled by visual noise.
  if (steps.length === 1 && !adaptive) {
    const s = steps[0];
    return (
      <SequentialRow
        step={s}
        selected={selectedId === s.id}
        onClick={() => onSelect(s.id)}
        variant="parallel"
      />
    );
  }
  return (
    <div className="flex flex-wrap gap-1 justify-center">
      {steps.map((s) => (
        <Node
          key={s.id}
          label={s.label}
          selected={selectedId === s.id}
          variant="parallel"
          onClick={() => onSelect(s.id)}
        />
      ))}
      {adaptive && (
        <Node
          label={`Adaptive agents (${adaptiveAgentCountLabel(adaptiveAgentCount)})`}
          selected={selectedId === "auto_adaptive_agents"}
          variant="adaptive"
          onClick={() => onSelect("auto_adaptive_agents")}
        />
      )}
    </div>
  );
}

function SequentialRow({
  step,
  selected,
  onClick,
  variant = "sequential",
}: {
  step: StepConfig;
  selected: boolean;
  onClick: () => void;
  variant?: "sequential" | "parallel";
}) {
  return (
    <div className="flex justify-center">
      <Node
        label={step.label}
        selected={selected}
        variant={variant}
        onClick={onClick}
        wide
      />
    </div>
  );
}

function MergeRow({ selected, onClick }: { selected: boolean; onClick: () => void }) {
  return (
    <div className="flex justify-center">
      <Node label="Merge" selected={selected} variant="merge" onClick={onClick} />
    </div>
  );
}

function PreprocessRow({
  label,
  selected,
  onClick,
}: {
  label: string;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <div className="flex justify-center">
      <Node label={label} selected={selected} variant="preprocess" onClick={onClick} />
    </div>
  );
}

// ── Atoms ───────────────────────────────────────────────────────────

function Node({
  label,
  selected,
  variant,
  wide,
  onClick,
}: {
  label: string;
  selected: boolean;
  variant: "parallel" | "adaptive" | "sequential" | "merge" | "preprocess";
  wide?: boolean;
  onClick: () => void;
}) {
  // Use min-w-0 + flex-1 friendliness so wrapping works cleanly. `wide`
  // makes a single-row pill stretch a bit so the layout feels intentional
  // rather than orphaned in the center of the panel.
  const base =
    "rounded-md border text-[10px] font-medium truncate transition-colors text-left cursor-pointer";
  const sizing = wide
    ? "px-2.5 py-1 max-w-[260px] min-w-[120px] text-center"
    : "px-2 py-1 max-w-[140px]";
  const palette = (() => {
    if (selected) return "bg-blue-700 text-white border-blue-700";
    switch (variant) {
      case "parallel":
        return "bg-blue-50 dark:bg-blue-950/40 border-blue-200 dark:border-blue-900 text-blue-700 dark:text-blue-300 hover:bg-blue-100 dark:hover:bg-blue-950";
      case "adaptive":
        return "border-dashed bg-indigo-50 dark:bg-indigo-950/30 border-indigo-300 dark:border-indigo-800 text-indigo-700 dark:text-indigo-300 hover:bg-indigo-100 dark:hover:bg-indigo-950/50";
      case "sequential":
        return "bg-orange-50 dark:bg-orange-950/40 border-orange-200 dark:border-orange-900 text-orange-700 dark:text-orange-300 hover:bg-orange-100 dark:hover:bg-orange-950";
      case "merge":
        return "bg-amber-50 dark:bg-amber-950/40 border-amber-300 dark:border-amber-900 text-amber-700 dark:text-amber-300 hover:bg-amber-100 dark:hover:bg-amber-950";
      case "preprocess":
        return "bg-gray-50 dark:bg-gray-800 border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700";
    }
  })();
  return (
    <button onClick={onClick} title={label} className={`${base} ${sizing} ${palette}`}>
      {label}
    </button>
  );
}

function Connector() {
  // 12px tall vertical line, centered. Thin enough to feel like a hint
  // rather than a tied-together-with-string aesthetic.
  return (
    <div className="flex justify-center" aria-hidden="true">
      <div className="w-px h-3 bg-gray-300 dark:bg-gray-600" />
    </div>
  );
}

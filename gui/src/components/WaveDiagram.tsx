// Compact pipeline shape visualization rendered above the step list.
//
// Layout: vertical stack of waves, top-to-bottom along execution order.
// Within a parallel wave, branches flow horizontally and wrap to additional
// rows when they exceed the sidebar width — so the diagram grows downward,
// never sideways. A single thin vertical line between waves communicates
// "all of these complete before the next runs". Clicking any node selects
// the corresponding step in the editor.

import { useState } from "react";
import { computeWaves } from "../lib/pipelineHelpers";
import type { StepConfig, MergeConfig } from "../lib/types";

export type WaveSelection =
  | string
  | "merge"
  | "pipeline_settings"
  | "extraction"
  | "orientation";

interface Props {
  steps: StepConfig[];
  merge: MergeConfig;
  useOrientation: boolean;
  selectedId: WaveSelection | null;
  onSelect: (id: WaveSelection) => void;
}

export default function WaveDiagram({ steps, merge, useOrientation, selectedId, onSelect }: Props) {
  const [collapsed, setCollapsed] = useState(false);
  const waves = computeWaves(steps, false);
  const noEnabledSteps = waves.length === 0;

  // Build a flat row list so the connector logic stays simple: we render a
  // row, then a connector, then the next row. Pre-processing (extract +
  // optional orient) sits at the top so users can edit those stages too.
  // Merge is a one-node row that sits between a multi-agent parallel wave
  // and whatever follows.
  type Row =
    | { kind: "extract" }
    | { kind: "orient" }
    | { kind: "parallel"; steps: StepConfig[] }
    | { kind: "sequential"; step: StepConfig }
    | { kind: "merge" };
  const rows: Row[] = [{ kind: "extract" }];
  if (useOrientation) rows.push({ kind: "orient" });
  for (const w of waves) {
    if (w.kind === "parallel") {
      rows.push({ kind: "parallel", steps: w.steps });
      if (w.hasMultiAgent && merge.enabled) rows.push({ kind: "merge" });
    } else {
      rows.push({ kind: "sequential", step: w.step });
    }
  }

  return (
    <div className="px-3 py-2 border-b border-gray-200 dark:border-gray-700 bg-gray-50/40 dark:bg-gray-900/30">
      <div className="flex items-center justify-between mb-1.5">
        <span className="text-[10px] uppercase tracking-wider text-gray-400 dark:text-gray-500 font-medium">
          Execution shape
        </span>
        <button
          type="button"
          onClick={() => setCollapsed(!collapsed)}
          className="text-[10px] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
          title={collapsed ? "Show diagram" : "Hide diagram"}
        >
          {collapsed ? "▸" : "▾"}
        </button>
      </div>
      {!collapsed && noEnabledSteps && (
        <div className="px-1 pb-2 text-[10px] text-gray-400 dark:text-gray-500 italic">
          No enabled review steps — toggle one on below.
        </div>
      )}
      {!collapsed && (
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
                  label="Orient"
                  selected={selectedId === "orientation"}
                  onClick={() => onSelect("orientation")}
                />
              )}
              {row.kind === "parallel" && (
                <ParallelRow
                  steps={row.steps}
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
      )}
    </div>
  );
}

// ── Rows ────────────────────────────────────────────────────────────

function ParallelRow({
  steps,
  selectedId,
  onSelect,
}: {
  steps: StepConfig[];
  selectedId: WaveSelection | null;
  onSelect: (id: string) => void;
}) {
  // A single-step parallel wave isn't actually fan-out; render it like a
  // sequential row so the user isn't misled by visual noise.
  if (steps.length === 1) {
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
  variant: "parallel" | "sequential" | "merge" | "preprocess";
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
    if (selected) return "bg-blue-500 text-white border-blue-500";
    switch (variant) {
      case "parallel":
        return "bg-blue-50 dark:bg-blue-950/40 border-blue-200 dark:border-blue-900 text-blue-700 dark:text-blue-300 hover:bg-blue-100 dark:hover:bg-blue-950";
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

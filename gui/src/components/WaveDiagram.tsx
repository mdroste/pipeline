// Compact pipeline shape visualization rendered above the step list.
//
// Layout: vertical stack of dependency-ready waves, top-to-bottom, on a
// dotted canvas. Every stage is a card. A parallel wave sits in a labelled
// frame that splits into equal columns as it widens and wraps to additional
// rows otherwise — so the diagram grows downward, never sideways. An arrow
// between waves communicates "these are the next ready calls". Clicking any
// node selects the corresponding step in the editor.

import { Fragment, useId } from "react";
import { computeWaves } from "../lib/pipelineHelpers";
import type { StepConfig, MergeConfig } from "../lib/types";
import { adaptiveAgentCountLabel } from "../lib/autoReview";
import type { AdaptiveAgentRange } from "../lib/autoReview";
import "./WaveDiagram.css";

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
  adaptiveAgentRange?: AdaptiveAgentRange;
  selectedId: WaveSelection | null;
  onSelect: (id: WaveSelection) => void;
  /** Instruction shown under the canvas; callers tailor it to what a click does. */
  hint?: string;
}

export default function WaveDiagram({
  steps,
  merge,
  adaptiveReview = false,
  adaptiveAgentCount = null,
  adaptiveAgentRange,
  selectedId,
  onSelect,
  hint = "Select a stage to inspect or edit it.",
}: Props) {
  const waves = computeWaves(steps, false);
  const noEnabledSteps = waves.length === 0;
  const enabledSteps = steps.filter((step) => step.enabled);
  const stepCalls = enabledSteps.reduce(
    (total, step) =>
      total + Math.max(1, step.agents?.length ?? 0) * (step.for_each?.max ?? 1),
    0,
  );
  const mergeCalls = merge.enabled
    ? enabledSteps.reduce(
        (total, step) =>
          total +
          ((step.agents?.length ?? 0) > 1 ? (step.for_each?.max ?? 1) : 0),
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
      rows.push({
        kind: "parallel",
        steps: w.steps,
        adaptive: adaptiveReview && !foundParallelWave,
      });
      foundParallelWave = true;
      if (w.hasMultiAgent && merge.enabled) rows.push({ kind: "merge" });
    } else {
      rows.push({ kind: "sequential", step: w.step });
    }
  }

  return (
    <div className="px-4 py-4">
      <div className="overflow-hidden rounded-ui-lg border border-line bg-surface">
        <div className="border-b border-line px-4 py-3">
          <h3 className="text-sm font-semibold text-ink">Workflow overview</h3>
          <p className="mt-0.5 text-ui-meta text-ink-muted">
            {adaptiveReview
              ? `${enabledSteps.length} saved steps plus ${adaptiveAgentCountLabel(adaptiveAgentCount, adaptiveAgentRange)} auto-selected specialists in ${waves.length} execution ${waves.length === 1 ? "wave" : "waves"}.`
              : `${enabledSteps.length} enabled ${enabledSteps.length === 1 ? "step" : "steps"} in ${waves.length} execution ${waves.length === 1 ? "wave" : "waves"}; up to ${providerCalls} provider ${providerCalls === 1 ? "call" : "calls"} before retries.`}
          </p>
        </div>
        <div className="wave-canvas flex flex-col items-center px-4 py-6">
          {rows.map((row, i) => (
            <Fragment key={i}>
              {i > 0 && <Connector />}
              {row.kind === "extract" && (
                <Node
                  kind="extract"
                  label="Extract"
                  selected={selectedId === "extraction"}
                  onClick={() => onSelect("extraction")}
                />
              )}
              {row.kind === "orient" && (
                <Node
                  kind="orient"
                  label={adaptiveReview ? "Orient + classify" : "Orient"}
                  detail={adaptiveReview ? "Selects specialists" : undefined}
                  selected={selectedId === "orientation"}
                  onClick={() => onSelect("orientation")}
                />
              )}
              {row.kind === "parallel" && (
                <ParallelRow
                  steps={row.steps}
                  adaptive={row.adaptive}
                  adaptiveAgentCount={adaptiveAgentCount}
                  adaptiveAgentRange={adaptiveAgentRange}
                  selectedId={selectedId}
                  onSelect={onSelect}
                />
              )}
              {row.kind === "sequential" && (
                <StepNode
                  step={row.step}
                  selected={selectedId === row.step.id}
                  onClick={() => onSelect(row.step.id)}
                />
              )}
              {row.kind === "merge" && (
                <Node
                  kind="merge"
                  label="Merge"
                  detail="Combines agent replies"
                  selected={selectedId === "merge"}
                  onClick={() => onSelect("merge")}
                />
              )}
            </Fragment>
          ))}
          {noEnabledSteps && (
            <>
              <Connector />
              <p className="w-52 max-w-full rounded-ui-md border border-dashed border-line-strong bg-surface px-3 py-2 text-center text-ui-meta text-ink-muted">
                No enabled review steps. Turn one on to add it here.
              </p>
            </>
          )}
        </div>
      </div>
      <p className="mt-3 text-center text-ui-meta text-ink-muted">{hint}</p>
    </div>
  );
}

// ── Rows ────────────────────────────────────────────────────────────

function ParallelRow({
  steps,
  adaptive,
  adaptiveAgentCount,
  adaptiveAgentRange,
  selectedId,
  onSelect,
}: {
  steps: StepConfig[];
  adaptive: boolean;
  adaptiveAgentCount: number | null;
  adaptiveAgentRange?: AdaptiveAgentRange;
  selectedId: WaveSelection | null;
  onSelect: (id: string) => void;
}) {
  // A single-step parallel wave isn't actually fan-out; render it like a
  // sequential row so the user isn't misled by visual noise.
  if (steps.length === 1 && !adaptive) {
    const s = steps[0];
    return (
      <StepNode
        step={s}
        selected={selectedId === s.id}
        onClick={() => onSelect(s.id)}
      />
    );
  }
  const count = adaptiveAgentCountLabel(adaptiveAgentCount, adaptiveAgentRange);
  return (
    <div
      role="group"
      aria-label="Parallel steps"
      className="wave-parallel relative w-full rounded-ui-lg border border-dashed border-line-strong px-3 pb-3 pt-5"
    >
      <span className="absolute -top-2 left-3 inline-flex items-center gap-1 rounded-full border border-line bg-surface px-2 text-ui-meta font-medium text-ink-muted">
        <StageIcon kind="fork" />
        In parallel
      </span>
      <div className="wave-parallel-cards">
        {steps.map((s) => (
          <StepNode
            key={s.id}
            step={s}
            selected={selectedId === s.id}
            onClick={() => onSelect(s.id)}
            inGroup
          />
        ))}
        {adaptive && (
          <Node
            kind="adaptive"
            inGroup
            label="Adaptive agents"
            accessibleLabel={`Adaptive agents (${count})`}
            detail={`${count} auto-selected`}
            selected={selectedId === "auto_adaptive_agents"}
            onClick={() => onSelect("auto_adaptive_agents")}
          />
        )}
      </div>
    </div>
  );
}

function StepNode({
  step,
  selected,
  onClick,
  inGroup,
}: {
  step: StepConfig;
  selected: boolean;
  onClick: () => void;
  inGroup?: boolean;
}) {
  return (
    <Node
      kind="step"
      label={step.label}
      detail={stepDetail(step)}
      selected={selected}
      onClick={onClick}
      inGroup={inGroup}
    />
  );
}

/** Short card subtitle: only what changes how many calls the step makes. */
function stepDetail(step: StepConfig): string | undefined {
  const parts: string[] = [];
  const agents = step.agents?.length ?? 0;
  if (agents > 1) parts.push(`${agents} agents`);
  if (step.for_each) parts.push(`Up to ${step.for_each.max} items`);
  if (step.run_if) parts.push("Conditional");
  return parts.length ? parts.join(" · ") : undefined;
}

// ── Atoms ───────────────────────────────────────────────────────────

type NodeKind = "extract" | "orient" | "step" | "adaptive" | "merge";

function Node({
  kind,
  label,
  accessibleLabel,
  detail,
  selected,
  onClick,
  inGroup,
}: {
  kind: NodeKind;
  label: string;
  /** Overrides the button name when the visible label omits a detail. */
  accessibleLabel?: string;
  detail?: string;
  selected: boolean;
  onClick: () => void;
  /** Parallel-frame cards take their width from the frame's columns. */
  inGroup?: boolean;
}) {
  const detailId = useId();
  // Standalone cards share one width so the spine stays straight; max-w-full
  // lets the narrowest sidebar shrink them instead of overflowing.
  const width = inGroup ? "" : "w-52 max-w-full";
  const border = selected
    ? "border-accent ring-2 ring-accent-soft"
    : kind === "adaptive"
      ? "border-dashed border-line-strong hover:border-ink-muted"
      : "border-line hover:border-line-strong";
  return (
    <button
      type="button"
      onClick={onClick}
      title={accessibleLabel ?? label}
      aria-label={accessibleLabel ?? label}
      aria-describedby={detail ? detailId : undefined}
      className={`${width} flex cursor-pointer items-center gap-2 rounded-ui-md border bg-raised px-2.5 py-2 text-left shadow-ui-1 transition hover:shadow-ui-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${border}`}
    >
      <span
        className={`grid h-6 w-6 shrink-0 place-items-center rounded-ui-sm transition-colors ${
          selected ? "bg-accent-soft text-accent" : "bg-sunken text-ink-muted"
        }`}
      >
        <StageIcon kind={kind} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="line-clamp-2 break-words text-ui-label font-medium text-ink">
          {label}
        </span>
        {detail && (
          <span
            id={detailId}
            className="block truncate text-ui-meta text-ink-muted"
          >
            {detail}
          </span>
        )}
      </span>
    </button>
  );
}

function Connector() {
  // Short arrow between waves, centered on the canvas. Drawn from muted ink
  // rather than the border token so it stands apart from the dot grid.
  return (
    <svg
      aria-hidden="true"
      className="shrink-0 text-ink-muted"
      width="12"
      height="24"
      viewBox="0 0 12 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeOpacity="0.55"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M6 0v22" />
      <path d="m2.5 18.5 3.5 3.5 3.5-3.5" />
    </svg>
  );
}

function StageIcon({ kind }: { kind: NodeKind | "fork" }) {
  const paths: Record<NodeKind | "fork", string[]> = {
    // Page with text lines.
    extract: [
      "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z",
      "M14 3v5h5",
      "M9 13h6M9 17h4",
    ],
    // Compass.
    orient: ["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z", "m15.5 8.5-2 5-5 2 2-5z"],
    // Speech bubble: one model call.
    step: ["M4 5h16v11H9l-5 4z", "M8 9h8M8 12h5"],
    // Four-point sparkle.
    adaptive: [
      "M12 3c.6 4.6 2.4 6.4 7 7-4.6.6-6.4 2.4-7 7-.6-4.6-2.4-6.4-7-7 4.6-.6 6.4-2.4 7-7z",
      "M19 16v4M17 18h4",
    ],
    // Two branches joining into one.
    merge: ["M6 3v4a6 6 0 0 0 12 0V3", "M12 13v8"],
    // One line splitting into two.
    fork: ["M12 3v8", "M6 21v-4a6 6 0 0 1 12 0v4"],
  };
  const size = kind === "fork" ? 12 : 14;
  return (
    <svg
      aria-hidden="true"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {paths[kind].map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}

// Review overview: a numbered rail of the stages a review runs through.
//
// Stage 1 prepares the input (extract + orient). Each later number is one
// dependency-ready execution wave, the same numbering the run-progress view
// uses. Review steps are cards; system work (preparation, merge) stays quiet.
// Clicking any node selects that stage in the editor.

import { useId, type ReactNode } from "react";
import type { AdaptiveAgentRange } from "../lib/autoReview";
import type { MergeConfig, StepConfig } from "../lib/types";
import {
  agentName,
  buildOverview,
  type OverviewModel,
  type OverviewNode,
  type OverviewStage,
  type WaveSelection,
} from "../lib/workflowOverview";

export type { WaveSelection } from "../lib/workflowOverview";

interface Props {
  steps: StepConfig[];
  merge: MergeConfig;
  /** Extraction input mode; names the preparation stage. */
  inputMode?: string;
  adaptiveReview?: boolean;
  adaptiveAgentCount?: number | null;
  adaptiveAgentRange?: AdaptiveAgentRange;
  selectedId: WaveSelection | null;
  onSelect: (id: WaveSelection) => void;
  /** Single-column layout for the designer sidebar. */
  compact?: boolean;
  /** Instruction shown under the rail; callers tailor it to what a click does. */
  hint?: string;
}

export default function WaveDiagram({
  steps,
  merge,
  inputMode,
  adaptiveReview = false,
  adaptiveAgentCount = null,
  adaptiveAgentRange,
  selectedId,
  onSelect,
  compact = false,
  hint = "Select a stage to inspect or edit it.",
}: Props) {
  const headingId = useId();
  const model = buildOverview({
    steps,
    merge,
    inputMode,
    adaptiveReview,
    adaptiveAgentCount,
    adaptiveAgentRange,
  });
  const lastIndex = model.stages.length - 1;

  return (
    <section aria-labelledby={headingId} className={compact ? "px-4 py-4" : ""}>
      <header
        className={
          compact ? "space-y-2" : "flex items-center justify-between gap-4"
        }
      >
        <h3 id={headingId} className="text-ui-title font-semibold text-ink">
          Review overview
        </h3>
        <SummaryChips model={model} />
      </header>

      <ol className="mt-4">
        {model.stages.map((stage, index) => (
          <li key={stage.key} className="relative flex gap-3">
            <div
              className="relative flex w-6 flex-none justify-center"
              aria-hidden="true"
            >
              {index > 0 && (
                <span className="absolute top-0 h-2 w-px bg-line-strong dark:bg-line" />
              )}
              {index < lastIndex && (
                <span className="absolute bottom-0 top-8 w-px bg-line-strong dark:bg-line" />
              )}
              <StageMarker
                stage={stage}
                active={stage.nodes.some((node) => node.id === selectedId)}
              />
            </div>
            <div
              className={`min-w-0 flex-1 ${index < lastIndex ? "pb-2" : ""}`}
            >
              <StageBody
                stage={stage}
                compact={compact}
                selectedId={selectedId}
                onSelect={onSelect}
              />
            </div>
          </li>
        ))}
      </ol>

      {model.stepCount === 0 && (
        <p className="ml-9 mt-2 rounded-ui-md border border-dashed border-line-strong px-3 py-2.5 text-ui-meta text-ink-muted">
          No enabled review steps yet. Turn one on under Steps.
        </p>
      )}
      <p className="mt-4 text-center text-ui-meta text-ink-muted">{hint}</p>
    </section>
  );
}

// ── Header ──────────────────────────────────────────────────────────

function plural(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? "" : "s"}`;
}

function SummaryChips({ model }: { model: OverviewModel }) {
  const chips: { key: string; value: string; label: string; title?: string }[] =
    model.adaptive
      ? [
          {
            key: "steps",
            value: String(model.stepCount),
            label: model.stepCount === 1 ? "saved step" : "saved steps",
          },
          {
            key: "specialists",
            value: model.adaptiveLabel,
            label: "auto-selected specialists",
          },
        ]
      : [
          {
            key: "steps",
            value: String(model.stepCount),
            label: model.stepCount === 1 ? "step" : "steps",
          },
          {
            key: "calls",
            value: `≤ ${model.providerCalls}`,
            label: model.providerCalls === 1 ? "model call" : "model calls",
            title: `Up to ${plural(model.providerCalls, "provider call")} before retries`,
          },
        ];
  return (
    <div className="flex flex-wrap gap-1.5">
      {chips.map((chip) => (
        <span
          key={chip.key}
          title={chip.title}
          className="rounded-full bg-sunken px-2.5 py-0.5 text-ui-meta tabular-nums text-ink-muted dark:bg-transparent dark:px-0"
        >
          <span className="font-semibold text-ink">{chip.value}</span>{" "}
          {chip.label}
        </span>
      ))}
    </div>
  );
}

// ── Rail ────────────────────────────────────────────────────────────

const FOCUS = "outline-none focus-visible:ring-2 focus-visible:ring-accent";

function StageMarker({
  stage,
  active,
}: {
  stage: OverviewStage;
  active: boolean;
}) {
  const tone = active
    ? "border-inverse bg-inverse text-on-inverse dark:border-line-strong dark:bg-raised dark:text-ink"
    : stage.number !== null
      ? "border-line-strong bg-surface text-ink dark:border-line dark:text-ink-muted"
      : "border-line bg-sunken text-ink-muted";
  return (
    <span
      className={`relative z-[1] mt-2 grid h-6 w-6 place-items-center rounded-full border text-[12px] font-semibold tabular-nums transition-colors ${tone}`}
    >
      {stage.number ?? <Icon name="merge" className="h-3.5 w-3.5" />}
    </span>
  );
}

function StageBody({
  stage,
  compact,
  selectedId,
  onSelect,
}: {
  stage: OverviewStage;
  compact: boolean;
  selectedId: WaveSelection | null;
  onSelect: (id: WaveSelection) => void;
}) {
  if (stage.kind === "merge") {
    const node = stage.nodes[0];
    return (
      <button
        type="button"
        aria-label={node.label}
        title={node.caption}
        onClick={() => onSelect(node.id)}
        className={`flex w-full items-center gap-2 rounded-ui-md px-3 py-[11px] text-left transition-colors ${
          node.id === selectedId ? "bg-accent-soft" : "hover:bg-sunken"
        } ${FOCUS}`}
      >
        <span className="text-ui-label font-medium text-ink">{node.label}</span>
        {!compact && (
          <span className="truncate text-ui-meta text-ink-muted">
            {node.caption}
          </span>
        )}
        <span className="ml-auto">
          <AgentDots agents={node.agents} />
        </span>
      </button>
    );
  }

  if (stage.kind === "single") {
    const node = stage.nodes[0];
    return (
      <StepCard
        node={node}
        selected={node.id === selectedId}
        onSelect={onSelect}
      />
    );
  }

  return (
    <div>
      <div className="flex items-baseline gap-1.5 px-3 pb-2 pt-[11px]">
        <span className="text-ui-label font-semibold text-ink">
          {stage.title}
        </span>
        {stage.caption && (
          <span className="text-ui-meta text-ink-muted">· {stage.caption}</span>
        )}
      </div>
      {stage.kind === "prepare" ? (
        <div className="grid grid-cols-2 gap-2">
          {stage.nodes.map((node) => (
            <PrepareTile
              key={node.id}
              node={node}
              compact={compact}
              selected={node.id === selectedId}
              onSelect={onSelect}
            />
          ))}
        </div>
      ) : (
        <div
          role="group"
          aria-label="Parallel steps"
          className={`grid gap-2 ${compact ? "grid-cols-1" : "grid-cols-2"}`}
        >
          {stage.nodes.map((node) => (
            <StepCard
              key={node.id}
              node={node}
              selected={node.id === selectedId}
              onSelect={onSelect}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function PrepareTile({
  node,
  compact,
  selected,
  onSelect,
}: {
  node: OverviewNode;
  compact: boolean;
  selected: boolean;
  onSelect: (id: WaveSelection) => void;
}) {
  return (
    <button
      type="button"
      aria-label={node.label}
      title={node.caption}
      onClick={() => onSelect(node.id)}
      className={`flex min-w-0 items-start gap-2 rounded-ui-md border px-3 py-2 text-left transition-colors ${
        selected
          ? "border-accent bg-accent-soft"
          : "border-transparent bg-sunken hover:border-line-strong"
      } ${FOCUS}`}
    >
      <Icon
        name={node.id === "extraction" ? "extract" : "orient"}
        className="mt-0.5 h-3.5 w-3.5 text-ink-muted"
      />
      <span className="min-w-0">
        <span className="block truncate text-ui-label font-medium text-ink">
          {node.label}
        </span>
        {!compact && (
          <span className="block truncate text-ui-meta text-ink-muted">
            {node.caption}
          </span>
        )}
      </span>
    </button>
  );
}

function StepCard({
  node,
  selected,
  onSelect,
}: {
  node: OverviewNode;
  selected: boolean;
  onSelect: (id: WaveSelection) => void;
}) {
  const captionId = useId();
  const tone = selected
    ? "border-accent ring-2 ring-accent-soft"
    : `${node.adaptive ? "border-dashed border-line-strong" : "border-line"} hover:border-line-strong hover:shadow-ui-2 dark:border-transparent dark:hover:border-line-strong dark:hover:shadow-none`;
  return (
    <button
      type="button"
      aria-label={
        node.adaptive ? `${node.label} (${node.adaptive})` : node.label
      }
      aria-describedby={node.caption ? captionId : undefined}
      title={node.condition ?? node.label}
      onClick={() => onSelect(node.id)}
      className={`flex w-full items-center gap-3 rounded-ui-md border bg-surface dark:bg-sunken dark:shadow-none px-3 py-2.5 text-left shadow-ui-1 transition-[border-color,box-shadow] duration-150 ${tone} ${FOCUS}`}
    >
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-1.5">
          {node.adaptive && (
            <Icon name="adaptive" className="h-3.5 w-3.5 text-ink-muted" />
          )}
          <span className="truncate text-ui-label font-medium text-ink">
            {node.label}
          </span>
          <NodeTags node={node} />
        </span>
        {node.caption && (
          <span
            id={captionId}
            className="mt-0.5 block truncate text-ui-meta text-ink-muted"
          >
            {node.caption}
          </span>
        )}
      </span>
      <AgentDots agents={node.agents} />
    </button>
  );
}

// ── Atoms ───────────────────────────────────────────────────────────

const AGENT_COLORS: Record<string, string> = {
  claude: "bg-[#D97757]",
  codex: "bg-ink dark:bg-[#10A37F]",
  antigravity: "bg-[#4285F4]",
  local: "bg-[#8A8A8A]",
};

function AgentDots({ agents }: { agents: string[] }) {
  if (!agents.length) return null;
  return (
    <span
      className="inline-flex flex-none items-center gap-1"
      title={agents.map(agentName).join(" + ")}
      aria-hidden="true"
    >
      {agents.map((agent) => (
        <span
          key={agent}
          className={`h-2 w-2 rounded-full ${AGENT_COLORS[agent] ?? AGENT_COLORS.local}`}
        />
      ))}
    </span>
  );
}

function NodeTags({ node }: { node: OverviewNode }) {
  return (
    <>
      {node.tools.includes("web") && <Tag icon="web">Web</Tag>}
      {node.fanOut !== null && (
        <Tag icon="fanout" title={`Fans out up to ${node.fanOut} times`}>
          ×{node.fanOut}
        </Tag>
      )}
      {node.condition && (
        <Tag icon="condition" title={node.condition}>
          If
        </Tag>
      )}
    </>
  );
}

function Tag({
  icon,
  title,
  children,
}: {
  icon: IconName;
  title?: string;
  children: ReactNode;
}) {
  return (
    <span
      title={title}
      className="inline-flex flex-none items-center gap-1 rounded-full border border-line px-1.5 text-ui-meta text-ink-muted"
    >
      <Icon name={icon} className="h-3 w-3" />
      {children}
    </span>
  );
}

type IconName =
  "extract" | "orient" | "merge" | "adaptive" | "web" | "fanout" | "condition";

const ICONS: Record<IconName, ReactNode> = {
  extract: (
    <>
      <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
      <path d="M14 3v5h5M9 13h6M9 17h4" />
    </>
  ),
  orient: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="m15.5 8.5-2 5-5 2 2-5z" />
    </>
  ),
  merge: (
    <>
      <circle cx="6" cy="5" r="2" />
      <circle cx="18" cy="5" r="2" />
      <circle cx="12" cy="19" r="2" />
      <path d="M6 7v1.5c0 3.5 6 3.5 6 7V17M18 7v1.5c0 3.5-6 3.5-6 7" />
    </>
  ),
  adaptive: (
    <>
      <path d="M11 3.5l1.8 4.7 4.7 1.8-4.7 1.8L11 16.5l-1.8-4.7L4.5 10l4.7-1.8z" />
      <path d="M18.5 14.5l.8 2 2 .8-2 .8-.8 2-.8-2-2-.8 2-.8z" />
    </>
  ),
  web: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18M12 3c2.4 2.6 3.6 5.6 3.6 9s-1.2 6.4-3.6 9c-2.4-2.6-3.6-5.6-3.6-9S9.6 5.6 12 3z" />
    </>
  ),
  fanout: (
    <>
      <path d="m12 3 9 5-9 5-9-5z" />
      <path d="m3 13 9 5 9-5" />
    </>
  ),
  condition: <path d="M4 5h16l-6 7.5V19l-4 2v-8.5z" />,
};

function Icon({ name, className }: { name: IconName; className: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`flex-none ${className}`}
      aria-hidden="true"
    >
      {ICONS[name]}
    </svg>
  );
}

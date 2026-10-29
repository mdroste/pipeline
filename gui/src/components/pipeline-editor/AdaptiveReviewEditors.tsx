import { adaptiveAgentCountLabel } from "../../lib/autoReview";
import type { AdaptiveAgentRange } from "../../lib/autoReview";

type AdaptiveSlotKind = "subject" | "method";

export function AdaptiveAgentsRow({
  count,
  range,
  selected,
  onSelect,
}: {
  count: number | null;
  range: AdaptiveAgentRange;
  selected: boolean;
  onSelect: () => void;
}) {
  const countLabel = adaptiveAgentCountLabel(count, range);
  return (
    <button
      type="button"
      aria-label={`Adaptive agents — ${count === null ? `Automatic, ${countLabel} agents` : `${count} agents`}`}
      onClick={onSelect}
      className={`w-full border-b border-blue-100 px-4 py-2.5 text-left dark:border-blue-950/60 ${
        selected
          ? "bg-blue-100/80 dark:bg-blue-950/60"
          : "bg-blue-50/40 hover:bg-blue-50 dark:bg-blue-950/15 dark:hover:bg-blue-950/35"
      }`}
    >
      <span className="flex items-center justify-between gap-2">
        <span className="text-sm font-medium text-blue-800 dark:text-blue-200">
          Adaptive agents
        </span>
        <span className="shrink-0 rounded-full bg-blue-100 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-blue-700 dark:bg-blue-900/60 dark:text-blue-200">
          {countLabel} agents
        </span>
      </span>
    </button>
  );
}

export function AdaptiveAgentsEditorPanel({
  count,
  range,
  coreStepLabels,
  onCountChange,
  onBrowse,
}: {
  count: number | null;
  range: AdaptiveAgentRange;
  coreStepLabels: string[];
  onCountChange: (count: number | null) => void;
  onBrowse: (kind: AdaptiveSlotKind) => void;
}) {
  const automaticRange = `${range.totalMin}–${range.totalMax}`;
  return (
    <div className="flex-1 overflow-y-auto">
      <div className="mx-auto max-w-xl space-y-6 p-5">
        <div>
          <h3 className="text-base font-semibold text-ink">Adaptive agents</h3>
          <p className="mt-1 text-ui-body text-ink-muted">
            Pipeline selects specialists after assessing the paper. Each reviews
            it independently.
          </p>
        </div>

        <div>
          <label
            htmlFor="adaptive-agent-count"
            className="block text-ui-label font-medium text-ink"
          >
            Number of agents
          </label>
          <select
            id="adaptive-agent-count"
            value={count ?? "automatic"}
            onChange={(event) => {
              onCountChange(
                event.target.value === "automatic"
                  ? null
                  : Number(event.target.value),
              );
            }}
            className="mt-1.5 block w-full max-w-xs rounded-ui-sm border border-line-strong bg-raised px-3 py-2 text-ui-body text-ink outline-none focus:ring-2 focus:ring-accent-soft"
          >
            <option value="automatic">Automatic ({automaticRange})</option>
            {Array.from(
              { length: range.totalMax - range.totalMin + 1 },
              (_, index) => range.totalMin + index,
            ).map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
          <p className="mt-1.5 text-ui-meta text-ink-muted">
            Automatic uses the fewest that cover the paper.
          </p>
        </div>

        <dl className="divide-y divide-line rounded-ui-md border border-line">
          <SpecialistRow
            label="Subject specialists"
            min={range.subjectMin}
            max={range.subjectMax}
            onBrowse={() => onBrowse("subject")}
            browseLabel="Browse subject catalog"
          />
          <SpecialistRow
            label="Method specialists"
            min={range.methodMin}
            max={range.methodMax}
            onBrowse={() => onBrowse("method")}
            browseLabel="Browse method catalog"
          />
        </dl>

        {coreStepLabels.length > 0 && (
          <p className="text-ui-meta text-ink-muted">
            Their reports are consolidated alongside {coreStepLabels.join(", ")}
            .
          </p>
        )}
      </div>
    </div>
  );
}

function SpecialistRow({
  label,
  min,
  max,
  onBrowse,
  browseLabel,
}: {
  label: string;
  min: number;
  max: number;
  onBrowse: () => void;
  browseLabel: string;
}) {
  return (
    <div className="flex items-center gap-3 px-4 py-3">
      <dt className="flex-1 text-ui-label text-ink">{label}</dt>
      <dd className="text-ui-label tabular-nums text-ink-muted">
        {min === max ? min : `${min}–${max}`} per paper
      </dd>
      <dd>
        <button
          type="button"
          onClick={onBrowse}
          aria-label={browseLabel}
          className="rounded-ui-sm px-2 py-1 text-ui-label font-medium text-accent hover:bg-sunken"
        >
          Browse
        </button>
      </dd>
    </div>
  );
}

export type { AdaptiveSlotKind };

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
      <span className="mt-0.5 block text-[11px] text-blue-700/80 dark:text-blue-300/80">
        Subject and method specialists selected from the paper’s orientation
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
  const methodRange =
    range.methodMin === range.methodMax
      ? `${range.methodMin}`
      : `${range.methodMin} to ${range.methodMax}`;
  return (
    <div className="flex-1 overflow-y-auto">
      <div className="mx-auto max-w-3xl space-y-5 p-5">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-base font-semibold text-gray-900 dark:text-gray-100">
              Adaptive agents
            </h3>
            <span className="rounded-full bg-blue-50 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-blue-700 dark:bg-blue-950/50 dark:text-blue-300">
              Auto-selected
            </span>
          </div>
          <p className="mt-2 text-sm leading-6 text-gray-600 dark:text-gray-300">
            Orientation selects the subject and method specialists best matched
            to the paper, and classifies the document’s genre — every reviewer
            receives that classification as shared context. Each selected role
            becomes an independent parallel review step.
          </p>
        </div>

        <section className="rounded-xl border border-blue-200 bg-blue-50/50 p-4 dark:border-blue-900 dark:bg-blue-950/20">
          <label
            htmlFor="adaptive-agent-count"
            className="text-xs font-semibold uppercase tracking-wide text-blue-800 dark:text-blue-200"
          >
            Number of adaptive agents
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
            className="mt-2 block w-full max-w-xs rounded-lg border border-blue-300 bg-white px-3 py-2 text-sm text-gray-900 outline-none focus:ring-2 focus:ring-blue-200 dark:border-blue-800 dark:bg-gray-900 dark:text-gray-100 dark:focus:ring-blue-950"
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
          <p className="mt-2 text-xs leading-5 text-blue-900/80 dark:text-blue-200/80">
            A fixed count is enforced by the validated review plan. Automatic
            lets the router use the smallest set that covers the paper, from{" "}
            {range.totalMin} to {range.totalMax} agents.
          </p>
        </section>

        <div className="grid gap-4 md:grid-cols-2">
          <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-800 dark:bg-gray-900">
            <h4 className="text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">
              Subject specialists
            </h4>
            <p className="mt-2 text-sm leading-6 text-gray-700 dark:text-gray-300">
              One primary subject specialist is always selected. A second may
              cover genuinely interdisciplinary work.
            </p>
            <button
              type="button"
              onClick={() => onBrowse("subject")}
              className="mt-3 text-xs font-medium text-blue-700 underline decoration-blue-300 underline-offset-2 hover:text-blue-900 dark:text-blue-300 dark:hover:text-blue-100"
            >
              Browse subject catalog
            </button>
          </section>

          <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-800 dark:bg-gray-900">
            <h4 className="text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">
              Method specialists
            </h4>
            <p className="mt-2 text-sm leading-6 text-gray-700 dark:text-gray-300">
              {methodRange} method reviewer{range.methodMax === 1 ? "" : "s"}{" "}
              scrutinize the methods that materially support the paper’s central
              claims.
            </p>
            <button
              type="button"
              onClick={() => onBrowse("method")}
              className="mt-3 text-xs font-medium text-blue-700 underline decoration-blue-300 underline-offset-2 hover:text-blue-900 dark:text-blue-300 dark:hover:text-blue-100"
            >
              Browse method catalog
            </button>
          </section>
        </div>

        <section className="rounded-xl border border-green-200 bg-green-50/60 p-4 dark:border-green-950 dark:bg-green-950/20">
          <h4 className="text-xs font-semibold uppercase tracking-wide text-green-700 dark:text-green-300">
            Synthesis input
          </h4>
          <p className="mt-2 text-sm leading-6 text-green-900 dark:text-green-100">
            Every adaptive-agent report feeds directly into{" "}
            <span className="font-medium">Consolidate Feedback</span>
            {coreStepLabels.length > 0
              ? `, alongside ${coreStepLabels.join(", ")}`
              : ""}
            . Validate Feedback then checks the consolidated report against the
            paper.
          </p>
        </section>
      </div>
    </div>
  );
}

export type { AdaptiveSlotKind };

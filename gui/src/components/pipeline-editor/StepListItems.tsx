import { memo } from "react";
import type { StepConfig } from "../../lib/types";

function StepRow({
  step,
  summary,
  selected,
  onToggle,
  onSelect,
  onDelete,
  canMoveUp,
  canMoveDown,
  onMoveUp,
  onMoveDown,
}: {
  step: StepConfig;
  summary: string;
  selected: boolean;
  onToggle: () => void;
  onSelect: () => void;
  onDelete?: () => void;
  canMoveUp?: boolean;
  canMoveDown?: boolean;
  onMoveUp?: () => void;
  onMoveDown?: () => void;
}) {
  const badges: string[] = [];
  if (step.tools?.includes("WebSearch")) badges.push("web search");
  if (step.agents?.length) {
    badges.push(...step.agents.map((a) => a.charAt(0).toUpperCase() + a.slice(1)));
  }
  if (step.model) badges.push(`model: ${step.model}`);
  if (step.effort) badges.push(`effort: ${step.effort}`);
  if (Object.keys(step.model_overrides ?? {}).length) {
    badges.push(`${Object.keys(step.model_overrides ?? {}).length} model ${Object.keys(step.model_overrides ?? {}).length === 1 ? "policy" : "policies"}`);
  }
  if (Object.keys(step.effort_overrides ?? {}).length) {
    badges.push(`${Object.keys(step.effort_overrides ?? {}).length} effort ${Object.keys(step.effort_overrides ?? {}).length === 1 ? "override" : "overrides"}`);
  }

  return (
    <div
      className={`border-b border-gray-100 dark:border-gray-800 ${
        selected ? "bg-blue-50 dark:bg-blue-950" : "hover:bg-gray-50 dark:hover:bg-gray-800"
      }`}
    >
      <div className="flex items-center gap-2 px-2 py-3">
        {step.phase === "sequential" && onMoveUp && onMoveDown && (
          <span className="flex shrink-0 gap-0.5">
            <button
              type="button"
              onClick={onMoveUp}
              disabled={!canMoveUp}
              aria-label={`Move ${step.label} up`}
              className="flex h-6 w-6 items-center justify-center text-[10px] leading-none text-gray-600
                         hover:text-gray-900 disabled:opacity-25 dark:text-gray-400 dark:hover:text-gray-100"
            >
              ▲
            </button>
            <button
              type="button"
              onClick={onMoveDown}
              disabled={!canMoveDown}
              aria-label={`Move ${step.label} down`}
              className="flex h-6 w-6 items-center justify-center text-[10px] leading-none text-gray-600
                         hover:text-gray-900 disabled:opacity-25 dark:text-gray-400 dark:hover:text-gray-100"
            >
              ▼
            </button>
          </span>
        )}
        <button
          type="button"
          role="switch"
          aria-label={`Enable ${step.label}`}
          aria-checked={step.enabled}
          onClick={onToggle}
          className={`relative h-4 w-7 shrink-0 rounded-full transition-colors ${
            step.enabled ? "bg-green-600" : "bg-gray-300 dark:bg-gray-600"
          }`}
        >
          <div className={`absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform ${
            step.enabled ? "translate-x-3.5" : "translate-x-0.5"
          }`} />
        </button>
        <button type="button" aria-label={step.label} onClick={onSelect} className="min-w-0 flex-1 text-left">
          <span className="block truncate text-sm font-medium text-gray-800 dark:text-gray-200">
            {step.label}
          </span>
          <span className="mt-0.5 block truncate text-[11px] font-normal text-gray-500 dark:text-gray-400" title={summary}>
            {summary}
          </span>
        </button>
        <button
          type="button"
          onClick={onDelete}
          aria-label={`Remove ${step.label}`}
          className="p-1 text-gray-500 hover:text-red-600 shrink-0"
        >
          <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
            <path fillRule="evenodd" d="M8.75 1A2.75 2.75 0 006 3.75v.443c-.795.077-1.584.176-2.365.298a.75.75 0 10.23 1.482l.149-.022.841 10.518A2.75 2.75 0 007.596 19h4.807a2.75 2.75 0 002.742-2.53l.841-10.519.149.023a.75.75 0 00.23-1.482A41.03 41.03 0 0014 4.193V3.75A2.75 2.75 0 0011.25 1h-2.5zM10 4c.84 0 1.673.025 2.5.075V3.75c0-.69-.56-1.25-1.25-1.25h-2.5c-.69 0-1.25.56-1.25 1.25v.325C8.327 4.025 9.16 4 10 4zM8.58 7.72a.75.75 0 00-1.5.06l.3 7.5a.75.75 0 101.5-.06l-.3-7.5zm4.34.06a.75.75 0 10-1.5-.06l-.3 7.5a.75.75 0 101.5.06l.3-7.5z" />
          </svg>
        </button>
      </div>
      {badges.length > 0 && (
        <div className="flex gap-1.5 px-4 pb-2 flex-wrap">
          {badges.map((b) => (
            <span key={b} className={`text-[10px] px-1.5 py-0.5 rounded ${
              b === "web search" ? "bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300" :
              ["Claude", "Codex", "Antigravity"].includes(b) ? "bg-purple-100 text-purple-700 dark:bg-purple-900 dark:text-purple-300" :
              b.startsWith("model:") || b.startsWith("effort:") ? "bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300 font-mono" :
              "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300"
            }`}>{b}</span>
          ))}
        </div>
      )}
    </div>
  );
}

// --- Model / effort overrides ---
//
// Per-step overrides that take precedence over the global Settings values for
// the active provider. Empty string = inherit. Showing the inputs collapsed by
// default keeps the editor uncluttered for the common case where users don't
// override anything.

export const MemoizedStepRow = memo(StepRow);

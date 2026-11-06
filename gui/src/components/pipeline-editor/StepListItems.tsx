import { memo } from "react";
import { MenuItem, MenuSeparator } from "../../ui";
import DesignerActionMenu from "./DesignerActionMenu";
import { PROVIDER_LABELS, type Provider } from "../../lib/providers";
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
    badges.push(...step.agents.map((a) => PROVIDER_LABELS[a as Provider] ?? a));
  }
  if (step.model) badges.push(`model: ${step.model}`);
  if (step.effort) badges.push(`effort: ${step.effort}`);
  if (Object.keys(step.model_overrides ?? {}).length) {
    badges.push(
      `${Object.keys(step.model_overrides ?? {}).length} model ${Object.keys(step.model_overrides ?? {}).length === 1 ? "policy" : "policies"}`,
    );
  }
  if (Object.keys(step.effort_overrides ?? {}).length) {
    badges.push(
      `${Object.keys(step.effort_overrides ?? {}).length} effort ${Object.keys(step.effort_overrides ?? {}).length === 1 ? "override" : "overrides"}`,
    );
  }

  const details = [...badges, summary].filter(Boolean).join(" · ");
  return (
    <div
      className="designer-step designer-row"
      data-selected={selected || undefined}
    >
      <button
        type="button"
        aria-label={step.label}
        aria-pressed={selected}
        onClick={onSelect}
        className="min-w-0 flex-1 py-2 text-left"
      >
        <span
          className={`block text-ui-body font-medium ${step.enabled ? "text-ink" : "text-ink-muted"}`}
        >
          {step.label}
        </span>
        {details && <span className="designer-detail">{details}</span>}
      </button>
      <button
        type="button"
        role="switch"
        aria-label={`Enable ${step.label}`}
        aria-checked={step.enabled}
        onClick={onToggle}
        className="designer-switch"
      >
        <span aria-hidden="true" />
      </button>
      <DesignerActionMenu label={`Actions for ${step.label}`}>
        {onMoveUp && onMoveDown && (
          <>
            <MenuItem disabled={!canMoveUp} onSelect={onMoveUp}>
              Move up
            </MenuItem>
            <MenuItem disabled={!canMoveDown} onSelect={onMoveDown}>
              Move down
            </MenuItem>
            <MenuSeparator />
          </>
        )}
        <MenuItem danger onSelect={onDelete}>
          Remove step
        </MenuItem>
      </DesignerActionMenu>
    </div>
  );
}

export const MemoizedStepRow = memo(StepRow);

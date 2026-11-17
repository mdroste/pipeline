import { useRef, useState } from "react";
import Button from "../../ui/Button";
import { Menu, MenuItem, MenuSeparator } from "../../ui/Menu";
import type { StepConfig } from "../../lib/workflowTypes";
import type { StepActions } from "./useStepActions";

/** Mutations belong to the selected editor, including Pipeline checks. */
export default function StepControls({
  step,
  steps,
  actions,
}: {
  step: StepConfig;
  steps: StepConfig[];
  actions: StepActions;
}) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const sequential = steps.filter((item) => item.phase === "sequential");
  const position = sequential.findIndex((item) => item.id === step.id);
  return (
    <div className="flex flex-wrap items-center gap-3">
      <button
        type="button"
        role="switch"
        aria-label={"Enable " + step.label}
        aria-checked={step.enabled}
        onClick={() => actions.updateStepEnabled(step.id, !step.enabled)}
        className="ui-toggle gap-2 text-ui-label"
      >
        <span aria-hidden="true" className="ui-toggle-track">
          <span className="ui-toggle-thumb" />
        </span>
        Enabled
      </button>
      <Button
        size="sm"
        ref={trigger}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        Step actions
      </Button>
      <Menu
        anchorRef={trigger}
        open={open}
        onClose={() => setOpen(false)}
        label="Step actions"
        align="end"
      >
        {position >= 0 && (
          <>
            <MenuItem
              disabled={position === 0}
              onSelect={() => actions.moveSequentialStep(step.id, -1)}
            >
              Move up
            </MenuItem>
            <MenuItem
              disabled={position === sequential.length - 1}
              onSelect={() => actions.moveSequentialStep(step.id, 1)}
            >
              Move down
            </MenuItem>
            <MenuSeparator />
          </>
        )}
        <MenuItem danger onSelect={() => actions.removeStep(step.id)}>
          Remove step
        </MenuItem>
      </Menu>
    </div>
  );
}

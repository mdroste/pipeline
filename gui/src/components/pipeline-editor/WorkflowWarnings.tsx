import type { StepConfig } from "../../lib/types";
import type { WorkflowLintWarning } from "../../lib/pipelineHelpers";

export default function WorkflowWarnings({
  warnings,
  steps,
  onSelect,
}: {
  warnings: WorkflowLintWarning[];
  steps: StepConfig[];
  onSelect: (warning: WorkflowLintWarning) => void;
}) {
  if (!warnings.length) return null;
  const groups = [...new Set(warnings.map((warning) => warning.stepId))];
  return (
    <details
      open
      className="mx-4 mb-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200"
    >
      <summary className="font-semibold">
        Step references · {warnings.length}{" "}
        {warnings.length === 1 ? "warning" : "warnings"}
      </summary>
      {groups.map((id) => (
        <div key={id} className="mt-3">
          <h3 className="font-medium">
            {steps.find((step) => step.id === id)?.label || id}
          </h3>
          <ul className="mt-1 space-y-2">
            {warnings
              .filter((warning) => warning.stepId === id)
              .map((warning, index) => (
                <li key={index}>
                  <button
                    type="button"
                    className="text-left underline underline-offset-2"
                    onClick={() => onSelect(warning)}
                  >
                    {
                      {
                        prompt: "Prompt",
                        condition: "Run condition",
                        iteration: "Repeated items",
                      }[warning.field]
                    }
                    : {warning.message}
                  </button>
                </li>
              ))}
          </ul>
        </div>
      ))}
    </details>
  );
}

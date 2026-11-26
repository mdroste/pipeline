import { useEffect, useState } from "react";
import type { PipelineConfig } from "../../lib/workflowTypes";
import { useSetupReads } from "../reviewSetupReads";
import RunPlanPreview from "../RunPlanPreview";
import ReviewSetupDialog from "./ReviewSetupDialog";

export default function ReviewWorkflowSteps({
  refreshKey,
  inputMode,
  hasInput,
  disabled,
  onConfigure,
}: {
  refreshKey: number;
  inputMode: string;
  hasInput: boolean;
  disabled: boolean;
  onConfigure: (stageId?: string) => void;
}) {
  const reads = useSetupReads(refreshKey);
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [page, setPage] = useState(0);
  const [diagram, setDiagram] = useState(false);
  useEffect(() => {
    let live = true;
    setConfig(null);
    setError(null);
    reads
      .config()
      .then((value) => {
        if (live) setConfig(value);
      })
      .catch((cause) => {
        if (live) setError(String(cause));
      });
    return () => {
      live = false;
    };
  }, [reads, attempt]);
  if (error)
    return (
      <div role="alert">
        <p>Couldn't load review steps. {error}</p>
        <button onClick={() => setAttempt((n) => n + 1)}>Retry steps</button>
      </div>
    );
  if (!config) return <p role="status">Loading review steps…</p>;
  const steps = config.steps.filter((step) => step.enabled);
  const pages = Math.max(1, Math.ceil(steps.length / 4));
  const current = Math.min(page, pages - 1);
  return (
    <>
      <div className="review-section-heading">
        <h3>{steps.length} configured steps</h3>
        <button type="button" onClick={() => setDiagram(true)}>
          View diagram
        </button>
      </div>
      <ol className="review-step-list" start={current * 4 + 1}>
        {steps.slice(current * 4, current * 4 + 4).map((step) => (
          <li key={step.id}>
            <button
              type="button"
              disabled={disabled}
              onClick={() => onConfigure(step.id)}
            >
              <strong title={step.label}>{step.label}</strong>
              <span>
                {step.phase === "parallel" ? "Parallel" : "Sequential"} ·{" "}
                {step.kind === "analyzer" ? "Analysis" : "Review"}
              </span>
            </button>
          </li>
        ))}
      </ol>
      {pages > 1 && (
        <div className="review-pagination">
          <button
            type="button"
            disabled={!current}
            onClick={() => setPage(current - 1)}
          >
            Previous
          </button>
          <span aria-live="polite">
            {current + 1} / {pages}
          </span>
          <button
            type="button"
            disabled={current === pages - 1}
            onClick={() => setPage(current + 1)}
          >
            Next
          </button>
        </div>
      )}
      {diagram && (
        <ReviewSetupDialog
          title="Workflow steps"
          onClose={() => setDiagram(false)}
        >
          <RunPlanPreview
            refreshKey={refreshKey}
            inputMode={inputMode}
            hasInput={hasInput}
            onOpenDesigner={(stageId) => {
              if (!disabled) {
                setDiagram(false);
                onConfigure(stageId);
              }
            }}
          />
        </ReviewSetupDialog>
      )}
    </>
  );
}

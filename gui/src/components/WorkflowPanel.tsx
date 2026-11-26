import { workflowDescription } from "../lib/workflowDescriptions";
import { useSetupReads } from "./reviewSetupReads";
import { useState, useEffect, useId } from "react";
import { useReviewSetupEnvironment } from "./reviewSetupEnvironment";
import type { PipelineConfig, ProfileSummary } from "../lib/workflowTypes";
import ReportIssueLink from "./ReportIssueLink";
import ReviewWorkflowComparison from "./review-setup/ReviewWorkflowComparison";
import {
  adaptiveAgentCount,
  adaptiveAgentCountLabel,
  adaptiveAgentRange,
  isAutoReview,
} from "../lib/autoReview";

interface Props {
  disabled: boolean;
  onConfigure: () => void;
  onProfileChange: (config?: PipelineConfig) => void;
  refreshKey: number;
}

export default function WorkflowPanel({
  disabled,
  onConfigure,
  onProfileChange,
  refreshKey,
}: Props) {
  const selectId = useId();
  const [comparing, setComparing] = useState(false);
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [activeId, setActiveId] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [switching, setSwitching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loadAttempt, setLoadAttempt] = useState(0);

  const reads = useSetupReads(refreshKey);
  const environment = useReviewSetupEnvironment();
  useEffect(() => {
    let stale = false;
    setLoading(true);
    setError(null);
    reads
      .workflow()
      .then(([c, p, active]) => {
        if (stale) return;
        setConfig(c);
        setProfiles(p);
        setActiveId(active);
        setLoading(false);
      })
      .catch((caught) => {
        if (stale) return;
        console.error(caught);
        setError(caught instanceof Error ? caught.message : String(caught));
        setLoading(false);
      });
    return () => {
      stale = true;
    };
  }, [reads, loadAttempt]);

  const handleSwitch = async (id: string) => {
    setSwitching(true);
    setError(null);
    try {
      const newConfig = await environment.switchWorkflow({ id });
      setConfig(newConfig);
      setActiveId(id);
      onProfileChange(newConfig);
      setComparing(false);
    } catch (e) {
      console.error("Failed to switch profile:", e);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSwitching(false);
    }
  };

  if (loading) {
    return (
      <div
        role="status"
        className="border-t border-gray-200 pt-4 text-xs text-gray-500 dark:border-line dark:text-gray-400"
      >
        Loading review design…
      </div>
    );
  }
  if (error || !config) {
    return (
      <div
        role="alert"
        className="border-t border-gray-200 pt-4 text-xs text-danger dark:border-line"
      >
        <p>Review design unavailable{error ? `: ${error}` : "."}</p>
        <button
          type="button"
          onClick={() => setLoadAttempt((attempt) => attempt + 1)}
          className="mt-1 font-medium underline"
        >
          Retry
        </button>
        <ReportIssueLink />
      </div>
    );
  }

  return (
    <div className="review-workflow-picker">
      <div className="mb-2 flex min-w-0 items-center justify-between gap-2">
        <label
          htmlFor={selectId}
          className="text-ui-label font-medium text-ink"
        >
          Review design
        </label>
        <div className="review-workflow-actions">
          <button
            type="button"
            className="ui-button ui-button--quiet ui-button--sm"
            aria-label="Compare review designs"
            disabled={disabled || switching}
            onClick={() => setComparing(true)}
          >
            Compare
          </button>
          <button
            type="button"
            onClick={onConfigure}
            disabled={disabled}
            className="ui-button ui-button--link ui-button--sm"
          >
            Edit design
          </button>
        </div>
      </div>
      <select
        id={selectId}
        value={activeId}
        onChange={(e) => handleSwitch(e.target.value)}
        disabled={disabled || switching}
        title={switching ? "Switching review design" : undefined}
        className="w-full"
      >
        {profiles.map((p) => (
          <option key={p.id} value={p.id}>
            {p.invalid ? `${p.name} (needs repair)` : p.name}
          </option>
        ))}
      </select>
      {profiles.find((p) => p.id === activeId) && (
        <div className="mt-2.5 space-y-1.5 text-ui-meta leading-5 text-ink-muted">
          <p>
            {
              workflowDescription(profiles.find((p) => p.id === activeId)!)
                .outcome
            }
          </p>
          {isAutoReview(config) && (
            <p>
              This design selects{" "}
              {adaptiveAgentCountLabel(
                adaptiveAgentCount(config),
                adaptiveAgentRange(config),
              )}{" "}
              specialist roles for each document. Model settings choose which
              models perform those roles.
            </p>
          )}
        </div>
      )}
      {comparing && (
        <ReviewWorkflowComparison
          profiles={profiles}
          activeId={activeId}
          disabled={disabled || switching}
          onSelect={(id) => void handleSwitch(id)}
          onClose={() => setComparing(false)}
        />
      )}
      {profiles.find((p) => p.id === activeId)?.invalid && (
        <p role="alert" className="mt-1 text-[0.6875rem] text-warning">
          {profiles.find((p) => p.id === activeId)?.invalid}
        </p>
      )}
      {switching && (
        <p
          role="status"
          className="mt-1 text-[0.6875rem] text-gray-500 dark:text-gray-400"
        >
          Switching review design…
        </p>
      )}
    </div>
  );
}

import { workflowDescription } from "../lib/workflowDescriptions";
import {
  adaptiveAgentCountLabel,
  adaptiveAgentCount,
  adaptiveAgentRange,
  isAutoReview,
} from "../lib/autoReview";
import { useSetupReads } from "./reviewSetupReads";
import { useState, useEffect, useId } from "react";
import { workflowProfileClient } from "../lib/workflowProfileClient";
import type { PipelineConfig, ProfileSummary } from "../lib/workflowTypes";
import ReportIssueLink from "./ReportIssueLink";

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
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [activeId, setActiveId] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [switching, setSwitching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loadAttempt, setLoadAttempt] = useState(0);

  const reads = useSetupReads(refreshKey);
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
      const newConfig = await workflowProfileClient.switch({ id });
      setConfig(newConfig);
      setActiveId(id);
      onProfileChange(newConfig);
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
        Loading workflow…
      </div>
    );
  }
  if (error || !config) {
    return (
      <div
        role="alert"
        className="border-t border-gray-200 pt-4 text-xs text-red-600 dark:border-line dark:text-red-400"
      >
        <p>Workflow unavailable{error ? `: ${error}` : "."}</p>
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
    <div className="border-t border-line pt-4">
      <div className="mb-2 flex min-w-0 items-center justify-between gap-2">
        <label
          htmlFor={selectId}
          className="text-ui-label font-medium text-ink"
        >
          Workflow
        </label>
        <button
          type="button"
          onClick={onConfigure}
          disabled={disabled}
          className="shrink-0 rounded-ui-sm px-1.5 py-1 text-ui-meta font-medium text-ink-muted transition-colors hover:bg-sunken hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50"
        >
          Edit workflow…
        </button>
      </div>
      <select
        id={selectId}
        value={activeId}
        onChange={(e) => handleSwitch(e.target.value)}
        disabled={disabled || switching}
        title={switching ? "Switching workflow" : undefined}
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
          <p>
            {config.extraction?.input_mode === "none"
              ? "No document needed"
              : config.extraction?.input_mode === "folder"
                ? "Folder input"
                : workflowDescription(profiles.find((p) => p.id === activeId)!)
                    .inputs}{" "}
            · {config.steps.filter((step) => step.enabled).length} configured
            steps
            {isAutoReview(config)
              ? " + " +
                adaptiveAgentCountLabel(
                  adaptiveAgentCount(config),
                  adaptiveAgentRange(config),
                ) +
                " specialists"
              : ""}
          </p>
        </div>
      )}
      <details className="mt-3 text-ui-meta">
        <summary className="review-disclosure">Compare workflows</summary>
        {Array.from(
          new Set(profiles.map((p) => workflowDescription(p).category)),
        ).map((category) => (
          <section key={category} className="mt-3">
            <h3 className="mb-1 font-medium text-ink-muted">{category}</h3>
            {profiles
              .filter((p) => workflowDescription(p).category === category)
              .map((profile) => (
                <button
                  key={profile.id}
                  type="button"
                  disabled={disabled || switching || !!profile.invalid}
                  aria-pressed={profile.id === activeId}
                  onClick={() => void handleSwitch(profile.id)}
                  className="mb-2 block w-full rounded-lg border border-line p-3 text-left hover:bg-sunken aria-pressed:bg-sunken disabled:opacity-50"
                >
                  <span className="font-medium">
                    {profile.name}
                    {profile.id === activeId ? " · Selected" : ""}
                  </span>
                  <span className="mt-1 block text-ink-muted">
                    {workflowDescription(profile).outcome}
                  </span>
                  <span className="mt-1 block text-ink-muted">
                    {workflowDescription(profile).inputs} · {profile.step_count}{" "}
                    configured steps{profile.invalid ? " · Needs repair" : ""}
                  </span>
                </button>
              ))}
          </section>
        ))}
      </details>
      {profiles.find((p) => p.id === activeId)?.invalid && (
        <p
          role="alert"
          className="mt-1 text-[0.6875rem] text-amber-800 dark:text-amber-200"
        >
          {profiles.find((p) => p.id === activeId)?.invalid}
        </p>
      )}
      {switching && (
        <p
          role="status"
          className="mt-1 text-[0.6875rem] text-gray-500 dark:text-gray-400"
        >
          Switching workflow…
        </p>
      )}
    </div>
  );
}

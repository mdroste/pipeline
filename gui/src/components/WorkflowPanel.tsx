import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PipelineConfig, ProfileSummary, StepConfig } from "../lib/types";
import { computeWaves } from "../lib/pipelineHelpers";
import {
  adaptiveAgentCount,
  adaptiveAgentCountLabel,
  isAutoReview,
} from "../lib/autoReview";
import AutoReviewCatalogDialog from "./AutoReviewCatalogDialog";

interface Props {
  disabled: boolean;
  /** The pipeline editor manages profiles while open — the switcher defers to it. */
  editorOpen: boolean;
  onConfigure: () => void;
  onProfileChange: (config?: PipelineConfig) => void;
  refreshKey: number;
}

export default function WorkflowPanel({
  disabled,
  editorOpen,
  onConfigure,
  onProfileChange,
  refreshKey,
}: Props) {
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [activeId, setActiveId] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [switching, setSwitching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [catalogOpen, setCatalogOpen] = useState(false);

  useEffect(() => {
    let stale = false;
    setLoading(true);
    setError(null);
    Promise.all([
      invoke<PipelineConfig>("get_pipeline_config"),
      invoke<ProfileSummary[]>("list_profiles"),
      invoke<string>("get_active_profile"),
    ])
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
  }, [refreshKey, loadAttempt]);

  const handleSwitch = async (id: string) => {
    setSwitching(true);
    setError(null);
    try {
      setCatalogOpen(false);
      const newConfig = await invoke<PipelineConfig>("switch_profile", { id });
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
      <div role="status" className="border-t border-gray-200 pt-4 text-xs text-gray-500 dark:border-gray-700 dark:text-gray-400">
        Loading workflow…
      </div>
    );
  }
  if (error || !config) {
    return (
      <div role="alert" className="border-t border-gray-200 pt-4 text-xs text-red-600 dark:border-gray-700 dark:text-red-400">
        <p>Workflow unavailable{error ? `: ${error}` : "."}</p>
        <button type="button" onClick={() => setLoadAttempt((attempt) => attempt + 1)} className="mt-1 font-medium underline">
          Retry
        </button>
      </div>
    );
  }

  const enabledSteps = config.steps.filter((s) => s.enabled);
  const disabledCount = config.steps.length - enabledSteps.length;
  const autoAssembled = isAutoReview(config);
  const autoReview = autoAssembled || activeId === "auto-review";
  const configuredAdaptiveCount = adaptiveAgentCount(config);

  const groups: { phase: StepConfig["phase"]; steps: StepConfig[] }[] = computeWaves(config.steps)
    .map((wave) => wave.kind === "parallel"
      ? { phase: "parallel" as const, steps: wave.steps }
      : { phase: "sequential" as const, steps: [wave.step] });

  return (
    <div className="border-t border-gray-200 dark:border-gray-700 pt-4">
      <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">
        Workflow
      </label>
      <select
        value={activeId}
        onChange={(e) => handleSwitch(e.target.value)}
        disabled={disabled || editorOpen || switching}
        title={
          editorOpen
            ? "The pipeline editor is open — switch profiles there"
            : switching
              ? "Switching workflow"
              : undefined
        }
        className="w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                   text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                   focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent
                   transition-colors disabled:opacity-50"
      >
        {profiles.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
          </option>
        ))}
      </select>
      {switching && <p role="status" className="mt-1 text-[11px] text-gray-500 dark:text-gray-400">Switching workflow…</p>}

      {/* Read-only summary — steps are managed in the pipeline editor */}
      <div className="mt-3">
        {groups.map((group, gi) => (
          <div key={gi} className={gi > 0 ? "mt-2.5" : ""}>
            <p className="text-[10px] uppercase tracking-wider text-gray-500 dark:text-gray-400 mb-0.5">
              {group.phase === "parallel" ? "Parallel" : "Sequential"}
            </p>
            {group.steps.filter((step) => !step.run_if).map((step) => (
              <div key={step.id} className="flex items-center gap-2 py-0.5">
                <span className="w-1.5 h-1.5 rounded-full bg-gray-300 dark:bg-gray-600 shrink-0" />
                <span className="text-xs text-gray-600 dark:text-gray-400 truncate">
                  {step.label}
                </span>
                {step.agents?.length > 1 && (
                  <span className="text-[10px] text-gray-500 dark:text-gray-400 shrink-0">
                    {step.agents.map((a) => a.charAt(0).toUpperCase()).join("+")}
                  </span>
                )}
              </div>
            ))}
            {autoReview && group.phase === "parallel" && gi === groups.findIndex((candidate) => candidate.phase === "parallel") && (
              <button
                type="button"
                onClick={() => setCatalogOpen(true)}
                className="flex w-full items-center gap-2 py-0.5 text-left"
                title="Browse the adaptive-agent catalog"
              >
                <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-blue-400 dark:bg-blue-500" />
                <span className="truncate text-xs font-medium text-blue-700 dark:text-blue-300">
                  Adaptive agents
                </span>
                <span className="ml-auto shrink-0 text-[10px] text-blue-600/80 dark:text-blue-300/80">
                  {adaptiveAgentCountLabel(configuredAdaptiveCount)}
                </span>
              </button>
            )}
            {!autoReview && (() => {
              const conditional = group.steps.filter((step) => !!step.run_if);
              if (conditional.length === 0) return null;
              return (
                <details className="mt-1 rounded-md border border-gray-200 px-2 py-1 dark:border-gray-700">
                  <summary className="cursor-pointer text-[11px] text-gray-600 dark:text-gray-400">
                    {conditional.length} conditional specialist{conditional.length === 1 ? "" : "s"}
                  </summary>
                  <div className="mt-1 border-t border-gray-100 pt-1 dark:border-gray-800">
                    {conditional.map((step) => (
                      <div key={step.id} className="flex items-center gap-2 py-0.5">
                        <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-gray-300 dark:bg-gray-600" />
                        <span className="truncate text-xs text-gray-600 dark:text-gray-400">
                          {step.label}
                        </span>
                      </div>
                    ))}
                  </div>
                </details>
              );
            })()}
          </div>
        ))}
        {enabledSteps.length === 0 && (
          <p className="text-xs text-gray-500 dark:text-gray-400">
            No steps enabled.
          </p>
        )}
        {autoReview && (
          <div className="mt-2 rounded-lg border border-blue-100 bg-blue-50/70 px-2.5 py-2 dark:border-blue-900/70 dark:bg-blue-950/25">
            <p className="text-[11px] leading-4 text-blue-900 dark:text-blue-200">
              Automatic: selects 2-6 additional field/methodology-specific review agents tailored for each document.
            </p>
            <button
              type="button"
              onClick={() => setCatalogOpen(true)}
              className="mt-1 text-[11px] font-medium text-blue-700 underline decoration-blue-300 underline-offset-2 hover:text-blue-900 dark:text-blue-300 dark:hover:text-blue-100"
            >
              Browse specialist catalog
            </button>
          </div>
        )}
        {disabledCount > 0 && (
          <p className="text-[11px] text-gray-500 dark:text-gray-400 mt-1.5">
            +{disabledCount} disabled step{disabledCount === 1 ? "" : "s"}
          </p>
        )}
      </div>

      <button
        onClick={onConfigure}
        disabled={disabled}
        className="w-full mt-2 py-1.5 text-xs text-gray-500 hover:text-gray-700
                   hover:bg-gray-50 dark:hover:bg-gray-800 dark:hover:text-gray-300
                   rounded transition-colors disabled:opacity-50"
      >
        Edit workflow…
      </button>
      {catalogOpen && <AutoReviewCatalogDialog onClose={() => setCatalogOpen(false)} />}
    </div>
  );
}

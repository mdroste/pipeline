import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PipelineConfig, ProfileSummary, StepConfig } from "../lib/types";
import { computeWaves } from "../lib/pipelineHelpers";

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
            {group.steps.map((step) => (
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
          </div>
        ))}
        {enabledSteps.length === 0 && (
          <p className="text-xs text-gray-500 dark:text-gray-400">
            No steps enabled.
          </p>
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
    </div>
  );
}

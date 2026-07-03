import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PipelineConfig, ProfileSummary, StepConfig } from "../lib/types";

interface Props {
  disabled: boolean;
  /** The pipeline editor manages profiles while open — the switcher defers to it. */
  editorOpen: boolean;
  onConfigure: () => void;
  onProfileChange: () => void;
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

  useEffect(() => {
    let stale = false;
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
      })
      .catch(console.error);
    return () => {
      stale = true;
    };
  }, [refreshKey]);

  const handleSwitch = async (id: string) => {
    try {
      const newConfig = await invoke<PipelineConfig>("switch_profile", { id });
      setConfig(newConfig);
      setActiveId(id);
      onProfileChange();
    } catch (e) {
      console.error("Failed to switch profile:", e);
    }
  };

  if (!config) return null;

  const enabledSteps = config.steps.filter((s) => s.enabled);
  const disabledCount = config.steps.length - enabledSteps.length;

  // Group consecutive same-phase steps in execution order, so alternating
  // pipelines (parallel wave → sequential → parallel wave) read correctly.
  const groups: { phase: StepConfig["phase"]; steps: StepConfig[] }[] = [];
  for (const step of enabledSteps) {
    const last = groups[groups.length - 1];
    if (last && last.phase === step.phase) last.steps.push(step);
    else groups.push({ phase: step.phase, steps: [step] });
  }

  return (
    <div className="border-t border-gray-200 dark:border-gray-700 pt-4">
      <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">
        Workflow
      </label>
      <select
        value={activeId}
        onChange={(e) => handleSwitch(e.target.value)}
        disabled={disabled || editorOpen}
        title={
          editorOpen
            ? "The pipeline editor is open — switch profiles there"
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

      {/* Read-only summary — steps are managed in the pipeline editor */}
      <div className="mt-3">
        {groups.map((group, gi) => (
          <div key={gi} className={gi > 0 ? "mt-2.5" : ""}>
            <p className="text-[10px] uppercase tracking-wider text-gray-400 mb-0.5">
              {group.phase === "parallel" ? "Parallel" : "Sequential"}
            </p>
            {group.steps.map((step) => (
              <div key={step.id} className="flex items-center gap-2 py-0.5">
                <span className="w-1.5 h-1.5 rounded-full bg-gray-300 dark:bg-gray-600 shrink-0" />
                <span className="text-xs text-gray-600 dark:text-gray-400 truncate">
                  {step.label}
                </span>
                {step.agents?.length > 1 && (
                  <span className="text-[10px] text-gray-400 dark:text-gray-500 shrink-0">
                    {step.agents.map((a) => a.charAt(0).toUpperCase()).join("+")}
                  </span>
                )}
              </div>
            ))}
          </div>
        ))}
        {enabledSteps.length === 0 && (
          <p className="text-xs text-gray-400 dark:text-gray-500">
            No steps enabled.
          </p>
        )}
        {disabledCount > 0 && (
          <p className="text-[11px] text-gray-400 dark:text-gray-500 mt-1.5">
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

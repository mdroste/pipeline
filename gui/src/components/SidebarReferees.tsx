import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PipelineConfig, ProfileSummary } from "../lib/types";

interface Props {
  disabled: boolean;
  onConfigure: () => void;
  refreshKey: number;
}

export default function SidebarReferees({ disabled, onConfigure, refreshKey }: Props) {
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [collapsed, setCollapsed] = useState(false);
  const [activeProfileName, setActiveProfileName] = useState<string>("");

  useEffect(() => {
    let stale = false;
    Promise.all([
      invoke<PipelineConfig>("get_pipeline_config"),
      invoke<ProfileSummary[]>("list_profiles"),
      invoke<string>("get_active_profile"),
    ])
      .then(([c, profiles, activeId]) => {
        if (stale) return;
        setConfig(c);
        const active = profiles.find((p) => p.id === activeId);
        setActiveProfileName(active?.name ?? activeId);
      })
      .catch(console.error);
    return () => { stale = true; };
  }, [refreshKey]);

  const toggle = async (id: string) => {
    if (!config || disabled) return;
    const previous = config;
    const updated = {
      ...config,
      steps: config.steps.map((s) =>
        s.id === id ? { ...s, enabled: !s.enabled } : s
      ),
    };
    setConfig(updated);
    try {
      await invoke("save_pipeline_config", { config: updated });
    } catch (e) {
      console.error("Failed to save:", e);
      setConfig(previous);
    }
  };

  if (!config) return null;

  const parallelSteps = config.steps.filter((s) => s.phase === "parallel");
  const sequentialSteps = config.steps.filter((s) => s.phase === "sequential");

  return (
    <div className="border-t border-gray-200 dark:border-gray-700 pt-4">
      <button
        onClick={() => setCollapsed(!collapsed)}
        className="flex items-center justify-between w-full text-left mb-2"
      >
        <span className="text-sm font-medium text-gray-700 dark:text-gray-300">
          Pipeline
          {activeProfileName && (
            <span className="text-xs text-gray-400 ml-1.5 font-normal">
              ({activeProfileName})
            </span>
          )}
        </span>
        <span className="text-xs text-gray-400">
          {collapsed ? "\u25BC" : "\u25B2"}
        </span>
      </button>

      {!collapsed && (
        <div className="space-y-1">
          {/* Parallel steps */}
          {parallelSteps.length > 0 && (
            <>
              <p className="text-[10px] uppercase tracking-wider text-gray-400 mt-1 mb-1">
                Parallel
              </p>
              {parallelSteps.map((step) => (
                <div key={step.id} className="flex items-center gap-2 py-1">
                  <button
                    onClick={() => toggle(step.id)}
                    disabled={disabled}
                    className={`w-7 h-4 rounded-full relative transition-colors shrink-0 ${
                      step.enabled ? "bg-green-500" : "bg-gray-300"
                    } ${disabled ? "opacity-50" : ""}`}
                  >
                    <div
                      className={`absolute top-0.5 w-3 h-3 rounded-full bg-white shadow transition-transform ${
                        step.enabled ? "translate-x-3.5" : "translate-x-0.5"
                      }`}
                    />
                  </button>
                  <span
                    className={`text-xs truncate ${
                      step.enabled ? "text-gray-700 dark:text-gray-300" : "text-gray-400 dark:text-gray-600"
                    }`}
                  >
                    {step.label}
                  </span>
                  {step.agents?.length > 0 && (
                    <span className="text-[10px] text-gray-400 dark:text-gray-500 shrink-0">
                      {step.agents.map((a) => a.charAt(0).toUpperCase()).join("+")}
                    </span>
                  )}
                </div>
              ))}
            </>
          )}

          {/* Merge indicator */}
          {config.merge?.enabled && parallelSteps.some((s) => s.agents?.length > 1) && (
            <>
              <p className="text-[10px] uppercase tracking-wider text-gray-400 mt-3 mb-1">
                Merge
              </p>
              <div className="flex items-center gap-2 py-1">
                <span className="text-xs text-gray-500 dark:text-gray-400">
                  Cross-agent merge
                </span>
                <span className="text-[10px] text-green-600 dark:text-green-400 ml-auto">auto</span>
              </div>
            </>
          )}

          {/* Sequential steps */}
          {sequentialSteps.length > 0 && (
            <>
              <p className="text-[10px] uppercase tracking-wider text-gray-400 mt-3 mb-1">
                Sequential
              </p>
              {sequentialSteps.map((step) => (
                <div key={step.id} className="flex items-center gap-2 py-1">
                  <button
                    onClick={() => toggle(step.id)}
                    disabled={disabled}
                    className={`w-7 h-4 rounded-full relative transition-colors shrink-0 ${
                      step.enabled ? "bg-green-500" : "bg-gray-300"
                    } ${disabled ? "opacity-50" : ""}`}
                  >
                    <div
                      className={`absolute top-0.5 w-3 h-3 rounded-full bg-white shadow transition-transform ${
                        step.enabled ? "translate-x-3.5" : "translate-x-0.5"
                      }`}
                    />
                  </button>
                  <span
                    className={`text-xs truncate ${
                      step.enabled ? "text-gray-700 dark:text-gray-300" : "text-gray-400 dark:text-gray-600"
                    }`}
                  >
                    {step.label}
                  </span>
                </div>
              ))}
            </>
          )}

          {/* Configure link */}
          <button
            onClick={onConfigure}
            disabled={disabled}
            className="w-full mt-2 py-1.5 text-xs text-gray-500 hover:text-gray-700
                       hover:bg-gray-50 rounded transition-colors disabled:opacity-50"
          >
            Customize pipeline steps...
          </button>
        </div>
      )}
    </div>
  );
}

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PipelineConfig, ProfileSummary } from "../lib/types";
import {
  adaptiveAgentCount,
  adaptiveAgentCountLabel,
  adaptiveAgentRange,
  isAutoReview,
} from "../lib/autoReview";
import AutoReviewCatalogDialog from "./AutoReviewCatalogDialog";

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
      <div
        role="status"
        className="border-t border-gray-200 pt-4 text-xs text-gray-500 dark:border-gray-700 dark:text-gray-400"
      >
        Loading workflow…
      </div>
    );
  }
  if (error || !config) {
    return (
      <div
        role="alert"
        className="border-t border-gray-200 pt-4 text-xs text-red-600 dark:border-gray-700 dark:text-red-400"
      >
        <p>Workflow unavailable{error ? `: ${error}` : "."}</p>
        <button
          type="button"
          onClick={() => setLoadAttempt((attempt) => attempt + 1)}
          className="mt-1 font-medium underline"
        >
          Retry
        </button>
      </div>
    );
  }

  const enabledSteps = config.steps.filter((s) => s.enabled);
  const disabledCount = config.steps.length - enabledSteps.length;
  const autoAssembled = isAutoReview(config);
  const autoReview =
    autoAssembled || ["auto-review", "auto-review-quick"].includes(activeId);
  const configuredAdaptiveCount = adaptiveAgentCount(config);
  const configuredAdaptiveRange = adaptiveAgentRange(config);

  return (
    <div className="border-t border-gray-200 dark:border-gray-700 pt-4">
      <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">
        Workflow
      </label>
      <select
        value={activeId}
        onChange={(e) => handleSwitch(e.target.value)}
        disabled={disabled || switching}
        title={switching ? "Switching workflow" : undefined}
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
      {switching && (
        <p
          role="status"
          className="mt-1 text-[11px] text-gray-500 dark:text-gray-400"
        >
          Switching workflow…
        </p>
      )}

      {autoReview && (
        <div className="mt-2 rounded-lg border border-blue-100 bg-blue-50/70 px-2.5 py-2 dark:border-blue-900/70 dark:bg-blue-950/25">
          <p className="text-[11px] leading-4 text-blue-900 dark:text-blue-200">
            Adaptive agents:{" "}
            {adaptiveAgentCountLabel(
              configuredAdaptiveCount,
              configuredAdaptiveRange,
            )}{" "}
            additional subject and method reviewers tailored to each document.
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
      <p className="mt-2 text-[11px] text-gray-500 dark:text-gray-400">
        {enabledSteps.length} step{enabledSteps.length === 1 ? "" : "s"}
        {disabledCount > 0 ? ` · ${disabledCount} disabled` : ""} — the full
        plan is shown beside this panel.
      </p>

      <button
        onClick={onConfigure}
        disabled={disabled}
        className="w-full mt-2 py-1.5 text-xs text-gray-500 hover:text-gray-700
                   hover:bg-gray-50 dark:hover:bg-gray-800 dark:hover:text-gray-300
                   rounded transition-colors disabled:opacity-50"
      >
        Edit workflow…
      </button>
      {catalogOpen && (
        <AutoReviewCatalogDialog onClose={() => setCatalogOpen(false)} />
      )}
    </div>
  );
}

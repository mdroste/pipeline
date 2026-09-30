import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PipelineConfig, ProfileSummary } from "../lib/types";
import {
  adaptiveAgentCount,
  adaptiveAgentRange,
  isAutoReview,
} from "../lib/autoReview";
import WaveDiagram from "./WaveDiagram";
import Spinner from "../ui/Spinner";

/** Live preview of the active review workflow on the New review screen:
 * what will actually run, replacing the former static explainer. */
export default function RunPlanPreview({
  refreshKey,
  inputMode,
  hasInput,
  onOpenDesigner,
}: {
  refreshKey: number;
  inputMode: string;
  hasInput: boolean;
  onOpenDesigner: () => void;
}) {
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [profileName, setProfileName] = useState("");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let stale = false;
    Promise.all([
      invoke<PipelineConfig>("get_pipeline_config"),
      invoke<ProfileSummary[]>("list_profiles"),
      invoke<string>("get_active_profile"),
    ])
      .then(([loaded, profiles, active]) => {
        if (stale) return;
        setConfig(loaded);
        setProfileName(
          profiles.find((profile) => profile.id === active)?.name ?? "",
        );
        setError(null);
      })
      .catch((caught) => {
        if (!stale) setError(String(caught));
      });
    return () => {
      stale = true;
    };
  }, [refreshKey]);
  if (error)
    return (
      <div className="flex min-h-full items-center justify-center p-8">
        <p className="max-w-sm text-center text-xs text-gray-500 dark:text-gray-400">
          The workflow plan could not be loaded. {error}
        </p>
      </div>
    );
  if (!config)
    return (
      <div className="min-h-full">
        <Spinner label="Loading workflow plan…" />
      </div>
    );
  return (
    <div className="flex min-h-full justify-center px-8 py-10">
      <div className="w-full max-w-2xl">
        <div className="mb-6 text-center">
          {profileName && (
            <h2 className="text-base font-semibold text-gray-900 dark:text-gray-100">
              {profileName}
            </h2>
          )}
          <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
            {inputMode === "none"
              ? "This workflow needs no input — start it when ready."
              : hasInput
                ? "Input selected. Start the review when ready."
                : inputMode === "folder"
                  ? "Select a folder on the left to begin."
                  : "Select a document on the left to begin."}
          </p>
        </div>
        <WaveDiagram
          steps={config.steps}
          merge={config.merge}
          adaptiveReview={isAutoReview(config)}
          adaptiveAgentCount={adaptiveAgentCount(config)}
          adaptiveAgentRange={adaptiveAgentRange(config)}
          selectedId={null}
          onSelect={onOpenDesigner}
        />
        <p className="mt-4 text-center text-xs text-gray-500 dark:text-gray-400">
          Select any stage to open it in the Review designer.
        </p>
      </div>
    </div>
  );
}

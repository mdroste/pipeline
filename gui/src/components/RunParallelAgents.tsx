import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  ModelCatalog,
  PipelineConfig,
  RunParallelOverrides,
  Settings,
} from "../lib/types";
import {
  defaultMergeAgent,
  defaultMergeEffortOverrides,
  defaultMergeModelOverrides,
  defaultParallelAgents,
  PROVIDER_LABELS,
  PROVIDERS,
  type Provider,
} from "../lib/providers";
import AgentDefaultsControl from "./AgentDefaultsControl";

interface Props {
  value: RunParallelOverrides | null;
  disabled?: boolean;
  localLlmActive?: boolean;
  refreshKey?: number;
  onChange: (value: RunParallelOverrides | null) => void;
}

export default function RunParallelAgents({
  value,
  disabled = false,
  localLlmActive = false,
  refreshKey = 0,
  onChange,
}: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [workflowMerge, setWorkflowMerge] = useState<
    PipelineConfig["merge"] | null
  >(null);
  const [catalogs, setCatalogs] = useState<Record<string, ModelCatalog>>({});
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let live = true;
    setWorkflowMerge(null);
    Promise.all([
      invoke<{ settings: Settings }>("get_settings"),
      invoke<PipelineConfig>("get_pipeline_config"),
    ])
      .then(([response, config]) => {
        if (!live) return;
        setSettings(response.settings);
        setWorkflowMerge(config.merge);
      })
      .catch(() => {
        // The run readiness error remains authoritative. Keep this optional
        // control out of the way if Settings cannot be read.
      });
    return () => {
      live = false;
    };
  }, [refreshKey]);

  const providers = useMemo<Provider[]>(() => {
    if (!settings) return ["claude", "codex", "antigravity"];
    return PROVIDERS.filter(
      (provider) =>
        provider !== "local" ||
        (localLlmActive && Boolean(settings.local_model.trim())),
    ) as Provider[];
  }, [localLlmActive, settings]);

  useEffect(() => {
    if (!open || !settings) return;
    let live = true;
    Promise.allSettled(
      providers.map(async (provider) => {
        const catalog = await invoke<ModelCatalog>("get_model_catalog", {
          provider,
          settings,
          refresh: false,
        });
        return [provider, catalog] as const;
      }),
    ).then((results) => {
      if (!live) return;
      const loaded = results.flatMap((result) =>
        result.status === "fulfilled" ? [result.value] : [],
      );
      setCatalogs(Object.fromEntries(loaded));
    });
    return () => {
      live = false;
    };
  }, [open, providers, settings]);

  if (!settings) return null;

  const inheritedAgents = defaultParallelAgents(settings);
  const agents = value?.agents ?? inheritedAgents;
  const modelOverrides =
    value?.model_overrides ?? settings.default_parallel_model_overrides ?? {};
  const effortOverrides =
    value?.effort_overrides ?? settings.default_parallel_effort_overrides ?? {};
  const workflowMergeAgent = workflowMerge?.agents?.[0];
  const inheritedMergeAgent = workflowMergeAgent || defaultMergeAgent(settings);
  const mergeAgent = value?.merge_agent || inheritedMergeAgent;
  const inheritedMergeModelOverrides = workflowMergeAgent
    ? {}
    : defaultMergeModelOverrides(settings);
  const inheritedMergeEffortOverrides = workflowMergeAgent
    ? {}
    : defaultMergeEffortOverrides(settings);
  const mergeModelOverrides = value?.merge_agent
    ? (value.merge_model_overrides ?? {})
    : inheritedMergeModelOverrides;
  const mergeEffortOverrides = value?.merge_agent
    ? (value.merge_effort_overrides ?? {})
    : inheritedMergeEffortOverrides;
  const multipleAgents = agents.length > 1;
  const mergeEnabled = workflowMerge?.enabled ?? true;
  const mergeSource = value?.merge_agent
    ? "This report"
    : workflowMergeAgent
      ? "Workflow"
      : "Settings default";

  return (
    <section className="rounded-lg border border-gray-200 bg-gray-50/60 px-3 py-2.5 dark:border-gray-800 dark:bg-gray-800/25">
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <p className="text-xs font-medium text-gray-700 dark:text-gray-300">
            Parallel agents
          </p>
          <p className="mt-0.5 truncate text-[11px] text-gray-500 dark:text-gray-500">
            {agents
              .map((agent) => PROVIDER_LABELS[agent as Provider])
              .join(" + ")}
            {!value && " · Settings default"}
          </p>
          {multipleAgents && (
            <p className="mt-0.5 truncate text-[11px] text-gray-500 dark:text-gray-500">
              {mergeEnabled
                ? `Merge · ${PROVIDER_LABELS[mergeAgent as Provider] ?? mergeAgent} · ${mergeSource}`
                : "Merge · Disabled by workflow"}
            </p>
          )}
        </div>
        {value && (
          <button
            type="button"
            disabled={disabled}
            onClick={() => onChange(null)}
            className="text-[11px] text-gray-500 hover:text-gray-800 disabled:opacity-50 dark:text-gray-400 dark:hover:text-gray-200"
          >
            Reset
          </button>
        )}
        <button
          type="button"
          aria-expanded={open}
          disabled={disabled}
          onClick={() => setOpen((current) => !current)}
          className="rounded border border-gray-300 bg-white px-2 py-1 text-[11px] font-medium text-gray-600 hover:border-gray-400 disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300"
        >
          {open ? "Done" : "Change"}
        </button>
      </div>
      {open && (
        <div className="mt-3 border-t border-gray-200 pt-3 dark:border-gray-800">
          <AgentDefaultsControl
            agents={agents}
            modelOverrides={modelOverrides}
            effortOverrides={effortOverrides}
            settings={settings}
            catalogs={catalogs}
            providers={providers}
            multi
            label="Agents for this report"
            help="Each selected provider independently runs every Parallel step in this report."
            disabled={disabled}
            onChange={(next) =>
              onChange({
                ...(value ?? {}),
                agents: next.agents,
                model_overrides: next.modelOverrides,
                effort_overrides: next.effortOverrides,
              })
            }
          />
          {multipleAgents && mergeEnabled && (
            <div className="mt-4 border-t border-gray-200 pt-4 dark:border-gray-800">
              <AgentDefaultsControl
                agents={[mergeAgent]}
                modelOverrides={mergeModelOverrides}
                effortOverrides={mergeEffortOverrides}
                settings={settings}
                catalogs={catalogs}
                providers={providers}
                label="Merge for this report"
                help="One provider combines the independent Parallel outputs before downstream steps run."
                disabled={disabled}
                onChange={(next) =>
                  onChange({
                    agents,
                    model_overrides: modelOverrides,
                    effort_overrides: effortOverrides,
                    merge_agent: next.agents[0],
                    merge_model_overrides: next.modelOverrides,
                    merge_effort_overrides: next.effortOverrides,
                  })
                }
              />
            </div>
          )}
        </div>
      )}
    </section>
  );
}

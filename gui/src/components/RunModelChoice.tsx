import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  DepsReport,
  ModelCatalog,
  PipelineConfig,
  ReviewModelChoice,
  RosterEntry,
  RunModelOverrides,
  Settings,
} from "../lib/types";
import { workflowParallelAgents } from "../lib/parallelAgents";
import {
  rosterModelSummary,
  rosterSourceLabel,
  stageModelSummary,
  withoutDefaultModel,
} from "../lib/modelChoice";
import {
  defaultMergeAgent,
  defaultMergeEffortOverrides,
  defaultMergeModelOverrides,
  PROVIDERS,
  providerTransport,
  type Provider,
} from "../lib/providers";
import AgentDefaultsControl from "./AgentDefaultsControl";

interface Props {
  value: RunModelOverrides | null;
  disabled?: boolean;
  /** What readiness last saw; undefined leaves providers unlabeled. */
  dependencies?: DepsReport | null;
  localLlmActive?: boolean;
  refreshKey?: number;
  onChange: (value: RunModelOverrides | null) => void;
}

const EMPTY: RunModelOverrides = {
  agents: [],
  model_overrides: {},
  effort_overrides: {},
};

/** The review model Settings would use, as the starting choice. */
function defaultReview(settings: Settings): ReviewModelChoice {
  const provider = settings.preferred_provider || "claude";
  return {
    provider,
    model:
      (provider === "local" ? settings.local_model : settings.default_model) ??
      "",
    effort: settings.default_effort ?? "",
  };
}

function entryName(entry: RosterEntry): string {
  if (!entry.step_labels.length) return entry.label;
  if (entry.step_labels.length <= 2) return entry.step_labels.join(", ");
  return `${entry.step_labels.length} ${entry.label.toLowerCase()}`;
}

/**
 * New review's model choice. One review model covers every call that
 * inherits the default; anything that does not follow it (a Settings stage
 * exception, a workflow-pinned step) is listed under it and can be switched
 * to the review model. The list comes from the backend, which uses the
 * executor's own inheritance rules.
 */
export default function RunModelChoice({
  value,
  disabled = false,
  dependencies,
  localLlmActive = false,
  refreshKey = 0,
  onChange,
}: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [catalogs, setCatalogs] = useState<Record<string, ModelCatalog>>({});
  const [roster, setRoster] = useState<RosterEntry[] | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let live = true;
    setConfig(null);
    Promise.all([
      invoke<{ settings: Settings }>("get_settings"),
      invoke<PipelineConfig>("get_pipeline_config"),
    ])
      .then(([response, workflow]) => {
        if (!live) return;
        setSettings(response.settings);
        setConfig(workflow);
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

  // Model lists load up front so collapsed lines can name each model.
  useEffect(() => {
    if (!settings) return;
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
      setCatalogs(
        Object.fromEntries(
          results.flatMap((result) =>
            result.status === "fulfilled" ? [result.value] : [],
          ),
        ),
      );
    });
    return () => {
      live = false;
    };
  }, [providers, settings]);

  useEffect(() => {
    if (!settings || !config) return;
    let live = true;
    invoke<RosterEntry[]>("get_model_roster", {
      runParallelOverrides: value ?? null,
    })
      .then((entries) => {
        if (live) setRoster(entries);
      })
      .catch(() => {
        if (live) setRoster(null);
      });
    return () => {
      live = false;
    };
  }, [config, settings, value, refreshKey]);

  if (!settings || !config) return null;

  const review = value?.review ?? defaultReview(settings);
  const pickerSettings = withoutDefaultModel(settings);
  const reviewKey = `${review.provider}:${providerTransport(settings, review.provider)}`;
  const reviewModels = review.model
    ? { [reviewKey]: { mode: "pinned" as const, model: review.model } }
    : {};
  const reviewEfforts = review.effort ? { [reviewKey]: review.effort } : {};
  const change = (patch: Partial<RunModelOverrides>) =>
    onChange({ ...EMPTY, ...value, review, ...patch });

  const exceptions = (roster ?? []).filter(
    (entry) =>
      entry.stage !== "fallback" &&
      (entry.source === "settings" || entry.source === "workflow"),
  );
  const fallback = roster?.find((entry) => entry.stage === "fallback");
  const followers = (roster ?? []).filter(
    (entry) => entry.source === "review" || entry.source === "default",
  );

  const inherited = workflowParallelAgents(config, settings);
  const parallelChosen = !!value?.agents.length;
  const agents = parallelChosen ? value!.agents : [review.provider];
  const modelOverrides = parallelChosen ? value!.model_overrides : reviewModels;
  const effortOverrides = parallelChosen
    ? value!.effort_overrides
    : reviewEfforts;
  const workflowMergeAgent = config.merge.agents?.[0];
  const mergeAgent =
    value?.merge_agent || workflowMergeAgent || defaultMergeAgent(settings);
  const mergeModelOverrides = value?.merge_agent
    ? (value.merge_model_overrides ?? {})
    : workflowMergeAgent
      ? {}
      : defaultMergeModelOverrides(settings);
  const mergeEffortOverrides = value?.merge_agent
    ? (value.merge_effort_overrides ?? {})
    : workflowMergeAgent
      ? {}
      : defaultMergeEffortOverrides(settings);

  return (
    <section
      aria-label="Models for this review"
      className="rounded-lg border border-gray-200 bg-gray-50/60 px-3 py-2.5 dark:border-gray-800 dark:bg-gray-800/25"
    >
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <p className="text-xs font-medium text-gray-700 dark:text-gray-300">
            Models for this review
          </p>
          <p className="mt-0.5 break-words text-[11px] leading-4 text-gray-600 dark:text-gray-400">
            {stageModelSummary(
              pickerSettings,
              review.provider,
              reviewModels,
              reviewEfforts,
              catalogs,
            )}
          </p>
          <p className="mt-0.5 text-[11px] leading-4 text-gray-500 dark:text-gray-500">
            {value ? "Chosen for this review" : "Your default model"}
            {followers.length > 0 &&
              ` · runs ${followers
                .map((entry) => entryName(entry).toLowerCase())
                .join(", ")}`}
          </p>
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

      {exceptions.length > 0 && (
        <div
          role="note"
          aria-label="Calls on other models"
          className="mt-2 rounded border border-amber-200 bg-amber-50 px-2 py-1.5 text-[11px] leading-4 text-amber-900 dark:border-amber-900 dark:bg-amber-950/35 dark:text-amber-200"
        >
          <p className="font-medium">
            {exceptions.length === 1
              ? "1 part of this review uses another model:"
              : `${exceptions.length} parts of this review use other models:`}
          </p>
          <ul className="mt-1 space-y-0.5">
            {exceptions.map((entry) => (
              <li
                key={`${entry.stage}-${entry.step_ids.join(",")}-${entry.provider}`}
              >
                {entryName(entry)} →{" "}
                {rosterModelSummary(settings, entry, catalogs)}{" "}
                <span className="text-amber-700 dark:text-amber-300">
                  ({rosterSourceLabel(entry)})
                </span>
              </li>
            ))}
          </ul>
          <button
            type="button"
            disabled={disabled}
            onClick={() =>
              change({
                clear_settings_exceptions: true,
                override_workflow_pins: true,
              })
            }
            className="mt-1.5 font-medium underline underline-offset-2 disabled:opacity-50"
          >
            Use the review model for everything
          </button>
        </div>
      )}
      {fallback && (
        <p className="mt-1.5 break-words text-[11px] leading-4 text-gray-500 dark:text-gray-500">
          Backup after a usage limit:{" "}
          {rosterModelSummary(settings, fallback, catalogs)}
        </p>
      )}

      {open && (
        <div className="mt-3 space-y-4 border-t border-gray-200 pt-3 dark:border-gray-800">
          <AgentDefaultsControl
            agents={[review.provider]}
            modelOverrides={reviewModels}
            effortOverrides={reviewEfforts}
            settings={pickerSettings}
            catalogs={catalogs}
            providers={providers}
            stacked
            dependencies={dependencies}
            label="Review model"
            help="Every part of the review uses this model unless listed above."
            disabled={disabled}
            onChange={(next) => {
              const provider = next.agents[0] ?? review.provider;
              const key = `${provider}:${providerTransport(settings, provider)}`;
              const selection =
                next.modelOverrides[key] ?? next.modelOverrides[provider];
              change({
                review: {
                  provider,
                  model: selection?.mode === "pinned" ? selection.model : "",
                  effort:
                    next.effortOverrides[key] ??
                    next.effortOverrides[provider] ??
                    "",
                },
              });
            }}
          />

          <div className="space-y-1.5 text-[11px] text-gray-600 dark:text-gray-400">
            <label className="flex items-start gap-2">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={!!value?.clear_settings_exceptions}
                disabled={disabled}
                onChange={(event) =>
                  change({ clear_settings_exceptions: event.target.checked })
                }
              />
              Use the review model for stages that Settings sets to another
              model
            </label>
            <label className="flex items-start gap-2">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={!!value?.override_workflow_pins}
                disabled={disabled}
                onChange={(event) =>
                  change({ override_workflow_pins: event.target.checked })
                }
              />
              Use the review model for steps the workflow assigns to a model
            </label>
          </div>

          {inherited.applies && (
            <div className="border-t border-gray-200 pt-3 dark:border-gray-800">
              <label className="flex items-start gap-2 text-[11px] text-gray-600 dark:text-gray-400">
                <input
                  type="checkbox"
                  className="mt-0.5"
                  checked={parallelChosen}
                  disabled={disabled}
                  onChange={(event) =>
                    change(
                      event.target.checked
                        ? {
                            agents: [review.provider],
                            model_overrides: reviewModels,
                            effort_overrides: reviewEfforts,
                          }
                        : {
                            agents: [],
                            model_overrides: {},
                            effort_overrides: {},
                            merge_agent: undefined,
                            merge_model_overrides: undefined,
                            merge_effort_overrides: undefined,
                          },
                    )
                  }
                />
                Choose the models for parallel steps, such as a second opinion
              </label>
              {parallelChosen && (
                <div className="mt-3">
                  <AgentDefaultsControl
                    agents={agents}
                    modelOverrides={modelOverrides}
                    effortOverrides={effortOverrides}
                    settings={pickerSettings}
                    catalogs={catalogs}
                    providers={providers}
                    multi
                    stacked
                    dependencies={dependencies}
                    label="Parallel steps"
                    help="Each selected model runs its own version of every parallel step."
                    disabled={disabled}
                    onChange={(next) =>
                      change({
                        agents: next.agents,
                        model_overrides: next.modelOverrides,
                        effort_overrides: next.effortOverrides,
                      })
                    }
                  />
                  {agents.length > 1 && config.merge.enabled && (
                    <div className="mt-4">
                      <AgentDefaultsControl
                        agents={[mergeAgent]}
                        modelOverrides={mergeModelOverrides}
                        effortOverrides={mergeEffortOverrides}
                        settings={pickerSettings}
                        catalogs={catalogs}
                        providers={providers}
                        stacked
                        dependencies={dependencies}
                        label="Combine results"
                        help="One model combines the parallel results before the next steps begin."
                        disabled={disabled}
                        onChange={(next) =>
                          change({
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
            </div>
          )}

          {roster && roster.length > 0 && (
            <div className="border-t border-gray-200 pt-3 dark:border-gray-800">
              <p className="text-xs font-medium text-gray-700 dark:text-gray-300">
                Every model this review calls
              </p>
              <ul
                aria-label="Every model this review calls"
                className="mt-1.5 space-y-1.5"
              >
                {roster.map((entry) => (
                  <li
                    key={`${entry.stage}-${entry.step_ids.join(",")}-${entry.provider}`}
                    className="text-[11px] leading-4"
                  >
                    <span className="font-medium text-gray-700 dark:text-gray-300">
                      {entryName(entry)}
                    </span>
                    <span className="block break-words text-gray-600 dark:text-gray-400">
                      {rosterModelSummary(settings, entry, catalogs)}
                    </span>
                    <span className="block text-gray-500 dark:text-gray-500">
                      {rosterSourceLabel(entry)}
                      {entry.note ? ` · ${entry.note}` : ""}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </section>
  );
}

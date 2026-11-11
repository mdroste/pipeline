import { useEffect, useId, useMemo, useState } from "react";
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
  stageModelDetails,
  withoutDefaultModel,
} from "../lib/modelChoice";
import {
  defaultMergeAgent,
  defaultMergeEffortOverrides,
  defaultMergeModelOverrides,
  PROVIDERS,
  PROVIDER_LABELS,
  providerTransport,
  type Provider,
} from "../lib/providers";
import { Icon } from "../ui/icons";
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
  const editorId = useId();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [catalogs, setCatalogs] = useState<Record<string, ModelCatalog>>({});
  const [catalogErrors, setCatalogErrors] = useState<string[]>([]);
  const [roster, setRoster] = useState<RosterEntry[] | null>(null);
  const [open, setOpen] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [rosterError, setRosterError] = useState<string | null>(null);
  const [rosterLoading, setRosterLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    setConfig(null);
    setLoadError(null);
    Promise.all([
      invoke<{ settings: Settings }>("get_settings"),
      invoke<PipelineConfig>("get_pipeline_config"),
    ])
      .then(([response, workflow]) => {
        if (!live) return;
        setSettings(response.settings);
        setConfig(workflow);
      })
      .catch((error) => {
        if (live) setLoadError(String(error));
      });
    return () => {
      live = false;
    };
  }, [refreshKey, attempt]);

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
    setCatalogs({});
    setCatalogErrors([]);
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
      setCatalogErrors(
        results.flatMap((result, index) =>
          result.status === "rejected"
            ? [PROVIDER_LABELS[providers[index]]]
            : [],
        ),
      );
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
  }, [providers, settings, attempt]);

  useEffect(() => {
    if (!settings || !config) return;
    let live = true;
    setRosterLoading(true);
    setRosterError(null);
    setRoster(null);
    invoke<RosterEntry[]>("get_model_roster", {
      runParallelOverrides: value ?? null,
    })
      .then((entries) => {
        if (live) {
          setRoster(entries);
          setRosterLoading(false);
        }
      })
      .catch((error) => {
        if (live) {
          setRosterError(String(error));
          setRosterLoading(false);
        }
      });
    return () => {
      live = false;
    };
  }, [config, settings, value, refreshKey, attempt]);

  if (!settings || !config)
    return (
      <section
        aria-label="Models for this review"
        className="rounded-ui-md border border-line p-4 text-ui-label"
      >
        <h2 className="font-medium">Models for this review</h2>
        {loadError ? (
          <div role="alert">
            <p>Couldn't load model choices. {loadError}</p>
            <button
              className="mt-2 underline"
              onClick={() => setAttempt((n) => n + 1)}
            >
              Retry model choices
            </button>
          </div>
        ) : (
          <p role="status">Loading model choices…</p>
        )}
      </section>
    );

  const hasModelChoice = !!(
    value?.review ||
    value?.agents.length ||
    value?.merge_agent
  );
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

  const summary = stageModelDetails(
    pickerSettings,
    review.provider,
    reviewModels,
    reviewEfforts,
    catalogs,
  );

  return (
    <section
      aria-label="Models for this review"
      className="overflow-hidden rounded-ui-md border border-line bg-surface"
    >
      <div className="flex items-center justify-between gap-2 px-3 pt-3">
        <h2 className="text-ui-label font-medium text-ink">
          Models for this review
        </h2>
        {hasModelChoice && (
          <button
            type="button"
            disabled={disabled}
            onClick={() =>
              onChange(
                value?.extraction_method
                  ? { ...EMPTY, extraction_method: value.extraction_method }
                  : null,
              )
            }
            className="rounded-ui-sm px-1.5 py-1 text-ui-meta text-ink-muted transition-colors hover:bg-sunken hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50"
          >
            Reset
          </button>
        )}
      </div>
      <button
        type="button"
        aria-label={open ? "Done" : "Change"}
        aria-expanded={open}
        aria-controls={editorId}
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        className="group block w-full px-3 py-3 text-left transition-colors hover:bg-sunken/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent disabled:opacity-50"
      >
        <span className="mb-2 flex flex-wrap items-center justify-between gap-1.5">
          <span className="rounded-ui-sm bg-sunken px-2 py-0.5 text-ui-meta font-medium text-ink">
            {summary.provider}
          </span>
          <span className="text-ui-meta text-ink-muted">
            {hasModelChoice ? "This review" : "Your default model"}
          </span>
        </span>
        <span className="block break-words text-ui-body font-medium leading-5 text-ink">
          {summary.model}
        </span>
        <span className="mt-1 block text-ui-meta leading-5 text-ink-muted">
          {review.effort ? `${summary.effort} thinking` : summary.effort}
        </span>
        <span className="mt-3 flex items-center justify-between gap-2 text-ui-meta font-medium text-ink-muted group-hover:text-ink">
          {open ? "Done" : "Change model"}
          <Icon
            name="chevron-down"
            className={`h-3.5 w-3.5 transition-transform motion-reduce:transition-none ${open ? "rotate-180" : ""}`}
          />
        </span>
      </button>

      {rosterLoading && (
        <p role="status" className="px-3 pb-3 text-ui-meta text-ink-muted">
          Checking models for each step…
        </p>
      )}
      {catalogErrors.length > 0 && (
        <div
          role="alert"
          className="mx-3 mb-3 rounded-ui-sm bg-sunken p-3 text-ui-meta"
        >
          <p>
            Couldn't load model lists for {catalogErrors.join(", ")}. Your saved
            model choices are still selected.
          </p>
          <button
            className="mt-2 underline"
            onClick={() => setAttempt((n) => n + 1)}
          >
            Retry model lists
          </button>
        </div>
      )}
      {rosterError && (
        <div
          role="alert"
          className="mx-3 mb-3 rounded-ui-sm bg-sunken p-3 text-ui-meta"
        >
          <p>Couldn't check models for each step. {rosterError}</p>
          <button
            className="mt-2 underline"
            onClick={() => setAttempt((n) => n + 1)}
          >
            Retry model check
          </button>
        </div>
      )}
      {exceptions.length > 0 && (
        <div
          role="note"
          aria-label="Calls on other models"
          className="mx-3 mb-3 rounded-ui-sm bg-sunken p-2.5 text-ui-meta leading-5 text-ink"
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
                <span className="text-ink-muted">
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
            className="mt-2 rounded-ui-sm text-left font-medium underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50"
          >
            Use the review model for everything
          </button>
        </div>
      )}

      {open && (
        <div id={editorId} className="space-y-4 border-t border-line px-3 py-4">
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
            help="Used throughout the review, except where a different model is assigned."
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

          <details className="group/advanced border-t border-line pt-3">
            <summary className="flex cursor-pointer list-none items-center justify-between gap-2 rounded-ui-sm text-ui-label font-medium text-ink-muted hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent [&::-webkit-details-marker]:hidden">
              Advanced options
              <Icon
                name="chevron-down"
                className="h-3.5 w-3.5 shrink-0 transition-transform group-open/advanced:rotate-180 motion-reduce:transition-none"
              />
            </summary>
            <div className="mt-4 space-y-4">
              <div className="space-y-3 text-ui-meta leading-5 text-ink-muted">
                <label className="flex items-start gap-2">
                  <input
                    type="checkbox"
                    className="mt-1 shrink-0 accent-inverse"
                    checked={!!value?.clear_settings_exceptions}
                    disabled={disabled}
                    onChange={(event) =>
                      change({
                        clear_settings_exceptions: event.target.checked,
                      })
                    }
                  />
                  Use the review model for stages that Settings sets to another
                  model
                </label>
                <label className="flex items-start gap-2">
                  <input
                    type="checkbox"
                    className="mt-1 shrink-0 accent-inverse"
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
                <div className="border-t border-line pt-3">
                  <label className="flex items-start gap-2 text-ui-meta leading-5 text-ink-muted">
                    <input
                      type="checkbox"
                      className="mt-1 shrink-0 accent-inverse"
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
                    Choose the models for parallel steps, such as a second
                    opinion
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
                <div className="border-t border-line pt-3">
                  <p className="text-ui-label font-medium text-ink">
                    Every model this review calls
                  </p>
                  <ul
                    aria-label="Every model this review calls"
                    className="mt-1.5 space-y-1.5"
                  >
                    {roster.map((entry) => (
                      <li
                        key={`${entry.stage}-${entry.step_ids.join(",")}-${entry.provider}`}
                        className="text-ui-meta leading-5"
                      >
                        <span className="font-medium text-ink">
                          {entryName(entry)}
                        </span>
                        <span className="block break-words text-ink-muted">
                          {rosterModelSummary(settings, entry, catalogs)}
                        </span>
                        <span className="block text-ink-muted">
                          {rosterSourceLabel(entry)}
                          {entry.note ? ` · ${entry.note}` : ""}
                        </span>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          </details>
        </div>
      )}
      {fallback && (
        <div className="border-t border-line bg-sunken/40 px-3 py-2.5">
          <p className="text-ui-meta font-medium text-ink">
            Usage-limit backup
          </p>
          <p className="mt-1 break-words text-ui-meta leading-5 text-ink-muted">
            {rosterModelSummary(settings, fallback, catalogs)}
          </p>
        </div>
      )}
    </section>
  );
}

import { providerAvailability } from "../../lib/connections";
import type { DepsReport, ModelCatalog, Settings } from "../../lib/types";
import {
  defaultMergeAgent,
  defaultMergeEffortOverrides,
  defaultMergeModelOverrides,
  defaultModelSummary,
  defaultOrientationAgent,
  defaultParallelAgents,
  defaultSequentialAgent,
  PROVIDERS,
} from "../../lib/providers";
import { stageModelSummary } from "../../lib/modelChoice";

import ModelStage from "./ModelStage";
import AgentDefaultsControl from "../AgentDefaultsControl";
import DecimalField from "../DecimalField";

import { selectClass, inputClass, SectionHeader, Field } from "./controls";
import { SettingRow } from "./SaveState";
export function ModelDefaultsSection({
  settings,
  setSettings,
  catalogs,
  onChangeDefault,
  dependencies,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
  catalogs: Record<string, ModelCatalog>;
  onChangeDefault: () => void;
  dependencies?: DepsReport | null;
}) {
  const parallelAgents = defaultParallelAgents(settings);
  const mergeAgent = defaultMergeAgent(settings);
  const sequentialAgent = defaultSequentialAgent(settings);
  const orientationAgent = defaultOrientationAgent(settings);
  const usageLimitFallbackAgent = settings.usage_limit_fallback_agent || "";
  // A backup that isn't connected couldn't help, so offer a connected one first.
  const backupOptions = PROVIDERS.filter(
    (provider) => provider !== (settings.preferred_provider || "claude"),
  );
  const backupStatus = (status: "ready" | "unknown") =>
    backupOptions.find(
      (provider) =>
        providerAvailability(provider, settings, dependencies) === status,
    );
  const backupChoice = backupStatus("ready") ?? backupStatus("unknown");
  const defaultLabel = "Default review model";
  // Stages with their own model, which New review lists as exceptions.
  const exceptionCount = [
    settings.default_orientation_agent ||
      Object.keys(settings.default_orientation_model_overrides ?? {}).length ||
      Object.keys(settings.default_orientation_effort_overrides ?? {}).length,
    settings.default_merge_agent ||
      Object.keys(settings.default_merge_model_overrides ?? {}).length ||
      Object.keys(settings.default_merge_effort_overrides ?? {}).length,
    settings.default_sequential_agent ||
      Object.keys(settings.default_sequential_model_overrides ?? {}).length ||
      Object.keys(settings.default_sequential_effort_overrides ?? {}).length,
  ].filter(Boolean).length;
  // An inheriting stage names the model it inherits, not just where from.
  const defaultSummary = defaultModelSummary(settings, catalogs);
  const inheritsDefault = `${defaultLabel}: ${defaultSummary}`;
  const sequentialSummary = settings.default_sequential_agent
    ? stageModelSummary(
        settings,
        sequentialAgent,
        settings.default_sequential_model_overrides,
        settings.default_sequential_effort_overrides,
        catalogs,
      )
    : defaultSummary;

  return (
    <>
      <SectionHeader
        title="Model defaults"
        description="Every review runs on your default review model. A stage below, or a step in the review designer, can use another model. New review lists any that do, and can change the model for one review."
      />
      <SettingRow label={defaultLabel} description={defaultSummary}>
        <button
          type="button"
          className="settings-button"
          aria-label="Change default model"
          onClick={onChangeDefault}
        >
          Change
        </button>
      </SettingRow>

      <div className="space-y-0">
        <section className="space-y-5">
          <div className="divide-y divide-gray-200 dark:divide-neutral-800">
            <div>
              <ModelStage
                title="Parallel steps"
                inherited={
                  !settings.default_parallel_agents?.length
                    ? inheritsDefault
                    : undefined
                }
                agents={parallelAgents}
                models={settings.default_parallel_model_overrides}
                efforts={settings.default_parallel_effort_overrides}
                settings={settings}
                catalogs={catalogs}
              >
                <AgentDefaultsControl
                  agents={parallelAgents}
                  modelOverrides={settings.default_parallel_model_overrides}
                  effortOverrides={settings.default_parallel_effort_overrides}
                  settings={settings}
                  catalogs={catalogs}
                  providers={PROVIDERS}
                  dependencies={dependencies}
                  multi
                  inherit={{
                    active: !settings.default_parallel_agents?.length,
                    label: defaultLabel,
                    onSelect: () =>
                      setSettings({
                        ...settings,
                        default_parallel_agents: [],
                        default_parallel_model_overrides: {},
                        default_parallel_effort_overrides: {},
                      }),
                  }}
                  label="Parallel steps"
                  help="Steps that run at the same time use the default review model. Add another model to run a second opinion of each step."
                  onChange={(next) =>
                    setSettings({
                      ...settings,
                      default_parallel_agents: next.agents,
                      default_parallel_model_overrides: next.modelOverrides,
                      default_parallel_effort_overrides: next.effortOverrides,
                    })
                  }
                />
              </ModelStage>
            </div>
            <details
              className="settings-stage-exceptions py-3"
              open={exceptionCount > 0}
            >
              <summary className="cursor-pointer text-sm text-gray-800 dark:text-neutral-200">
                <span className="font-medium">Stage exceptions</span>
                <span className="ml-2 text-xs text-gray-500 dark:text-neutral-400">
                  {exceptionCount === 0
                    ? "None: every stage uses the default review model"
                    : `${exceptionCount} set`}
                </span>
              </summary>
              <p className="mt-1 text-xs leading-5 text-gray-500 dark:text-neutral-400">
                Give a stage its own model only when it needs one, such as a
                faster model for input assessment. New review lists every stage
                set here and can switch it to the review model for one review.
              </p>
              <div>
                <ModelStage
                  title="Input assessment"
                  inherited={
                    !settings.default_orientation_agent
                      ? inheritsDefault
                      : undefined
                  }
                  agents={[orientationAgent]}
                  models={settings.default_orientation_model_overrides}
                  efforts={settings.default_orientation_effort_overrides}
                  settings={settings}
                  catalogs={catalogs}
                >
                  <AgentDefaultsControl
                    agents={[orientationAgent]}
                    modelOverrides={
                      settings.default_orientation_model_overrides
                    }
                    effortOverrides={
                      settings.default_orientation_effort_overrides
                    }
                    settings={settings}
                    catalogs={catalogs}
                    providers={PROVIDERS}
                    dependencies={dependencies}
                    inherit={{
                      active: !settings.default_orientation_agent,
                      label: defaultLabel,
                      onSelect: () =>
                        setSettings({
                          ...settings,
                          default_orientation_agent: "",
                          default_orientation_model_overrides: {},
                          default_orientation_effort_overrides: {},
                        }),
                    }}
                    label="Input assessment"
                    help="Reads the inputs and selects the specialist reviewers."
                    onChange={(next) =>
                      setSettings({
                        ...settings,
                        default_orientation_agent: next.agents[0],
                        default_orientation_model_overrides:
                          next.modelOverrides,
                        default_orientation_effort_overrides:
                          next.effortOverrides,
                      })
                    }
                  />
                </ModelStage>
              </div>
              <div>
                <ModelStage
                  title="Combine results"
                  inherited={
                    !settings.default_merge_agent
                      ? `Same as sequential steps: ${sequentialSummary}`
                      : undefined
                  }
                  agents={[mergeAgent]}
                  models={defaultMergeModelOverrides(settings)}
                  efforts={defaultMergeEffortOverrides(settings)}
                  settings={settings}
                  catalogs={catalogs}
                >
                  <AgentDefaultsControl
                    agents={[mergeAgent]}
                    modelOverrides={defaultMergeModelOverrides(settings)}
                    effortOverrides={defaultMergeEffortOverrides(settings)}
                    settings={settings}
                    catalogs={catalogs}
                    providers={PROVIDERS}
                    dependencies={dependencies}
                    inherit={{
                      active: !settings.default_merge_agent,
                      label: "Same as sequential steps",
                      onSelect: () =>
                        setSettings({
                          ...settings,
                          default_merge_agent: "",
                          default_merge_model_overrides: {},
                          default_merge_effort_overrides: {},
                        }),
                    }}
                    label="Combine results"
                    help="Choose one model to combine results when a parallel step uses more than one provider."
                    onChange={(next) =>
                      setSettings({
                        ...settings,
                        default_merge_agent: next.agents[0],
                        default_merge_model_overrides: next.modelOverrides,
                        default_merge_effort_overrides: next.effortOverrides,
                      })
                    }
                  />
                </ModelStage>
              </div>
              <div>
                <ModelStage
                  title="Sequential steps"
                  inherited={
                    !settings.default_sequential_agent
                      ? inheritsDefault
                      : undefined
                  }
                  agents={[sequentialAgent]}
                  models={settings.default_sequential_model_overrides}
                  efforts={settings.default_sequential_effort_overrides}
                  settings={settings}
                  catalogs={catalogs}
                >
                  <AgentDefaultsControl
                    agents={[sequentialAgent]}
                    modelOverrides={settings.default_sequential_model_overrides}
                    effortOverrides={
                      settings.default_sequential_effort_overrides
                    }
                    settings={settings}
                    catalogs={catalogs}
                    providers={PROVIDERS}
                    dependencies={dependencies}
                    inherit={{
                      active: !settings.default_sequential_agent,
                      label: defaultLabel,
                      onSelect: () =>
                        setSettings({
                          ...settings,
                          default_sequential_agent: "",
                          default_sequential_model_overrides: {},
                          default_sequential_effort_overrides: {},
                        }),
                    }}
                    label="Sequential steps"
                    help="Choose one model for steps that run one at a time."
                    onChange={(next) =>
                      setSettings({
                        ...settings,
                        default_sequential_agent: next.agents[0],
                        default_sequential_model_overrides: next.modelOverrides,
                        default_sequential_effort_overrides:
                          next.effortOverrides,
                      })
                    }
                  />
                </ModelStage>
              </div>
            </details>
            <div
              id="usage-limit-fallback"
              tabIndex={-1}
              className="settings-anchor py-3"
            >
              <label className="flex items-start gap-2.5 text-sm text-gray-800 dark:text-neutral-200">
                <input
                  aria-label="Enable usage-limit fallback"
                  type="checkbox"
                  checked={Boolean(usageLimitFallbackAgent)}
                  disabled={!usageLimitFallbackAgent && !backupChoice}
                  onChange={(event) =>
                    setSettings({
                      ...settings,
                      usage_limit_fallback_agent: event.target.checked
                        ? (backupChoice ?? "")
                        : "",
                    })
                  }
                  className="mt-0.5 rounded border-gray-300 accent-gray-900 dark:border-neutral-700 dark:accent-neutral-200"
                />
                <span>
                  <span className="font-medium">
                    Continue after an account usage limit
                  </span>
                  <span className="mt-0.5 block text-xs leading-5 text-gray-500 dark:text-neutral-400">
                    If you reach a usage or credit limit, Pipeline retries the
                    call with your backup provider. Temporary rate limits use
                    Step retries instead.
                    {!usageLimitFallbackAgent &&
                      !backupChoice &&
                      " Connect a second provider to use a backup."}
                  </span>
                </span>
              </label>
              {usageLimitFallbackAgent && (
                <div className="mt-4 pl-6">
                  <AgentDefaultsControl
                    agents={[usageLimitFallbackAgent]}
                    modelOverrides={
                      settings.usage_limit_fallback_model_overrides ?? {}
                    }
                    effortOverrides={
                      settings.usage_limit_fallback_effort_overrides ?? {}
                    }
                    settings={settings}
                    catalogs={catalogs}
                    providers={PROVIDERS}
                    dependencies={dependencies}
                    label="Backup provider"
                    help="Pipeline tries this provider once for the call that reached the limit."
                    onChange={(next) =>
                      setSettings({
                        ...settings,
                        usage_limit_fallback_agent: next.agents[0],
                        usage_limit_fallback_model_overrides:
                          next.modelOverrides,
                        usage_limit_fallback_effort_overrides:
                          next.effortOverrides,
                      })
                    }
                  />
                </div>
              )}
            </div>
          </div>
        </section>
      </div>
    </>
  );
}

function LimitInput({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
}) {
  return (
    <label className="flex flex-col gap-1 text-xs text-gray-600 dark:text-neutral-400">
      {label}
      <DecimalField
        aria-label={`Review limit: ${label}`}
        min={0}
        value={value}
        onChange={(next) => onChange(next > 0 ? next : 0)}
        className={inputClass}
      />
    </label>
  );
}

export function WorkflowSection({
  settings,
  setSettings,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
}) {
  return (
    <>
      <SectionHeader
        title="Execution"
        description="Set how many model calls can run at once, how long they can take, and how often to retry."
      />
      <div>
        <SettingRow
          label="Model calls at once"
          description="How many model calls a review can run at the same time."
        >
          <div className="flex items-center gap-3">
            <input
              aria-label="Model calls at once"
              type="range"
              min={1}
              max={20}
              value={settings.max_workers}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  max_workers: parseInt(e.target.value, 10),
                })
              }
              className="flex-1 accent-gray-900 dark:accent-gray-300"
            />
            <span className="w-6 text-center font-mono text-sm text-gray-700 dark:text-neutral-300">
              {settings.max_workers}
            </span>
          </div>
        </SettingRow>

        <SettingRow
          label="Step time limit"
          description="Maximum time for each model call. Input assessment and PDF extraction calls get half this time."
        >
          <select
            aria-label="Step time limit"
            value={settings.step_timeout_secs}
            onChange={(e) =>
              setSettings({
                ...settings,
                step_timeout_secs: parseInt(e.target.value, 10),
              })
            }
            className={selectClass}
          >
            <option value={600}>10 minutes</option>
            <option value={1200}>20 minutes (default)</option>
            <option value={1800}>30 minutes</option>
            <option value={2700}>45 minutes</option>
            <option value={3600}>60 minutes</option>
          </select>
        </SettingRow>

        <SettingRow
          label="Step retries"
          description="How many times to retry a failed step. Enter 0 for no retries."
        >
          <div className="flex items-center gap-3">
            <input
              aria-label="Step retries"
              type="range"
              min={0}
              max={5}
              value={settings.max_retries}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  max_retries: parseInt(e.target.value, 10),
                })
              }
              className="flex-1 accent-gray-900 dark:accent-gray-300"
            />
            <span className="w-6 text-center font-mono text-sm text-gray-700 dark:text-neutral-300">
              {settings.max_retries}
            </span>
          </div>
        </SettingRow>

        <SettingRow
          label="Reviews at once"
          description="How many reviews run at the same time. A higher number finishes the queue sooner but reaches provider limits faster."
        >
          <select
            id="max-concurrent-reviews"
            aria-label="Reviews at once"
            value={settings.max_concurrent_reviews ?? 2}
            onChange={(e) =>
              setSettings({
                ...settings,
                max_concurrent_reviews: parseInt(e.target.value, 10),
              })
            }
            className={selectClass}
          >
            <option value={1}>1 (one at a time)</option>
            <option value={2}>2 (default)</option>
            <option value={3}>3</option>
            <option value={4}>4</option>
          </select>
        </SettingRow>

        <div className="pt-3">
          <Field
            label="Review limits"
            help="When a review reaches a limit, Pipeline starts no new steps. Running steps finish, and the results so far are saved as a partial report. Cost counts API use at list prices. Subscriptions use only the model call and time limits. Enter 0 for no limit."
          >
            <div id="review-limits" className="grid gap-2 sm:grid-cols-3">
              <LimitInput
                label="Cost (USD)"
                value={settings.review_max_cost_usd ?? 0}
                onChange={(value) =>
                  setSettings({ ...settings, review_max_cost_usd: value })
                }
              />
              <LimitInput
                label="Model calls"
                value={settings.review_max_calls ?? 0}
                onChange={(value) =>
                  setSettings({
                    ...settings,
                    review_max_calls: Math.round(value),
                  })
                }
              />
              <LimitInput
                label="Minutes"
                value={settings.review_max_minutes ?? 0}
                onChange={(value) =>
                  setSettings({
                    ...settings,
                    review_max_minutes: Math.round(value),
                  })
                }
              />
            </div>
          </Field>
        </div>
      </div>
    </>
  );
}

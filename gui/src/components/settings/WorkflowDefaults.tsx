import type { ModelCatalog, Settings } from "../../lib/types";
import {
  defaultMergeAgent,
  defaultMergeEffortOverrides,
  defaultMergeModelOverrides,
  defaultOrientationAgent,
  defaultParallelAgents,
  defaultSequentialAgent,
  PROVIDERS,
} from "../../lib/providers";

import AgentDefaultsControl from "../AgentDefaultsControl";

import { selectClass, SectionHeader, Field } from "./controls";
export function ModelDefaultsSection({
  settings,
  setSettings,
  catalogs,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
  catalogs: Record<string, ModelCatalog>;
}) {
  const parallelAgents = defaultParallelAgents(settings);
  const mergeAgent = defaultMergeAgent(settings);
  const sequentialAgent = defaultSequentialAgent(settings);
  const orientationAgent = defaultOrientationAgent(settings);
  const usageLimitFallbackAgent = settings.usage_limit_fallback_agent || "";

  return (
    <>
      <SectionHeader
        title="Model defaults"
        description="Choose who handles each stage. Steps set to Default use these providers, models, and thinking levels."
      />

      <div className="space-y-0">
        <section className="space-y-5">
          <div className="divide-y divide-gray-200 dark:divide-neutral-800">
            <div className="pb-5">
              <AgentDefaultsControl
                agents={[orientationAgent]}
                modelOverrides={settings.default_orientation_model_overrides}
                effortOverrides={settings.default_orientation_effort_overrides}
                settings={settings}
                catalogs={catalogs}
                providers={PROVIDERS}
                label="Orientation map"
                help="Select one default model to use for processing inputs and classifying adaptive workflow steps."
                onChange={(next) =>
                  setSettings({
                    ...settings,
                    default_orientation_agent: next.agents[0],
                    default_orientation_model_overrides: next.modelOverrides,
                    default_orientation_effort_overrides: next.effortOverrides,
                  })
                }
              />
            </div>
            <div className="py-5">
              <AgentDefaultsControl
                agents={parallelAgents}
                modelOverrides={settings.default_parallel_model_overrides}
                effortOverrides={settings.default_parallel_effort_overrides}
                settings={settings}
                catalogs={catalogs}
                providers={PROVIDERS}
                multi
                label="Parallel steps"
                help={
                  <>
                    Select one{" "}
                    <strong className="font-semibold">or more</strong> default
                    model to use for parallel workflow steps.
                  </>
                }
                onChange={(next) =>
                  setSettings({
                    ...settings,
                    default_parallel_agents: next.agents,
                    default_parallel_model_overrides: next.modelOverrides,
                    default_parallel_effort_overrides: next.effortOverrides,
                  })
                }
              />
            </div>
            <div className="py-5">
              <AgentDefaultsControl
                agents={[mergeAgent]}
                modelOverrides={defaultMergeModelOverrides(settings)}
                effortOverrides={defaultMergeEffortOverrides(settings)}
                settings={settings}
                catalogs={catalogs}
                providers={PROVIDERS}
                label="Merge"
                help="Select one default model to combine outputs when a Parallel step runs with multiple providers."
                onChange={(next) =>
                  setSettings({
                    ...settings,
                    default_merge_agent: next.agents[0],
                    default_merge_model_overrides: next.modelOverrides,
                    default_merge_effort_overrides: next.effortOverrides,
                  })
                }
              />
            </div>
            <div className="py-5">
              <AgentDefaultsControl
                agents={[sequentialAgent]}
                modelOverrides={settings.default_sequential_model_overrides}
                effortOverrides={settings.default_sequential_effort_overrides}
                settings={settings}
                catalogs={catalogs}
                providers={PROVIDERS}
                label="Sequential steps"
                help={
                  <>
                    Select <strong className="font-semibold">one</strong>{" "}
                    default model to use for sequential workflow steps.
                  </>
                }
                onChange={(next) =>
                  setSettings({
                    ...settings,
                    default_sequential_agent: next.agents[0],
                    default_sequential_model_overrides: next.modelOverrides,
                    default_sequential_effort_overrides: next.effortOverrides,
                  })
                }
              />
            </div>
            <div
              id="usage-limit-fallback"
              tabIndex={-1}
              className="settings-anchor py-5"
            >
              <label className="flex items-start gap-2.5 text-sm text-gray-800 dark:text-neutral-200">
                <input
                  aria-label="Enable usage-limit fallback"
                  type="checkbox"
                  checked={Boolean(usageLimitFallbackAgent)}
                  onChange={(event) => {
                    const fallback =
                      PROVIDERS.find(
                        (provider) =>
                          provider !==
                          (settings.preferred_provider || "claude"),
                      ) || "codex";
                    setSettings({
                      ...settings,
                      usage_limit_fallback_agent: event.target.checked
                        ? fallback
                        : "",
                    });
                  }}
                  className="mt-0.5 rounded border-gray-300 accent-gray-900 dark:border-neutral-700 dark:accent-neutral-200"
                />
                <span>
                  <span className="font-medium">
                    Continue after an account usage limit
                  </span>
                  <span className="mt-0.5 block text-xs leading-5 text-gray-500 dark:text-neutral-400">
                    If a subscription window, API quota, or credit balance is
                    exhausted, try one configured fallback provider. Temporary
                    throttling keeps its normal retry policy.
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
                    label="Fallback agent"
                    help="Used once for the affected call, including orientation, workflow, and merge calls."
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
            <div className="pt-5">
              <Field
                label="Preferred Provider"
                help="Used when a workflow has no assigned provider, and as the fallback for PDF extraction and revision comparison."
              >
                <select
                  aria-label="Preferred Provider"
                  value={settings.preferred_provider}
                  onChange={(event) =>
                    setSettings({
                      ...settings,
                      preferred_provider: event.target.value,
                    })
                  }
                  className={selectClass}
                >
                  <option value="claude">Claude (Anthropic)</option>
                  <option value="codex">ChatGPT (OpenAI)</option>
                  <option value="antigravity">Gemini (Google)</option>
                  <option value="local">
                    Local (Ollama / OpenAI-compatible)
                  </option>
                </select>
              </Field>
            </div>
          </div>
        </section>
      </div>
    </>
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
        description="Control concurrency, time limits, and retry behavior across model calls."
      />
      <div className="space-y-5">
        <Field label="Maximum Concurrent Agents">
          <div className="flex items-center gap-3">
            <input
              aria-label="Maximum Concurrent Agents"
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
        </Field>

        <Field
          label="Step Timeout"
          help="Maximum time for each LLM call. Orientation and extraction calls use half this value."
        >
          <select
            aria-label="Step Timeout"
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
        </Field>

        <Field
          label="Step Retries"
          help="Number of times to retry a failed step before giving up. Set to 0 for no retries."
        >
          <div className="flex items-center gap-3">
            <input
              aria-label="Step Retries"
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
        </Field>
      </div>
    </>
  );
}

import { reviewConnectionLabel } from "../../lib/connections";
import { memo, useState } from "react";
import {
  modelDisplayName,
  unsetEffortLabel,
  unsetModelOption,
} from "../../lib/modelChoice";
import type { ModelCatalog, Settings, StepConfig } from "../../lib/types";
import {
  decodeModelSelection,
  defaultParallelAgents,
  defaultSequentialAgent,
  effortOptions,
  encodeModelSelection,
  providerTransport,
} from "../../lib/providers";

function ModelOverrides({
  step,
  settings,
  catalogs,
  onChange,
}: {
  step: StepConfig;
  settings: Settings | null;
  catalogs: Record<string, ModelCatalog>;
  onChange: (patch: Partial<StepConfig>) => void;
}) {
  const hasOverride = !!(
    step.model ||
    step.effort ||
    Object.keys(step.model_overrides ?? {}).length ||
    Object.keys(step.effort_overrides ?? {}).length
  );
  const [open, setOpen] = useState(hasOverride);
  // A step without its own agents runs on the Settings default for its phase.
  const defaultAgents = !settings
    ? ["claude"]
    : step.phase === "parallel"
      ? defaultParallelAgents(settings)
      : [defaultSequentialAgent(settings)];
  const providers = Array.from(
    new Set(
      (step.agents?.length ? step.agents : defaultAgents).map(
        (provider) => provider || "claude",
      ),
    ),
  );

  const transportFor = (provider: string): "cli" | "api" =>
    settings ? providerTransport(settings, provider) : "api";

  // Mirrors apply_agent_defaults: only a step that leaves its agents empty
  // takes the Settings stage's model and thinking choices.
  const stage =
    step.phase === "parallel" ? "Parallel review" : "Sequential steps";
  const stageModels = !settings
    ? undefined
    : step.phase === "parallel"
      ? settings.default_parallel_model_overrides
      : settings.default_sequential_model_overrides;
  const stageEfforts = !settings
    ? undefined
    : step.phase === "parallel"
      ? settings.default_parallel_effort_overrides
      : settings.default_sequential_effort_overrides;
  const usesStage = !step.agents?.length;

  const inheritedModel = (
    provider: string,
    key: string,
    explicitAutomatic: boolean,
  ): string => {
    if (!settings) return "Not set";
    const catalog = catalogs[provider];
    // A saved Automatic choice skips the Settings stage and step model.
    if (explicitAutomatic) return unsetModelOption(settings, provider, catalog);
    if (step.model)
      return `This step: ${modelDisplayName(catalog, step.model)}`;
    const staged = usesStage
      ? (stageModels?.[key] ?? stageModels?.[provider])
      : undefined;
    if (staged?.mode === "pinned")
      return `Settings → ${stage}: ${modelDisplayName(catalog, staged.model)}`;
    if (staged?.mode === "role")
      return `Settings → ${stage}: ${catalog?.roles.find((role) => role.id === staged.role)?.label ?? staged.role}`;
    return unsetModelOption(settings, provider, catalog);
  };

  const inheritedEffort = (provider: string, key: string): string => {
    if (!settings) return "Not set";
    const capitalized = (value: string) =>
      value.charAt(0).toUpperCase() + value.slice(1);
    if (step.effort) return `This step: ${capitalized(step.effort)}`;
    const staged = usesStage
      ? stageEfforts?.[key] || stageEfforts?.[provider]
      : undefined;
    if (staged) return `Settings → ${stage}: ${capitalized(staged)}`;
    return unsetEffortLabel(settings, provider);
  };

  const updateModel = (key: string, value: string) => {
    const next = { ...(step.model_overrides ?? {}) };
    const selection = decodeModelSelection(value);
    if (selection) next[key] = selection;
    else delete next[key];
    onChange({ model: "", model_overrides: next });
  };

  const updateEffort = (key: string, value: string) => {
    const next = { ...(step.effort_overrides ?? {}) };
    if (value) next[key] = value;
    else delete next[key];
    onChange({ effort: "", effort_overrides: next });
  };

  return (
    <div>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="text-[0.6875rem] text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 flex items-center gap-1"
      >
        <span>{open ? "▾" : "▸"}</span>
        <span>Model overrides</span>
        {hasOverride && !open && (
          <span className="text-[0.625rem] text-gray-600 dark:text-gray-400 font-mono ml-1">
            {[
              step.model,
              step.effort,
              ...Object.entries(step.model_overrides ?? {}).map(
                ([key, selection]) =>
                  `${key}=${selection.mode === "automatic" ? "auto" : selection.mode === "role" ? selection.role : selection.model}`,
              ),
            ]
              .filter(Boolean)
              .join(" / ")}
          </span>
        )}
      </button>
      {open && (
        <div className="mt-2 space-y-2 pl-3 border-l-2 border-gray-200 dark:border-gray-700">
          <p className="text-[0.625rem] text-gray-600 dark:text-gray-400 leading-relaxed">
            Keep what this step inherits, or choose a model role or a model by
            name.
          </p>
          {providers.map((provider) => {
            const key = `${provider}:${transportFor(provider)}`;
            const catalog = catalogs[provider];
            const selection = step.model_overrides?.[key];
            const value =
              selection?.mode === "automatic"
                ? "inherit"
                : encodeModelSelection(selection);
            const known =
              value === "inherit" ||
              catalog?.roles.some((role) => value === `role:${role.id}`) ||
              catalog?.models.some((model) => value === `pinned:${model.id}`);
            const efforts = effortOptions(
              catalog,
              selection,
              provider === "claude"
                ? ["low", "medium", "high", "max"]
                : provider === "codex"
                  ? ["low", "medium", "high"]
                  : provider === "antigravity" &&
                      transportFor(provider) === "cli"
                    ? ["low", "medium", "high"]
                    : [],
            );
            return (
              <div key={key} className="space-y-1.5">
                <div className="text-[0.625rem] font-semibold uppercase tracking-wide text-gray-500">
                  {reviewConnectionLabel(provider, transportFor(provider))}
                </div>
                <select
                  aria-label={`${provider} ${transportFor(provider)} model override`}
                  value={value}
                  onChange={(event) => updateModel(key, event.target.value)}
                  className="w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200"
                >
                  <option value="inherit">
                    {inheritedModel(
                      provider,
                      key,
                      selection?.mode === "automatic",
                    )}
                  </option>
                  {!!catalog?.roles.length && (
                    <optgroup label="Stable roles">
                      {catalog.roles.map((role) => (
                        <option key={role.id} value={`role:${role.id}`}>
                          {role.label} — {role.model}
                        </option>
                      ))}
                    </optgroup>
                  )}
                  {!!catalog?.models.length && (
                    <optgroup label="Pin exact model">
                      {catalog.models.map((model) => (
                        <option
                          key={model.id}
                          value={`pinned:${model.id}`}
                          disabled={model.deprecated}
                        >
                          {model.display_name || model.id}
                        </option>
                      ))}
                    </optgroup>
                  )}
                  {!known && (
                    <option value={value}>
                      Saved selection (not currently listed)
                    </option>
                  )}
                </select>
                {!!efforts.length && (
                  <select
                    aria-label={`${provider} ${transportFor(provider)} effort override`}
                    value={step.effort_overrides?.[key] ?? ""}
                    onChange={(event) => updateEffort(key, event.target.value)}
                    className="w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200"
                  >
                    <option value="">{inheritedEffort(provider, key)}</option>
                    {efforts.map((effort) => (
                      <option key={effort} value={effort}>
                        {effort.charAt(0).toUpperCase() + effort.slice(1)}
                      </option>
                    ))}
                  </select>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

// --- Calibrate a synthesis step from rejected issues (Release 1.4) ---

export default memo(ModelOverrides);

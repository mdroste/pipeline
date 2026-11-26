import { providerFamilies } from "../../lib/connections";
import {
  effortOptions,
  PROVIDER_LABELS,
  withDefaultConnection,
  withDefaultProvider,
  type Provider,
} from "../../lib/providers";
import type { DepsReport, Settings } from "../../lib/settingsTypes";
import type { ModelCatalog } from "../../lib/reviewModelTypes";
import {
  unsetEffortLabel,
  unsetEffortOption,
  unsetModelOption,
  unsetModelSummary,
  withoutDefaultModel,
} from "../../lib/modelChoice";
import { SectionHeader, selectClass } from "./controls";
import {
  ModelConnectionField,
  SETTINGS_PROVIDERS,
} from "./ModelConnectionSettings";

const labelClass =
  "flex min-w-0 flex-col gap-1 text-xs text-gray-600 dark:text-neutral-400";

/**
 * One provider, model, and thinking level for Reviews and new conversations.
 * Reviews follow it live; a conversation copies it when created, so changing
 * it never moves one that has already started.
 */
export default function DefaultModelSettings({
  settings,
  setSettings,
  catalogs,
  catalogBlocked,
  dependencies,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
  catalogs: Record<string, ModelCatalog>;
  catalogBlocked: Record<string, boolean>;
  dependencies?: DepsReport | null;
}) {
  const chosen = Boolean(settings.default_model_chosen);
  const provider = settings.preferred_provider;
  const local = provider === "local";
  const model = (local ? settings.local_model : settings.default_model) ?? "";
  const catalog = catalogBlocked[provider] ? undefined : catalogs[provider];
  const providerDefaults = withoutDefaultModel(settings);
  const efforts = (next: string) =>
    next ? effortOptions(catalog, { mode: "pinned", model: next }, []) : [];
  const supported = efforts(model);

  const chooseModel = (next: string) =>
    setSettings({
      ...settings,
      ...(local ? { local_model: next } : { default_model: next }),
      default_effort: efforts(next).includes(settings.default_effort ?? "")
        ? settings.default_effort
        : "",
      default_model_chosen: true,
    });

  return (
    <>
      <SectionHeader
        title="Default model"
        description="Used for reviews and new conversations."
      />
      <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,16rem),1fr))] gap-3">
        <label className={labelClass}>
          Provider
          <select
            aria-label="Default provider"
            className={selectClass}
            value={chosen ? provider : ""}
            onChange={(event) =>
              setSettings(
                withDefaultProvider(settings, event.target.value as Provider),
              )
            }
          >
            {!chosen && (
              <option value="" disabled>
                Choose a provider
              </option>
            )}
            {providerFamilies.map(({ id }) => (
              <option key={id} value={id}>
                {SETTINGS_PROVIDERS[id].label}
              </option>
            ))}
          </select>
        </label>
        {chosen && (
          <ModelConnectionField
            provider={provider as Provider}
            settings={settings}
            dependencies={dependencies}
            ariaLabel="Default connection"
            showSetup
            onChange={(id) => setSettings(withDefaultConnection(settings, id))}
          />
        )}
      </div>
      <div className="mt-3 grid grid-cols-[repeat(auto-fit,minmax(min(100%,10rem),1fr))] gap-2">
        <label className={labelClass}>
          Model
          <select
            aria-label="Default model"
            title={
              model || unsetModelSummary(providerDefaults, provider, catalog)
            }
            className={selectClass}
            value={model}
            disabled={!chosen}
            onChange={(event) => chooseModel(event.target.value)}
          >
            <option value="">
              {local
                ? "Automatic"
                : unsetModelOption(providerDefaults, provider, catalog)}
            </option>
            {catalog?.models.map((item) => (
              <option key={item.id} value={item.id} disabled={item.deprecated}>
                {item.display_name || item.id}
              </option>
            ))}
            {model && !catalog?.models.some((item) => item.id === model) && (
              <option value={model}>
                {catalog ? `${model} (saved, not available)` : model}
              </option>
            )}
          </select>
        </label>
        <label className={labelClass}>
          Thinking
          <select
            aria-label="Default thinking"
            title={
              settings.default_effort ||
              unsetEffortLabel(
                providerDefaults,
                provider,
                catalog,
                model ? { mode: "pinned", model } : undefined,
              )
            }
            className={selectClass}
            value={settings.default_effort ?? ""}
            disabled={!chosen || !supported.length}
            onChange={(event) =>
              setSettings({ ...settings, default_effort: event.target.value })
            }
          >
            <option value="">
              {unsetEffortOption(
                providerDefaults,
                provider,
                catalog,
                model ? { mode: "pinned", model } : undefined,
              )}
            </option>
            {supported.map((effort) => (
              <option key={effort} value={effort}>
                {effort.charAt(0).toUpperCase() + effort.slice(1)}
              </option>
            ))}
          </select>
        </label>
      </div>
      {!chosen ? (
        <p className="settings-row-description mt-2">
          Until you choose, reviews use{" "}
          {PROVIDER_LABELS[provider as Provider] ?? provider} and new
          conversations start on ChatGPT.
        </p>
      ) : (
        !model && (
          <p className="settings-row-description mt-2">
            Choose a model to set a thinking level.
          </p>
        )
      )}
    </>
  );
}

import {
  connectionAvailability,
  connectionDescription,
  connections,
  type ConnectionId,
} from "../../lib/connections";
import {
  defaultConnectionId,
  effortOptions,
  PROVIDER_LABELS,
  withDefaultConnection,
  type Provider,
} from "../../lib/providers";
import type { DepsReport, ModelCatalog, Settings } from "../../lib/types";
import { SectionHeader, selectClass } from "./controls";

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
  const efforts = (next: string) =>
    next ? effortOptions(catalog, { mode: "pinned", model: next }, []) : [];
  const supported = efforts(model);
  const isMissing = (id: ConnectionId) =>
    dependencies !== undefined &&
    connectionAvailability(id, settings, dependencies) === "missing";
  const connection = defaultConnectionId(settings);

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
        description="Reviews and new conversations use this. A workflow or conversation can choose differently."
      />
      <div className="grid gap-2 sm:grid-cols-3">
        <label className={labelClass}>
          Provider
          <select
            aria-label="Default provider"
            className={selectClass}
            value={chosen ? connection : ""}
            onChange={(event) =>
              setSettings(
                withDefaultConnection(
                  settings,
                  event.target.value as ConnectionId,
                ),
              )
            }
          >
            {!chosen && (
              <option value="" disabled>
                Choose a provider
              </option>
            )}
            {connections.map(({ id, label }) => (
              <option key={id} value={id}>
                {label}
                {isMissing(id) ? " (not connected)" : ""}
              </option>
            ))}
          </select>
        </label>
        <label className={labelClass}>
          Model
          <select
            aria-label="Default model"
            className={selectClass}
            value={model}
            disabled={!chosen}
            onChange={(event) => chooseModel(event.target.value)}
          >
            <option value="">Automatic</option>
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
            className={selectClass}
            value={settings.default_effort ?? ""}
            disabled={!chosen || !supported.length}
            onChange={(event) =>
              setSettings({ ...settings, default_effort: event.target.value })
            }
          >
            <option value="">Default</option>
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
      ) : isMissing(connection) ? (
        <p className="mt-2 text-xs text-amber-700 dark:text-amber-300">
          {connectionDescription(connection).label} isn’t connected. Connect it
          below before starting a review.
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

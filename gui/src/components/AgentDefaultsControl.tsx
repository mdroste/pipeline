import type { ReactNode } from "react";
import { providerAvailability } from "../lib/connections";
import type {
  DepsReport,
  ModelCatalog,
  ModelSelection,
  Settings,
} from "../lib/types";
import {
  decodeModelSelection,
  effortOptions,
  encodeModelSelection,
  inheritedModelLabel,
  PROVIDER_LABELS,
  providerTransport,
  type Provider,
} from "../lib/providers";

interface Props {
  agents: string[];
  modelOverrides?: Record<string, ModelSelection>;
  effortOverrides?: Record<string, string>;
  settings: Settings;
  catalogs: Record<string, ModelCatalog>;
  providers: readonly Provider[];
  multi?: boolean;
  /** One column for narrow panels, where side-by-side menus can't show a name. */
  stacked?: boolean;
  /** What readiness last saw; undefined leaves providers unlabeled. */
  dependencies?: DepsReport | null;
  /** Set when this stage can follow the default instead of its own choice. */
  inherit?: { active: boolean; label: string; onSelect: () => void };
  label: string;
  help?: ReactNode;
  disabled?: boolean;
  onChange: (value: {
    agents: string[];
    modelOverrides: Record<string, ModelSelection>;
    effortOverrides: Record<string, string>;
  }) => void;
}

const chipClass =
  "rounded-full border px-2.5 py-1 text-[13px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50";
const chipOn =
  "border-gray-900 bg-gray-900 text-white dark:border-neutral-100 dark:bg-neutral-100 dark:text-neutral-950";
const chipMissing =
  "border-dashed border-gray-300 bg-transparent text-gray-400 hover:border-gray-500 dark:border-neutral-700 dark:text-neutral-500 dark:hover:border-neutral-500";
const chipOff =
  "border-gray-300 bg-white text-gray-600 hover:border-gray-500 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-400 dark:hover:border-neutral-500";

export default function AgentDefaultsControl({
  agents,
  modelOverrides = {},
  effortOverrides = {},
  settings,
  catalogs,
  providers,
  multi = false,
  stacked = false,
  dependencies,
  inherit,
  label,
  help,
  disabled = false,
  onChange,
}: Props) {
  const inheriting = inherit?.active ?? false;
  const notConnected = (provider: string) =>
    providerAvailability(provider, settings, dependencies) === "missing";
  const updateAgents = (provider: Provider) => {
    const active = agents.includes(provider);
    let next: string[];
    if (multi) {
      if (!inheriting && active && agents.length === 1) return;
      // Leaving the default keeps its provider unless that one was chosen.
      if (inheriting) next = active ? [provider] : [...agents, provider];
      else
        next = active
          ? agents.filter((agent) => agent !== provider)
          : [...agents, provider];
    } else {
      next = [provider];
    }
    onChange({ agents: next, modelOverrides, effortOverrides });
  };

  const updateModel = (provider: Provider, key: string, value: string) => {
    const next = { ...modelOverrides };
    const selection = decodeModelSelection(value);
    if (selection) next[key] = selection;
    else {
      delete next[key];
      // Old settings used the provider name without a transport suffix. If it
      // remains, display resolution immediately resurrects the override.
      delete next[provider];
    }
    onChange({ agents, modelOverrides: next, effortOverrides });
  };

  const updateEffort = (provider: Provider, key: string, value: string) => {
    const next = { ...effortOverrides };
    if (value) next[key] = value;
    else {
      delete next[key];
      delete next[provider];
    }
    onChange({ agents, modelOverrides, effortOverrides: next });
  };

  return (
    <div className="space-y-2.5">
      <div>
        <p className="text-xs font-medium text-gray-700 dark:text-neutral-300">
          {label}
        </p>
        {help && (
          <p className="mt-0.5 text-[13px] leading-4 text-gray-500 dark:text-neutral-500">
            {help}
          </p>
        )}
      </div>
      <div
        role="group"
        aria-label={`${label} providers`}
        className="flex flex-wrap gap-1.5"
      >
        {inherit && (
          <button
            type="button"
            aria-pressed={inheriting}
            disabled={disabled}
            onClick={inherit.onSelect}
            className={`${chipClass} ${inheriting ? chipOn : chipOff}`}
          >
            {inherit.label}
          </button>
        )}
        {providers.map((provider) => {
          const active = !inheriting && agents.includes(provider);
          const missing = notConnected(provider);
          return (
            <button
              key={provider}
              type="button"
              aria-pressed={active}
              disabled={disabled}
              title={missing ? "Not connected" : undefined}
              onClick={() => updateAgents(provider)}
              className={`${chipClass} ${active ? chipOn : missing ? chipMissing : chipOff}`}
            >
              {PROVIDER_LABELS[provider]}
            </button>
          );
        })}
      </div>
      <div className="space-y-2">
        {(inheriting ? [] : agents).map((providerName) => {
          const provider = providerName as Provider;
          const transport = providerTransport(settings, provider);
          const key = `${provider}:${transport}`;
          const selection = modelOverrides[key] ?? modelOverrides[provider];
          const value =
            selection?.mode === "automatic"
              ? "inherit"
              : encodeModelSelection(selection);
          const catalog = catalogs[provider];
          const known =
            value === "inherit" ||
            catalog?.models.some((model) => value === `pinned:${model.id}`);
          // Roles aren't offered here, but a saved one must keep its real name.
          const savedRole =
            selection?.mode === "role"
              ? catalog?.roles.find((role) => role.id === selection.role)
              : undefined;
          const efforts =
            provider === "claude"
              ? effortOptions(catalog, selection, [
                  "low",
                  "medium",
                  "high",
                  "max",
                ])
              : provider === "codex"
                ? effortOptions(catalog, selection, ["low", "medium", "high"])
                : [];
          return (
            <div
              key={provider}
              className={`grid ${stacked ? "grid-cols-1" : "grid-cols-[minmax(64px,0.7fr)_minmax(120px,1.8fr)]"} items-center gap-2 rounded-md border border-gray-200 px-2.5 py-2 dark:border-neutral-800`}
            >
              <span className="truncate text-[13px] font-medium text-gray-700 dark:text-neutral-300">
                {PROVIDER_LABELS[provider]}
              </span>
              <div
                className={`grid gap-1.5 ${efforts.length && !stacked ? "grid-cols-2" : "grid-cols-1"}`}
              >
                <select
                  aria-label={`${label} ${PROVIDER_LABELS[provider]} model`}
                  value={value}
                  disabled={disabled}
                  onChange={(event) =>
                    updateModel(provider, key, event.target.value)
                  }
                  className="min-w-0 rounded border border-gray-300 bg-white px-1.5 py-1 text-[13px] text-gray-800 disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-200"
                >
                  <option value="inherit">
                    {inheritedModelLabel(settings, provider, catalog)}
                  </option>
                  {!!catalog?.models.length && (
                    <optgroup label="Exact model">
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
                      {savedRole
                        ? `${savedRole.label} · ${savedRole.model}`
                        : `${selection?.mode === "pinned" ? selection.model : selection?.mode === "role" ? selection.role : "automatic"} (saved, not available)`}
                    </option>
                  )}
                </select>
                {!!efforts.length && (
                  <select
                    aria-label={`${label} ${PROVIDER_LABELS[provider]} thinking`}
                    value={
                      effortOverrides[key] ?? effortOverrides[provider] ?? ""
                    }
                    disabled={disabled}
                    onChange={(event) =>
                      updateEffort(provider, key, event.target.value)
                    }
                    className="min-w-0 rounded border border-gray-300 bg-white px-1.5 py-1 text-[13px] text-gray-800 disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-200"
                  >
                    <option value="">Default thinking</option>
                    {efforts.map((effort) => (
                      <option key={effort} value={effort}>
                        {effort.charAt(0).toUpperCase() + effort.slice(1)}
                      </option>
                    ))}
                  </select>
                )}
              </div>
              {notConnected(provider) && (
                <p className="col-span-full text-xs text-amber-700 dark:text-amber-300">
                  Not connected. Connect it in Settings → Connections.
                </p>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

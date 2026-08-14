import type { ReactNode } from "react";
import type { ModelCatalog, ModelSelection, Settings } from "../lib/types";
import {
  decodeModelSelection,
  effortOptions,
  encodeModelSelection,
  PROVIDER_LABELS,
  providerSelection,
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
  label: string;
  help?: ReactNode;
  disabled?: boolean;
  onChange: (value: {
    agents: string[];
    modelOverrides: Record<string, ModelSelection>;
    effortOverrides: Record<string, string>;
  }) => void;
}

function inheritedModelLabel(settings: Settings, provider: Provider): string {
  if (provider === "local") return settings.local_model || "not selected";
  const selection = providerSelection(settings, provider);
  if (selection.mode === "automatic") return "automatic";
  if (selection.mode === "role") return selection.role;
  return selection.model;
}

export default function AgentDefaultsControl({
  agents,
  modelOverrides = {},
  effortOverrides = {},
  settings,
  catalogs,
  providers,
  multi = false,
  label,
  help,
  disabled = false,
  onChange,
}: Props) {
  const updateAgents = (provider: Provider) => {
    const active = agents.includes(provider);
    let next: string[];
    if (multi) {
      if (active && agents.length === 1) return;
      next = active ? agents.filter((agent) => agent !== provider) : [...agents, provider];
    } else {
      next = [provider];
    }
    onChange({ agents: next, modelOverrides, effortOverrides });
  };

  const updateModel = (key: string, value: string) => {
    const next = { ...modelOverrides };
    const selection = decodeModelSelection(value);
    if (selection) next[key] = selection;
    else delete next[key];
    onChange({ agents, modelOverrides: next, effortOverrides });
  };

  const updateEffort = (key: string, value: string) => {
    const next = { ...effortOverrides };
    if (value) next[key] = value;
    else delete next[key];
    onChange({ agents, modelOverrides, effortOverrides: next });
  };

  return (
    <div className="space-y-2.5">
      <div>
        <p className="text-xs font-medium text-gray-700 dark:text-neutral-300">{label}</p>
        {help && <p className="mt-0.5 text-[11px] leading-4 text-gray-500 dark:text-neutral-500">{help}</p>}
      </div>
      <div role="group" aria-label={`${label} providers`} className="flex flex-wrap gap-1.5">
        {providers.map((provider) => {
          const active = agents.includes(provider);
          return (
            <button
              key={provider}
              type="button"
              aria-pressed={active}
              disabled={disabled}
              onClick={() => updateAgents(provider)}
              className={`rounded-full border px-2.5 py-1 text-[11px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
                active
                  ? "border-gray-900 bg-gray-900 text-white dark:border-neutral-100 dark:bg-neutral-100 dark:text-neutral-950"
                  : "border-gray-300 bg-white text-gray-600 hover:border-gray-500 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-400 dark:hover:border-neutral-500"
              }`}
            >
              {PROVIDER_LABELS[provider]}
            </button>
          );
        })}
      </div>
      <div className="space-y-2">
        {agents.map((providerName) => {
          const provider = providerName as Provider;
          const transport = providerTransport(settings, provider);
          const key = `${provider}:${transport}`;
          const selection = modelOverrides[key] ?? modelOverrides[provider];
          const value = encodeModelSelection(selection);
          const catalog = catalogs[provider];
          const known = value === "inherit" || value === "automatic"
            || catalog?.models.some((model) => value === `pinned:${model.id}`);
          const efforts = provider === "claude"
            ? effortOptions(catalog, selection, ["low", "medium", "high", "max"])
            : provider === "codex"
              ? effortOptions(catalog, selection, ["low", "medium", "high"])
              : provider === "antigravity" && transport === "cli"
                ? effortOptions(catalog, selection, ["low", "medium", "high"])
                : [];
          return (
            <div
              key={provider}
              className="grid grid-cols-[minmax(64px,0.7fr)_minmax(120px,1.8fr)] items-center gap-2 rounded-md border border-gray-200 px-2.5 py-2 dark:border-neutral-800"
            >
              <span className="truncate text-[11px] font-medium text-gray-700 dark:text-neutral-300">
                {PROVIDER_LABELS[provider]}
              </span>
              <div className={`grid gap-1.5 ${efforts.length ? "grid-cols-2" : "grid-cols-1"}`}>
                <select
                  aria-label={`${label} ${PROVIDER_LABELS[provider]} model`}
                  value={value}
                  disabled={disabled}
                  onChange={(event) => updateModel(key, event.target.value)}
                  className="min-w-0 rounded border border-gray-300 bg-white px-1.5 py-1 text-[11px] text-gray-800 disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-200"
                >
                  <option value="inherit">Provider default · {inheritedModelLabel(settings, provider)}</option>
                  <option value="automatic">Automatic</option>
                  {!!catalog?.models.length && (
                    <optgroup label="Exact model">
                      {catalog.models.map((model) => (
                        <option key={model.id} value={`pinned:${model.id}`} disabled={model.deprecated}>
                          {model.display_name || model.id} · {label}
                        </option>
                      ))}
                    </optgroup>
                  )}
                  {!known && <option value={value}>Saved model (not currently listed)</option>}
                </select>
                {!!efforts.length && (
                  <select
                    aria-label={`${label} ${PROVIDER_LABELS[provider]} thinking`}
                    value={effortOverrides[key] ?? effortOverrides[provider] ?? ""}
                    disabled={disabled}
                    onChange={(event) => updateEffort(key, event.target.value)}
                    className="min-w-0 rounded border border-gray-300 bg-white px-1.5 py-1 text-[11px] text-gray-800 disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-200"
                  >
                    <option value="">Default thinking</option>
                    {efforts.map((effort) => (
                      <option key={effort} value={effort}>{effort.charAt(0).toUpperCase() + effort.slice(1)}</option>
                    ))}
                  </select>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

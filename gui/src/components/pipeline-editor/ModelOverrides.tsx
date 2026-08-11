import { memo, useState } from "react";
import type { ModelCatalog, Settings, StepConfig } from "../../lib/types";
import {
  decodeModelSelection,
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
  const hasOverride = !!(step.model || step.effort || Object.keys(step.model_overrides ?? {}).length || Object.keys(step.effort_overrides ?? {}).length);
  const [open, setOpen] = useState(hasOverride);
  const providers = Array.from(new Set(
    (step.agents?.length ? step.agents : [settings?.preferred_provider || "claude"])
      .map((provider) => provider || "claude"),
  ));

  const transportFor = (provider: string): "cli" | "api" =>
    settings ? providerTransport(settings, provider) : "api";

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
        className="text-[11px] text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 flex items-center gap-1"
      >
        <span>{open ? "▾" : "▸"}</span>
        <span>Model overrides</span>
        {hasOverride && !open && (
          <span className="text-[10px] text-gray-600 dark:text-gray-400 font-mono ml-1">
            {[
              step.model,
              step.effort,
              ...Object.entries(step.model_overrides ?? {}).map(([key, selection]) =>
                `${key}=${selection.mode === "automatic" ? "auto" : selection.mode === "role" ? selection.role : selection.model}`
              ),
            ].filter(Boolean).join(" / ")}
          </span>
        )}
      </button>
      {open && (
        <div className="mt-2 space-y-2 pl-3 border-l-2 border-gray-200 dark:border-gray-700">
          <p className="text-[10px] text-gray-600 dark:text-gray-400 leading-relaxed">
            Each provider can inherit its global policy, follow its own current
            default, use a stable role, or pin an exact discovered model.
          </p>
          {providers.map((provider) => {
            const key = `${provider}:${transportFor(provider)}`;
            const catalog = catalogs[provider];
            const selection = step.model_overrides?.[key];
            const value = encodeModelSelection(selection);
            const known = value === "inherit" || value === "automatic"
              || catalog?.roles.some((role) => value === `role:${role.id}`)
              || catalog?.models.some((model) => value === `pinned:${model.id}`);
            const efforts = effortOptions(
              catalog,
              selection,
              provider === "claude" ? ["low", "medium", "high", "max"]
                : provider === "codex" ? ["low", "medium", "high"] : [],
            );
            return (
              <div key={key} className="space-y-1.5">
                <div className="text-[10px] font-semibold uppercase tracking-wide text-gray-500">
                  {provider} · {transportFor(provider)}
                </div>
                <select
                  aria-label={`${provider} ${transportFor(provider)} model override`}
                  value={value}
                  onChange={(event) => updateModel(key, event.target.value)}
                  className="w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200"
                >
                  <option value="inherit">Inherit global policy</option>
                  <option value="automatic">Automatic — provider default</option>
                  {!!catalog?.roles.length && <optgroup label="Stable roles">
                    {catalog.roles.map((role) => <option key={role.id} value={`role:${role.id}`}>{role.label} — {role.model}</option>)}
                  </optgroup>}
                  {!!catalog?.models.length && <optgroup label="Pin exact model">
                    {catalog.models.map((model) => <option key={model.id} value={`pinned:${model.id}`} disabled={model.deprecated}>{model.display_name || model.id}</option>)}
                  </optgroup>}
                  {!known && <option value={value}>Saved selection (not currently listed)</option>}
                </select>
                {!!efforts.length && (
                  <select
                    aria-label={`${provider} ${transportFor(provider)} effort override`}
                    value={step.effort_overrides?.[key] ?? ""}
                    onChange={(event) => updateEffort(key, event.target.value)}
                    className="w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200"
                  >
                    <option value="">Inherit effort</option>
                    {efforts.map((effort) => <option key={effort} value={effort}>{effort.charAt(0).toUpperCase() + effort.slice(1)}</option>)}
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

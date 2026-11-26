import { useLayoutEffect, useRef, useState } from "react";
import { providerAvailability } from "../../lib/connections";
import {
  unsetEffortLabel,
  unsetEffortOption,
  unsetModelSummary,
  withoutDefaultModel,
} from "../../lib/modelChoice";
import type { DepsReport, Settings } from "../../lib/settingsTypes";
import type { ModelCatalog, ModelSelection } from "../../lib/reviewModelTypes";
import {
  decodeModelSelection,
  effortOptions,
  encodeModelSelection,
  inheritedModelLabel,
  PROVIDER_LABELS,
  providerTransport,
  type Provider,
} from "../../lib/providers";
import { Icon } from "../../ui/icons";
import IconButton from "../../ui/IconButton";
import { Menu, MenuItem } from "../../ui/Menu";

export interface ReviewModelsValue {
  agents: string[];
  modelOverrides: Record<string, ModelSelection>;
  effortOverrides: Record<string, string>;
}

interface Props extends ReviewModelsValue {
  label: string;
  settings: Settings;
  catalogs: Record<string, ModelCatalog>;
  providers: readonly Provider[];
  dependencies?: DepsReport | null;
  disabled?: boolean;
  multi?: boolean;
  team?: boolean;
  onChange: (value: ReviewModelsValue) => void;
}

/** One compact row per provider. Model maps retain their transport-qualified keys. */
export default function ReviewModelRows({
  agents,
  modelOverrides,
  effortOverrides,
  label,
  settings,
  catalogs,
  providers,
  dependencies,
  disabled = false,
  multi = false,
  team = false,
  onChange,
}: Props) {
  const [adding, setAdding] = useState(false);
  const groupRef = useRef<HTMLDivElement>(null);
  const addRef = useRef<HTMLButtonElement>(null);
  const restoreAddFocus = useRef(false);
  const focusNewReviewer = useRef(false);
  useLayoutEffect(() => {
    if (restoreAddFocus.current) {
      addRef.current?.focus();
      restoreAddFocus.current = false;
    }
    if (focusNewReviewer.current) {
      const fields = groupRef.current?.querySelectorAll<HTMLSelectElement>(
        ".review-model-provider select",
      );
      fields?.[fields.length - 1]?.focus();
      focusNewReviewer.current = false;
    }
  }, [agents]);
  const choices = [...new Set([...providers, ...agents])] as Provider[];
  const available = providers.filter((provider) => !agents.includes(provider));
  const missing = (provider: string) =>
    providerAvailability(provider, settings, dependencies) === "missing";
  const publish = (patch: Partial<ReviewModelsValue>) =>
    onChange({ agents, modelOverrides, effortOverrides, ...patch });
  const clearProvider = <T,>(map: Record<string, T>, provider: string) =>
    Object.fromEntries(
      Object.entries(map).filter(
        ([key]) => key !== provider && !key.startsWith(`${provider}:`),
      ),
    );

  return (
    <div
      ref={groupRef}
      className={`review-model-rows ${team ? "review-team-rows" : ""}`}
      role="group"
      aria-label={label}
    >
      {agents.map((provider, index) => {
        const key = `${provider}:${providerTransport(settings, provider)}`;
        const selection = modelOverrides[key] ?? modelOverrides[provider];
        const model =
          selection?.mode === "automatic"
            ? "inherit"
            : encodeModelSelection(selection);
        const catalog = catalogs[provider];
        const modelSettings =
          selection?.mode === "automatic"
            ? withoutDefaultModel(settings)
            : settings;
        const known =
          model === "inherit" ||
          catalog?.models.some((entry) => model === `pinned:${entry.id}`);
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
        const effort = effortOverrides[key] ?? effortOverrides[provider] ?? "";
        const name = PROVIDER_LABELS[provider as Provider] ?? provider;
        return (
          <div className="review-model-row" key={index}>
            <label className="review-model-provider">
              <span>
                {team && index === 0
                  ? "Primary model"
                  : multi
                    ? team
                      ? "Additional model"
                      : `Model ${index + 1}`
                    : "Provider"}
              </span>
              <select
                aria-label={`${label} ${index + 1} provider`}
                value={provider}
                disabled={disabled}
                onChange={(event) => {
                  const next = event.target.value;
                  publish({
                    agents: agents.map((agent, i) =>
                      i === index ? next : agent,
                    ),
                    modelOverrides: clearProvider(
                      clearProvider(modelOverrides, provider),
                      next,
                    ),
                    effortOverrides: clearProvider(
                      clearProvider(effortOverrides, provider),
                      next,
                    ),
                  });
                }}
              >
                {choices.map((choice) => (
                  <option
                    key={choice}
                    value={choice}
                    disabled={choice !== provider && agents.includes(choice)}
                  >
                    {PROVIDER_LABELS[choice] ?? choice}
                    {missing(choice) ? " · Not connected" : ""}
                  </option>
                ))}
              </select>
            </label>
            <label className="review-model-model">
              <span>Model</span>
              <select
                aria-label={`${label} ${name} model`}
                title={
                  selection?.mode === "pinned"
                    ? selection.model
                    : selection?.mode === "role"
                      ? (savedRole?.model ?? selection.role)
                      : unsetModelSummary(modelSettings, provider, catalog)
                }
                value={model}
                disabled={disabled}
                onChange={(event) => {
                  const next = { ...modelOverrides };
                  const selected = decodeModelSelection(event.target.value);
                  if (selected) next[key] = selected;
                  else {
                    delete next[key];
                    delete next[provider];
                  }
                  publish({ modelOverrides: next });
                }}
              >
                <option value="inherit">
                  {inheritedModelLabel(modelSettings, provider, catalog)}
                </option>
                {!!catalog?.models.length && (
                  <optgroup label="Exact model">
                    {catalog.models.map((entry) => (
                      <option
                        key={entry.id}
                        value={`pinned:${entry.id}`}
                        disabled={entry.deprecated}
                      >
                        {entry.display_name || entry.id}
                      </option>
                    ))}
                  </optgroup>
                )}
                {!known && (
                  <option value={model}>
                    {savedRole
                      ? `${savedRole.label} · ${savedRole.model}`
                      : `${selection?.mode === "pinned" ? selection.model : selection?.mode === "role" ? selection.role : "automatic"} (saved, not available)`}
                  </option>
                )}
              </select>
            </label>
            <label className="review-model-effort">
              <span>Thinking</span>
              <select
                aria-label={`${label} ${name} thinking`}
                title={
                  effort ||
                  unsetEffortLabel(settings, provider, catalog, selection)
                }
                value={effort}
                disabled={disabled || (!efforts.length && !effort)}
                onChange={(event) => {
                  const next = { ...effortOverrides };
                  if (event.target.value) next[key] = event.target.value;
                  else {
                    delete next[key];
                    delete next[provider];
                  }
                  publish({ effortOverrides: next });
                }}
              >
                <option value="">
                  {unsetEffortOption(settings, provider, catalog, selection)}
                </option>
                {efforts.map((item) => (
                  <option key={item} value={item}>
                    {item.charAt(0).toUpperCase() + item.slice(1)}
                  </option>
                ))}
                {effort && !efforts.includes(effort) && (
                  <option value={effort}>{effort} (saved)</option>
                )}
              </select>
            </label>
            {multi && !(team && index === 0) && (
              <IconButton
                className="review-remove-model"
                label={`Remove model ${index + 1}`}
                disabled={disabled || agents.length === 1}
                onClick={() => {
                  restoreAddFocus.current = true;
                  publish({
                    agents: agents.filter((_, i) => i !== index),
                    modelOverrides: clearProvider(modelOverrides, provider),
                    effortOverrides: clearProvider(effortOverrides, provider),
                  });
                }}
              >
                <Icon name="close" className="h-3.5 w-3.5" />
              </IconButton>
            )}
          </div>
        );
      })}
      {multi && (
        <>
          <button
            ref={addRef}
            type="button"
            className="review-add-model"
            aria-haspopup="menu"
            aria-expanded={adding}
            disabled={disabled || agents.length >= 3 || !available.length}
            onClick={() => setAdding(!adding)}
          >
            <Icon name="plus" className="h-3.5 w-3.5" /> Add model{" "}
            <span>{agents.length} / 3 models</span>
          </button>
          <Menu
            open={adding && !disabled}
            onClose={() => setAdding(false)}
            anchorRef={addRef}
            label="Add review model"
            align="start"
          >
            {available.map((provider) => (
              <MenuItem
                key={provider}
                description={
                  missing(provider)
                    ? "Not connected · Configure in Settings"
                    : undefined
                }
                onSelect={() => {
                  if (!disabled && agents.length < 3) {
                    focusNewReviewer.current = true;
                    publish({ agents: [...agents, provider] });
                  }
                }}
              >
                {PROVIDER_LABELS[provider]}
              </MenuItem>
            ))}
          </Menu>
        </>
      )}
    </div>
  );
}

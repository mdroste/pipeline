import { SettingsCardId } from "./SaveState";
import { useId } from "react";
import type { ModelCatalog } from "../../lib/reviewModelTypes";

import InfoButton from "../InfoButton";
import { input } from "../../ui/classes";

export function SettingsCard({
  id,
  children,
}: {
  id: string;
  children: React.ReactNode;
}) {
  return (
    <SettingsCardId.Provider value={id}>
      <section id={id} tabIndex={-1} className="settings-card settings-anchor">
        {children}
      </section>
    </SettingsCardId.Provider>
  );
}

export function ProviderHeader({
  name,
  description,
  children,
}: {
  name: string;
  description?: string;
  children?: React.ReactNode;
}) {
  return (
    <header className="settings-provider-header">
      <div className="min-w-0 flex-1">
        <h2 className="text-base font-semibold">{name}</h2>
        {description && (
          <p className="text-xs leading-5 text-gray-500 dark:text-neutral-400">
            {description}
          </p>
        )}
      </div>
      {children}
    </header>
  );
}

export function CatalogStatus({
  blocked = false,
  catalog,
  loading,
  onRefresh,
}: {
  blocked?: boolean;
  catalog?: ModelCatalog;
  loading?: boolean;
  onRefresh: () => void;
}) {
  return (
    <div className="mt-1.5 flex items-start justify-between gap-3 text-[0.6875rem] text-gray-500 dark:text-neutral-400">
      <span>
        {loading
          ? "Loading models…"
          : blocked
            ? "Waiting to save. Choose Refresh to load models now."
            : catalog
              ? `${catalog.models.length} ${catalog.models.length === 1 ? "model" : "models"}${catalog.stale ? " · list may be out of date" : ""}`
              : "Models not loaded"}
        {catalog?.warning && (
          <span className="block text-amber-700 dark:text-amber-300">
            {catalog.warning}
          </span>
        )}
      </span>
      <button
        type="button"
        onClick={onRefresh}
        disabled={loading}
        className="shrink-0 underline disabled:opacity-40"
      >
        Refresh
      </button>
    </div>
  );
}

export const selectClass = "w-full";
export const inputClass = input;

export function SectionHeader({
  title,
  description,
  help,
}: {
  title: string;
  description?: string;
  help?: React.ReactNode;
}) {
  return (
    <div className="mb-3">
      <div className="flex items-center gap-1.5">
        <h2 className="text-base font-semibold text-gray-900 dark:text-neutral-100">
          {title}
        </h2>
        {help && <InfoButton label={title}>{help}</InfoButton>}
      </div>
      {description && (
        <p className="text-sm text-gray-500 dark:text-neutral-400 mt-1">
          {description}
        </p>
      )}
    </div>
  );
}

export function Field({
  label,
  help,
  children,
}: {
  label: string;
  help?: React.ReactNode;
  children: React.ReactNode;
}) {
  const labelId = useId();
  const helpId = useId();
  return (
    <div
      className="settings-field"
      role="group"
      aria-labelledby={labelId}
      aria-describedby={help ? helpId : undefined}
    >
      <div id={labelId} className="ui-field-label mb-1.5">
        <span>{label}</span>
      </div>
      {help && (
        <div id={helpId} className="ui-field-help">
          {help}
        </div>
      )}
      {children}
    </div>
  );
}

export function SubsectionHeader({
  label,
  help,
}: {
  label: string;
  help?: React.ReactNode;
}) {
  return (
    <div className="mb-3 flex items-center gap-1.5 pl-4 text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-neutral-400">
      <span>{label}</span>
      {help && <InfoButton label={label}>{help}</InfoButton>}
    </div>
  );
}

export function Toggle({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  const descriptionId = useId();
  return (
    <div className="settings-toggle">
      <div className="settings-row-copy">
        <span className="settings-row-label">{label}</span>
        <p id={descriptionId} className="settings-row-description">
          {description}
        </p>
      </div>
      <button
        type="button"
        role="switch"
        aria-label={label}
        aria-describedby={descriptionId}
        aria-checked={checked}
        onClick={(event) => {
          event.stopPropagation();
          onChange(!checked);
        }}
        className="ui-toggle"
      >
        <span aria-hidden="true" className="ui-toggle-track">
          <span className="ui-toggle-thumb" />
        </span>
      </button>
    </div>
  );
}

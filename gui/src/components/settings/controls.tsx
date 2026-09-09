import { SettingsCardId } from "./SaveState";
import { useId } from "react";
import type { ModelCatalog } from "../../lib/types";

import InfoButton from "../InfoButton";

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
  badge,
}: {
  name: string;
  description: string;
  badge: string;
}) {
  return (
    <header className="settings-provider-header">
      <div className="settings-provider-mark" aria-hidden="true">
        {name === "OpenAI-compatible" ? "<>" : name.slice(0, 1)}
      </div>
      <div className="min-w-0 flex-1">
        <h2 className="text-base font-semibold">{name}</h2>
        <p className="mt-1 text-xs leading-5 text-gray-500 dark:text-neutral-400">
          {description}
        </p>
        <span className="settings-scope">{badge}</span>
      </div>
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
    <div className="mt-1.5 flex items-start justify-between gap-3 text-[11px] text-gray-500 dark:text-neutral-400">
      <span>
        {loading
          ? "Discovering models…"
          : blocked
            ? "Waiting for autosave; Refresh to discover these values now"
            : catalog
              ? `${catalog.transport.toUpperCase()} · ${catalog.source_version || catalog.source}${catalog.stale ? " · stale" : ""}`
              : "Catalog not loaded"}
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

export const selectClass =
  "w-full py-2 px-3 border border-gray-300 dark:border-neutral-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-neutral-800 dark:text-neutral-200 focus:outline-none focus:ring-2 focus:ring-gray-900/20 dark:focus:ring-neutral-100/20 transition-[box-shadow,color,background-color,border-color]";

export const inputClass =
  "w-full py-2 px-3 border border-gray-300 dark:border-neutral-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-neutral-800 dark:text-neutral-200 focus:outline-none focus:ring-2 focus:ring-gray-900/20 dark:focus:ring-neutral-100/20 transition-[box-shadow,color,background-color,border-color]";

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
    <div className="mb-6">
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
  return (
    <div className="settings-field">
      <div className="mb-1.5 flex items-center gap-1.5 text-sm font-medium text-gray-700 dark:text-neutral-300">
        <span>{label}</span>
      </div>
      {help && <div className="settings-field-help">{help}</div>}
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
        className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors mt-0.5 ${
          checked
            ? "bg-gray-900 dark:bg-neutral-200"
            : "bg-gray-300 dark:bg-neutral-600"
        }`}
      >
        <span
          className={`inline-block h-3.5 w-3.5 rounded-full bg-white dark:bg-neutral-900 transition-transform ${
            checked ? "translate-x-[18px]" : "translate-x-[3px]"
          }`}
        />
      </button>
    </div>
  );
}

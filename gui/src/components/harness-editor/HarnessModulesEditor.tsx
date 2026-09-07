// Module selection grouped by kind, with capability bundle chips above the
// fine-grained rows and the session's unavailability reasons inline. Follows
// ArtifactContextEditor: preset chips first, then an exact allowlist.

import { memo, useMemo } from "react";
import type { HarnessModule, ModuleAvailability } from "../../lib/workbenchTypes";
import {
  activeBundle,
  applyBundle,
  availabilityFor,
  groupModules,
  moduleBundles,
  toggleModule,
} from "./harnessHelpers";

interface Props {
  catalog: HarnessModule[];
  selected: string[];
  readOnly: boolean;
  availability: ModuleAvailability[] | undefined;
  busy: boolean;
  onChange: (next: string[]) => void;
  /** Offered beside an execution module that only needs Edit access mode. */
  onSwitchToEdit?: () => void;
  conversationMode: "inspect" | "edit";
}

const chipClass = (active: boolean) =>
  `rounded-full border px-2.5 py-1 text-[11px] font-medium transition-colors disabled:opacity-50 ${
    active
      ? "border-gray-900 bg-gray-900 text-white dark:border-gray-100 dark:bg-gray-100 dark:text-gray-900"
      : "border-gray-300 bg-white text-gray-700 hover:border-gray-400 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
  }`;

function Switch({ checked, disabled, label, onToggle }: { checked: boolean; disabled: boolean; label: string; onToggle: () => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-label={label}
      aria-checked={checked}
      disabled={disabled}
      onClick={onToggle}
      className={`relative h-5 w-8 shrink-0 rounded-full transition-colors disabled:cursor-not-allowed disabled:opacity-60 ${
        checked ? "bg-green-600" : "bg-gray-300 dark:bg-gray-600"
      }`}
    >
      <span className={`absolute top-0.5 h-4 w-4 rounded-full bg-white shadow transition-transform ${checked ? "translate-x-3.5" : "translate-x-0.5"}`} />
    </button>
  );
}

function HarnessModulesEditor({ catalog, selected, readOnly, availability, busy, onChange, onSwitchToEdit, conversationMode }: Props) {
  const groups = useMemo(() => groupModules(catalog), [catalog]);
  const bundles = useMemo(() => moduleBundles(catalog), [catalog]);
  const active = useMemo(() => activeBundle(selected, bundles, catalog), [selected, bundles, catalog]);

  return (
    <div className="space-y-4">
      <p className="text-xs text-gray-500 dark:text-gray-400">
        This is an exact allowlist. A module the preset does not list never runs; a listed module that this conversation cannot support is shown as unavailable and skipped without failing the turn.
      </p>

      <div>
        <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">Capabilities</div>
        <div className="flex flex-wrap gap-1.5">
          {bundles.map((bundle) => (
            <button
              key={bundle.id}
              type="button"
              aria-pressed={active === bundle.id}
              disabled={readOnly || busy}
              title={bundle.description}
              onClick={() => onChange(applyBundle(selected, bundle, catalog))}
              className={chipClass(active === bundle.id)}
            >
              {bundle.label}
            </button>
          ))}
        </div>
        <p className="mt-1 text-[11px] text-gray-500 dark:text-gray-400">Bundles set the context, tool, and inspector modules together; instruction packs are chosen individually below.</p>
      </div>

      {groups.map((group) => (
        <section key={group.kind} className="rounded-xl border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
          <header className="border-b border-gray-100 px-3 py-2 dark:border-gray-800">
            <h4 className="text-xs font-semibold text-gray-800 dark:text-gray-200">{group.label}</h4>
            <p className="text-[11px] text-gray-500 dark:text-gray-400">{group.hint}</p>
          </header>
          <ul className="divide-y divide-gray-100 dark:divide-gray-800">
            {group.modules.map((module) => {
              const enabled = selected.includes(module.id);
              const state = availabilityFor(module.id, availability);
              const needsEdit = module.capability === "execute" && conversationMode !== "edit";
              return (
                <li key={module.id} className="flex items-start gap-3 px-3 py-2.5">
                  <Switch
                    checked={enabled}
                    disabled={readOnly || busy}
                    label={module.name}
                    onToggle={() => onChange(toggleModule(selected, module.id, catalog))}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-sm font-medium text-gray-800 dark:text-gray-200">{module.name}</span>
                      <span className="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-[10px] text-gray-600 dark:bg-gray-800 dark:text-gray-300">{module.capability}</span>
                      {module.requiresWorkspace && (
                        <span className="rounded bg-blue-50 px-1.5 py-0.5 text-[10px] font-medium text-blue-700 dark:bg-blue-950/60 dark:text-blue-300">Workspace</span>
                      )}
                    </div>
                    <p className="text-[11px] text-gray-500 dark:text-gray-400">{module.description}</p>
                    {enabled && !state.available && (
                      <div className="mt-1.5 rounded border border-amber-300 bg-amber-50 px-2 py-1.5 text-[11px] text-amber-800 dark:border-amber-700 dark:bg-amber-950/40 dark:text-amber-300">
                        <span className="font-medium">In this conversation:</span> {state.reasons.join(" ")}
                        {needsEdit && onSwitchToEdit && (
                          <button
                            type="button"
                            disabled={busy}
                            onClick={onSwitchToEdit}
                            className="ml-2 rounded border border-amber-400 bg-white px-2 py-0.5 text-[11px] font-medium text-amber-900 hover:bg-amber-100 dark:bg-transparent dark:text-amber-200"
                          >
                            Switch this conversation to Edit
                          </button>
                        )}
                      </div>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}

export default memo(HarnessModulesEditor);

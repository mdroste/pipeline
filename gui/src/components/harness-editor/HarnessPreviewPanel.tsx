// Read-only view of what the next turn will actually use: the assembled
// developer instructions as labelled host-owned bands, tools, and provenance.

import { memo } from "react";
import type { ConversationSnapshot, EffectiveHarness, HarnessCatalog } from "../../lib/workbenchTypes";
import { compactAccessSummary, compactHash, instructionSections } from "./harnessHelpers";

interface Props {
  effective: EffectiveHarness;
  catalog: HarnessCatalog;
  snapshot: ConversationSnapshot;
}

function HarnessPreviewPanel({ effective, catalog, snapshot }: Props) {
  const name = (id: string) => catalog.modules.find((module) => module.id === id)?.name ?? id;
  const facts: Array<[string, string]> = [
    ["Preset", effective.preset.name],
    ["Access", compactAccessSummary(effective)],
    ["Permission profile", effective.permissionProfile],
    ["Dynamic tools", String(effective.dynamicTools.length)],
    ["Context", effective.contextPreview ? `${Math.round(effective.contextPreview.length / 1024)} KiB${effective.contextTruncated ? " · truncated at budget" : ""}` : "none"],
    ["Fingerprint", compactHash(effective.fingerprint)],
  ];
  return (
    <div className="space-y-4">
      <p className="text-xs text-gray-500 dark:text-gray-400">
        Everything below is computed by Pipeline from the resolved settings and snapshotted immutably before each turn. Nothing here is editable; change the preset or access scopes instead.
      </p>
      <dl className="grid grid-cols-2 gap-x-4 gap-y-1 rounded-xl border border-gray-200 bg-white p-3 text-xs dark:border-gray-700 dark:bg-gray-900 sm:grid-cols-3">
        {facts.map(([label, value]) => (
          <div key={label} className="min-w-0">
            <dt className="text-[10px] font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">{label}</dt>
            <dd className="truncate text-gray-800 dark:text-gray-200" title={value}>{value}</dd>
          </div>
        ))}
      </dl>
      <section>
        <h3 className="text-[10px] font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">Modules</h3>
        <div className="mt-1 flex flex-wrap gap-1">
          {effective.enabledModules.map((id) => <span key={id} className="rounded bg-emerald-100 px-2 py-0.5 text-[11px] text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200">{name(id)}</span>)}
          {effective.unavailableModules.map((id) => <span key={id} className="rounded bg-gray-200 px-2 py-0.5 text-[11px] text-gray-500 dark:bg-neutral-800">{name(id)} · unavailable</span>)}
          {effective.enabledModules.length + effective.unavailableModules.length === 0 && <span className="text-[11px] text-gray-500">No modules; plain conversation.</span>}
        </div>
      </section>
      {effective.diagnostics.map((item) => (
        <p key={item} className="rounded border border-amber-200 bg-amber-50 p-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-200">{item}</p>
      ))}
      <section className="space-y-2">
        <h3 className="text-[10px] font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">Developer instructions, in order</h3>
        {instructionSections(effective).map((section) => (
          <article key={section.id} className="rounded-xl border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
            <h4 className="border-b border-gray-100 px-3 py-1.5 text-[11px] font-semibold text-gray-700 dark:border-gray-800 dark:text-gray-300">{section.label}</h4>
            <pre className="max-h-64 overflow-auto whitespace-pre-wrap p-3 text-[11px] leading-relaxed text-gray-700 dark:text-gray-300">{section.text}</pre>
          </article>
        ))}
      </section>
      <section className="text-[11px] text-gray-500 dark:text-gray-400">
        <h3 className="text-[10px] font-semibold uppercase tracking-wide">Native instruction sources</h3>
        <p className="mt-1">{snapshot.activeBinding?.instructionSources.length ? snapshot.activeBinding.instructionSources.join(", ") : "None reported by the active thread."}</p>
      </section>
    </div>
  );
}

export default memo(HarnessPreviewPanel);

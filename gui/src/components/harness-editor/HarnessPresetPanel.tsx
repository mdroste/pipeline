// Detail panel for one harness preset: editable header with a generated
// summary, ownership badges, use/clone/restore/save actions, and Instructions
// and Modules tabs. Built-ins render read-only; clones own their copy.

import { memo, useState, type KeyboardEvent } from "react";
import type { EffectiveHarness, HarnessCatalog, HarnessPreset } from "../../lib/workbenchTypes";
import HarnessInstructionsEditor from "./HarnessInstructionsEditor";
import HarnessModulesEditor from "./HarnessModulesEditor";
import { describeModules, instructionBlocks } from "./harnessHelpers";

export interface PresetDraft {
  name: string;
  description: string;
  instructions: string;
  modules: string[];
}

interface Props {
  preset: HarnessPreset;
  sourcePreset: HarnessPreset | null;
  catalog: HarnessCatalog;
  effective: EffectiveHarness;
  inUse: boolean;
  draft: PresetDraft;
  dirty: boolean;
  busy: boolean;
  onDraft: (patch: Partial<PresetDraft>) => void;
  onUse: () => void;
  onClone: () => void;
  onRestore: () => void;
  onSave: () => void;
  onDiscard: () => void;
  onSwitchToEdit: () => void;
}

type TabId = "instructions" | "modules";
const TABS: Array<[TabId, string]> = [["instructions", "Instructions"], ["modules", "Modules"]];

const primaryButton = "rounded-lg bg-gray-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-gray-800 disabled:opacity-50 dark:bg-gray-100 dark:text-gray-900 dark:hover:bg-gray-200";
const secondaryButton = "rounded-lg border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-100 disabled:opacity-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-800";
const inputClass = "w-full rounded border border-gray-300 bg-white px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100";

function HarnessPresetPanel({
  preset, sourcePreset, catalog, effective, inUse, draft, dirty, busy,
  onDraft, onUse, onClone, onRestore, onSave, onDiscard, onSwitchToEdit,
}: Props) {
  const [tab, setTab] = useState<TabId>("instructions");
  const readOnly = preset.builtIn;
  const summary = describeModules(draft.modules, catalog.modules, draft.instructions);

  const handleTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>, current: TabId) => {
    const index = TABS.findIndex(([id]) => id === current);
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % TABS.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + TABS.length) % TABS.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = TABS.length - 1;
    else return;
    event.preventDefault();
    setTab(TABS[next][0]);
    document.getElementById(`harness-tab-${TABS[next][0]}`)?.focus();
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="space-y-3 border-b border-gray-200 p-4 dark:border-gray-700">
        <div className="flex flex-wrap items-center gap-2">
          {readOnly ? (
            <h2 className="text-base font-semibold text-gray-900 dark:text-gray-100">{preset.name}</h2>
          ) : (
            <input
              aria-label="Preset name"
              value={draft.name}
              disabled={busy}
              onChange={(event) => onDraft({ name: event.target.value })}
              className={`${inputClass} max-w-sm text-base font-semibold`}
            />
          )}
          <span className={`rounded-full px-2 py-0.5 text-[10px] font-medium ${readOnly ? "bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300" : "bg-violet-100 text-violet-700 dark:bg-violet-950/60 dark:text-violet-300"}`}>
            {readOnly ? "Built-in" : "Custom"}
          </span>
          {!readOnly && sourcePreset && (
            <span className="rounded-full bg-gray-100 px-2 py-0.5 text-[10px] text-gray-600 dark:bg-gray-800 dark:text-gray-300">based on {sourcePreset.name}</span>
          )}
          {inUse && (
            <span className="rounded-full bg-emerald-100 px-2 py-0.5 text-[10px] font-medium text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200">In use by this conversation</span>
          )}
          {dirty && <span className="rounded-full bg-amber-100 px-2 py-0.5 text-[10px] font-medium text-amber-800 dark:bg-amber-950/60 dark:text-amber-200">Unsaved</span>}
        </div>
        <p data-testid="preset-summary" className="text-xs text-gray-600 dark:text-gray-300">{summary}</p>
        {readOnly ? (
          <p className="text-xs text-gray-500 dark:text-gray-400">{preset.description}</p>
        ) : (
          <input
            aria-label="Preset description"
            value={draft.description}
            disabled={busy}
            placeholder="One line shown in the preset list"
            onChange={(event) => onDraft({ description: event.target.value })}
            className={`${inputClass} text-xs`}
          />
        )}
        <div className="flex flex-wrap items-center gap-2">
          {!inUse && <button type="button" disabled={busy || dirty} title={dirty ? "Save or discard changes first" : undefined} onClick={onUse} className={primaryButton}>Use in this conversation</button>}
          <button type="button" disabled={busy} onClick={onClone} className={secondaryButton}>Clone to edit</button>
          {!readOnly && sourcePreset && (
            <button type="button" disabled={busy} onClick={onRestore} title={`Replace this preset's instructions and modules with ${sourcePreset.name}'s current values; the name is kept.`} className={secondaryButton}>
              Restore from {sourcePreset.name}
            </button>
          )}
          {dirty && (
            <>
              <button type="button" disabled={busy} onClick={onSave} className={primaryButton}>Save preset</button>
              <button type="button" disabled={busy} onClick={onDiscard} className={secondaryButton}>Discard</button>
            </>
          )}
        </div>
        {readOnly && (
          <p className="text-[11px] text-gray-500 dark:text-gray-400">
            Built-in presets are read-only. Clone to edit makes an independent copy that Pipeline updates never overwrite.
          </p>
        )}
      </header>

      <div role="tablist" aria-label="Preset sections" className="flex gap-1 border-b border-gray-200 px-4 dark:border-gray-700">
        {TABS.map(([id, label]) => (
          <button
            key={id}
            id={`harness-tab-${id}`}
            type="button"
            role="tab"
            aria-selected={tab === id}
            aria-controls={`harness-panel-${id}`}
            tabIndex={tab === id ? 0 : -1}
            onClick={() => setTab(id)}
            onKeyDown={(event) => handleTabKeyDown(event, id)}
            className={`shrink-0 rounded-t-lg border-b-2 px-3 py-2 text-xs font-medium transition-colors ${
              tab === id
                ? "border-gray-900 text-gray-900 dark:border-gray-100 dark:text-gray-100"
                : "border-transparent text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-200"
            }`}
          >
            {label}
          </button>
        ))}
      </div>

      <section id="harness-panel-instructions" role="tabpanel" aria-labelledby="harness-tab-instructions" hidden={tab !== "instructions"} className="min-h-0 flex-1 overflow-auto p-4">
        <HarnessInstructionsEditor
          value={draft.instructions}
          onChange={(instructions) => onDraft({ instructions })}
          readOnly={readOnly}
          blocks={instructionBlocks(catalog.presets)}
          effective={effective}
          inUse={inUse}
        />
      </section>
      <section id="harness-panel-modules" role="tabpanel" aria-labelledby="harness-tab-modules" hidden={tab !== "modules"} className="min-h-0 flex-1 overflow-auto p-4">
        <HarnessModulesEditor
          catalog={catalog.modules}
          selected={draft.modules}
          readOnly={readOnly}
          availability={effective.moduleAvailability}
          busy={busy}
          onChange={(modules) => onDraft({ modules })}
          onSwitchToEdit={onSwitchToEdit}
          conversationMode={effective.mode}
        />
      </section>
    </div>
  );
}

export default memo(HarnessPresetPanel);

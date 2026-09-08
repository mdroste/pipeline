import useContainerWidth from "../hooks/useContainerWidth";
// Lazy two-pane harness editor for one Workspace conversation. The left
// navigator lists presets and the conversation-level Access and Preview
// entries; the right panel edits the selection. The page owns all remote
// state and the single preset draft, mirroring PipelinePage.

import { useCallback, useEffect, useMemo, useState } from "react";
import SidebarPanel, { SidebarHeader } from "./SidebarPanel";
import PromptDialog from "./pipeline-editor/PromptDialog";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";
import { confirmDialog } from "./DialogService";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { ConversationSnapshot, EffectiveHarness, HarnessCatalog, HarnessPreset, WorkspaceConfig } from "../lib/workbenchTypes";
import HarnessPresetPanel, { type PresetDraft } from "./harness-editor/HarnessPresetPanel";
import HarnessAccessTable from "./harness-editor/HarnessAccessTable";
import HarnessPreviewPanel from "./harness-editor/HarnessPreviewPanel";
import { type AccessKey, type AccessScope, compactAccessSummary, compactModuleSummary, describePreset } from "./harness-editor/harnessHelpers";

type Selection = { kind: "preset"; id: string } | { kind: "access" } | { kind: "preview" };

function operation(prefix: string) { return `${prefix}-${crypto.randomUUID()}`; }

function draftFrom(preset: HarnessPreset): PresetDraft {
  return { name: preset.name, description: preset.description, instructions: preset.instructions, modules: [...preset.modules] };
}

function sameDraft(a: PresetDraft, b: PresetDraft) {
  return a.name === b.name && a.description === b.description && a.instructions === b.instructions
    && a.modules.length === b.modules.length && a.modules.every((id, index) => id === b.modules[index]);
}

const sectionHeader = "sticky top-0 z-[1] bg-gray-50 px-4 py-1.5 text-[10px] font-semibold uppercase tracking-wider text-gray-500 dark:bg-gray-800/50 dark:text-gray-400";

export default function WorkspaceHarnessEditor({ snapshot, onSnapshot, onClose, disabled = false, onBusy, beforeChange }: {
  snapshot: ConversationSnapshot;
  onSnapshot: (snapshot: ConversationSnapshot) => void;
  onClose: () => void;
  disabled?: boolean;
  onBusy?: (busy: boolean) => void;
  beforeChange?: () => Promise<void>;
}) {
  const [root, width] = useContainerWidth<HTMLDivElement>();
  const compact = width !== null && width < 760;
  const sessionId = snapshot.session.id;
  const workspaceId = snapshot.session.workspaceId;
  const [panelWidth, setPanelWidth] = usePersistentPanelWidth("workspace.ui.harnessPanelWidth", 300, 220, 480);
  const [catalog, setCatalog] = useState<HarnessCatalog | null>(null);
  const [effective, setEffective] = useState<EffectiveHarness | null>(null);
  const [globalConfig, setGlobalConfig] = useState<WorkspaceConfig | null>(null);
  const [workspaceConfig, setWorkspaceConfig] = useState<WorkspaceConfig | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  // The draft is keyed by preset id + revision so a save, restore, or
  // selection change reads fresh values on the very next render; no effect
  // can leave a stale draft on screen for one frame.
  const [draftState, setDraftState] = useState<{ key: string; draft: PresetDraft } | null>(null);
  const [naming, setNaming] = useState<{ sourceId: string; defaultName: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    const [nextCatalog, nextEffective, nextGlobal, nextWorkspace] = await Promise.all([
      workbenchClient.harnessCatalog(workspaceId),
      workbenchClient.effectiveHarness(sessionId),
      workbenchClient.getWorkspaceConfig(null),
      workspaceId ? workbenchClient.getWorkspaceConfig(workspaceId) : Promise.resolve(null),
    ]);
    setCatalog(nextCatalog); setEffective(nextEffective); setGlobalConfig(nextGlobal); setWorkspaceConfig(nextWorkspace);
  }, [sessionId, workspaceId]);

  useEffect(() => { void load().catch((cause) => setError(workbenchErrorMessage(cause))); }, [load]);

  useEffect(() => {
    if (!selection && effective) setSelection({ kind: "preset", id: effective.preset.id });
  }, [effective, selection]);

  const selectedPreset = useMemo(
    () => (selection?.kind === "preset" ? catalog?.presets.find((preset) => preset.id === selection.id) ?? null : null),
    [catalog, selection],
  );

  const draftKey = selectedPreset ? `${selectedPreset.id}:${selectedPreset.revision}` : "";
  const draft = selectedPreset ? (draftState?.key === draftKey ? draftState.draft : draftFrom(selectedPreset)) : null;
  const setDraft = (next: PresetDraft) => setDraftState({ key: draftKey, draft: next });

  const dirty = Boolean(selectedPreset && draft && !selectedPreset.builtIn && !sameDraft(draft, draftFrom(selectedPreset)));

  const guardDirty = async () => !dirty || confirmDialog("Discard unsaved changes to this preset?", { title: "Unsaved changes", confirmLabel: "Discard", destructive: true });

  const navigate = async (next: Selection) => {
    if (!(await guardDirty())) return;
    setSelection(next);
  };

  const act = async (fn: () => Promise<void>) => {
    if (busy || disabled) return;
    setBusy(true); onBusy?.(true); setError(null);
    try { await beforeChange?.(); await fn(); await load(); } catch (cause) { setError(workbenchErrorMessage(cause)); } finally { setBusy(false); onBusy?.(false); }
  };

  const patchSession = async (values: { presetId?: string; overrides?: Record<string, unknown> }) => {
    const latest = await workbenchClient.conversationSnapshot(sessionId);
    const updated = await workbenchClient.updateSession({
      sessionId: latest.session.id,
      expectedRevision: latest.session.revision,
      operationId: operation("harness-session"),
      presetId: values.presetId,
      overrides: values.overrides,
    });
    onSnapshot({ ...latest, session: updated.record, sequence: updated.sequence });
  };

  const setAccess = (scope: Exclude<AccessScope, "builtIn">, key: AccessKey, value: string | number | boolean | undefined) => void act(async () => {
    if (scope === "conversation") {
      const latest = await workbenchClient.conversationSnapshot(sessionId);
      const overrides = { ...latest.session.overrides };
      if (value === undefined) delete overrides[key]; else overrides[key] = value;
      await patchSession({ overrides });
      return;
    }
    const config = scope === "global" ? globalConfig : workspaceConfig;
    if (!config) throw new Error(scope === "workspace" ? "Move this conversation to a project to use project defaults." : "Global defaults are still loading.");
    const body = { ...config.body };
    if (value === undefined) delete body[key]; else body[key] = value;
    await workbenchClient.saveWorkspaceConfig({
      workspaceId: scope === "global" ? null : workspaceId,
      expectedRevision: config.revision,
      body,
      operationId: operation(`${scope}-defaults`),
    });
  });

  const savePreset = () => void act(async () => {
    if (!selectedPreset || !draft) return;
    await workbenchClient.updatePreset({
      presetId: selectedPreset.id,
      expectedRevision: selectedPreset.revision,
      name: draft.name.trim(),
      description: draft.description.trim(),
      instructions: draft.instructions,
      modules: draft.modules,
      operationId: operation("edit-preset"),
    });
  });

  const restorePreset = () => void act(async () => {
    if (!selectedPreset || !catalog) return;
    const source = catalog.presets.find((preset) => preset.id === selectedPreset.sourcePresetId);
    if (!source) throw new Error("The source preset is no longer available.");
    const confirmed = await confirmDialog(
      `Replace this preset's description, instructions, and modules with the current values of "${source.name}"? The name is kept and your edits are lost.`,
      { title: "Restore from source", confirmLabel: "Restore", destructive: true },
    );
    if (!confirmed) return;
    await workbenchClient.updatePreset({
      presetId: selectedPreset.id,
      expectedRevision: selectedPreset.revision,
      name: selectedPreset.name,
      description: source.description,
      instructions: source.instructions,
      modules: [...source.modules],
      operationId: operation("restore-preset"),
    });
  });

  const clonePreset = (name: string) => void act(async () => {
    if (!naming) return;
    const source = naming.sourceId;
    setNaming(null);
    const created = await workbenchClient.clonePreset({ workspaceId, sourcePresetId: source, name, operationId: operation("clone-preset") });
    await patchSession({ presetId: created.id });
    setSelection({ kind: "preset", id: created.id });
  });

  const requestClose = () => { if (!busy) void guardDirty().then((ok) => { if (ok) onClose(); }); };

  const successor = Boolean(snapshot.activeBinding?.harnessFingerprint && effective && snapshot.activeBinding.harnessFingerprint !== effective.fingerprint);
  const builtIns = catalog?.presets.filter((preset) => preset.builtIn) ?? [];
  const customs = catalog?.presets.filter((preset) => !preset.builtIn) ?? [];

  const presetRow = (preset: HarnessPreset) => {
    const selected = selection?.kind === "preset" && selection.id === preset.id;
    const inUse = effective?.preset.id === preset.id;
    return (
      <button
        key={preset.id}
        type="button"
        aria-current={selected ? "true" : undefined}
        onClick={() => void navigate({ kind: "preset", id: preset.id })}
        className={`w-full border-b border-gray-100 px-4 py-2.5 text-left transition-colors dark:border-gray-800 ${selected ? "bg-white shadow-sm dark:bg-gray-800" : "hover:bg-gray-100/70 dark:hover:bg-gray-800/50"}`}
      >
        <span className="flex items-center justify-between gap-2">
          <span className="min-w-0">
            <span className="block truncate text-sm font-medium text-gray-800 dark:text-gray-200">{preset.name}</span>
            <span className="block truncate text-[11px] text-gray-500 dark:text-gray-400">{catalog ? describePreset(preset, catalog.modules) : preset.description}</span>
          </span>
          {inUse && <span className="shrink-0 rounded-full bg-emerald-100 px-2 py-0.5 text-[10px] font-medium text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200">In use</span>}
        </span>
      </button>
    );
  };

  const entryRow = (kind: "access" | "preview", label: string, detail: string) => {
    const selected = selection?.kind === kind;
    return (
      <button
        type="button"
        aria-current={selected ? "true" : undefined}
        onClick={() => void navigate({ kind })}
        className={`w-full border-b border-gray-100 px-4 py-2.5 text-left transition-colors dark:border-gray-800 ${selected ? "bg-white shadow-sm dark:bg-gray-800" : "hover:bg-gray-100/70 dark:hover:bg-gray-800/50"}`}
      >
        <span className="block text-sm font-medium text-gray-800 dark:text-gray-200">{label}</span>
        <span className="block truncate text-[11px] text-gray-500 dark:text-gray-400">{detail}</span>
      </button>
    );
  };

  return (
    <div ref={root} className={`relative flex h-full min-h-0 min-w-0 flex-1 ${compact ? "flex-col" : ""}`} data-testid="workspace-harness-editor">
      {compact && <div className="workspace-project-tool-bar">
        <button type="button" onClick={requestClose}>Close editor</button>
        <select aria-label="Assistant editor section" value={selection?.kind === "preset" ? selection.id : selection?.kind ?? ""}
          onChange={event => void navigate(event.target.value === "access" || event.target.value === "preview" ? { kind: event.target.value } : { kind: "preset", id: event.target.value })}>
          {!selection && <option value="">Loading…</option>}
          {catalog?.presets.map(preset => <option key={preset.id} value={preset.id}>{preset.name}</option>)}
          <option value="access">Access & inheritance</option><option value="preview">Effective preview</option>
        </select>
      </div>}
      {!compact && <SidebarPanel aria-label="Harness navigation" width={panelWidth} defaultWidth={300} min={220} max={480} onResize={setPanelWidth} resizeLabel="Resize harness panel">
        <SidebarHeader title="Assistant editor">
          <p className="mt-2 truncate text-xs text-gray-500 dark:text-gray-400">{snapshot.session.title}</p>
          <button type="button" onClick={requestClose} className="mt-3 rounded text-xs text-gray-500 hover:text-gray-800 dark:hover:text-gray-200">Close editor</button>
        </SidebarHeader>
        {successor && (
          <p className="m-2 rounded border border-blue-200 bg-blue-50 p-2 text-[11px] text-blue-800 dark:border-blue-900 dark:bg-blue-950/30 dark:text-blue-200">
            The assistant will start fresh with the new settings and saved project context. Earlier messages remain in the transcript.
          </p>
        )}
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className={sectionHeader}>Presets</div>
          <div className="px-4 pt-2 text-[10px] text-gray-400">Built-in</div>
          {builtIns.map(presetRow)}
          {customs.length > 0 && <div className="px-4 pt-2 text-[10px] text-gray-400">Custom</div>}
          {customs.map(presetRow)}
          {catalog && customs.length === 0 && (
            <p className="px-4 py-2 text-[11px] text-gray-500 dark:text-gray-400">No custom presets yet. Open a built-in and choose Clone to edit.</p>
          )}
          <div className={sectionHeader}>This conversation</div>
          {entryRow("access", "Access & inheritance", effective ? compactAccessSummary(effective) : "Loading…")}
          {entryRow("preview", "Effective preview", effective ? compactModuleSummary(effective) : "Loading…")}
        </div>
      </SidebarPanel>}

      <div className="flex min-h-0 min-w-0 flex-1 flex-col bg-white dark:bg-gray-900">
        {error && <div role="alert" className="m-3 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300">{error}</div>}
        {!catalog || !effective ? (
          <p className="p-6 text-sm text-gray-500">Loading harness…</p>
        ) : selection?.kind === "preset" && selectedPreset && draft ? (
          <HarnessPresetPanel
            key={selectedPreset.id}
            preset={selectedPreset}
            sourcePreset={catalog.presets.find((preset) => preset.id === selectedPreset.sourcePresetId) ?? null}
            catalog={catalog}
            effective={effective}
            inUse={effective.preset.id === selectedPreset.id}
            draft={draft}
            dirty={dirty}
            busy={busy || disabled}
            onDraft={(patch) => setDraft({ ...draft, ...patch })}
            onUse={() => void act(() => patchSession({ presetId: selectedPreset.id }))}
            onClone={() => setNaming({ sourceId: selectedPreset.id, defaultName: `${selectedPreset.name} copy` })}
            onRestore={restorePreset}
            onSave={savePreset}
            onDiscard={() => setDraft(draftFrom(selectedPreset))}
            onSwitchToEdit={() => setAccess("conversation", "mode", "edit")}
          />
        ) : selection?.kind === "access" ? (
          <div className="min-h-0 flex-1 overflow-auto p-4">
            <h2 className="text-base font-semibold text-gray-900 dark:text-gray-100">Access &amp; inheritance</h2>
            <div className="mt-3">
              <HarnessAccessTable
                effective={effective}
                globalBody={globalConfig?.body ?? {}}
                workspaceBody={workspaceId ? workspaceConfig?.body ?? {} : null}
                workspaceName={snapshot.workspace?.name ?? null}
                conversationOverrides={snapshot.session.overrides}
                busy={busy || disabled}
                successor={successor}
                onSet={setAccess}
              />
            </div>
          </div>
        ) : selection?.kind === "preview" ? (
          <div className="min-h-0 flex-1 overflow-auto p-4">
            <h2 className="text-base font-semibold text-gray-900 dark:text-gray-100">Effective preview</h2>
            <div className="mt-3"><HarnessPreviewPanel effective={effective} catalog={catalog} snapshot={snapshot} /></div>
          </div>
        ) : (
          <p className="p-6 text-sm text-gray-500">Select a preset to edit.</p>
        )}
      </div>

      {naming && (
        <PromptDialog
          title="Name for the editable copy"
          defaultValue={naming.defaultName}
          onSubmit={clonePreset}
          onCancel={() => setNaming(null)}
        />
      )}
    </div>
  );
}

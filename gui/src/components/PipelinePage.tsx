import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  Phase,
  PipelineConfig,
  StepConfig,
  MergeConfig,
  ExtractionConfig,
  ProfileSummary,
  ExportEnvelope,
} from "../lib/types";
import { reorderSteps } from "../lib/pipelineHelpers";
import WaveDiagram, { type WaveSelection } from "./WaveDiagram";
import PromptEditor from "./PromptEditor";

interface Props {
  onClose: () => void;
  onProfileChange?: () => void;
}

const DEFAULT_MERGE: MergeConfig = { enabled: true, prompt: "", agents: [] };
const DEFAULT_EXTRACTION: ExtractionConfig = {
  method: "",
  marker_disable_ocr: null,
  marker_disable_images: null,
};

type EditingMode = WaveSelection | null;

export default function PipelinePage({ onClose, onProfileChange }: Props) {
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [editing, setEditing] = useState<EditingMode>(null);

  // Profile state
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [activeProfile, setActiveProfile] = useState<string>("deep-review");
  // Mirrors BUILTIN_PROFILES in pipeline_config.rs — these cannot be deleted.
  const BUILTIN_PROFILES = [
    "deep-review",
    "quick-review",
    "empirical",
    "quick-code-review",
    "deep-code-review",
    "replication-audit",
    "grant-review",
  ];
  const [exportMenuOpen, setExportMenuOpen] = useState(false);

  // Prompt dialog state
  const [promptDialog, setPromptDialog] = useState<{
    title: string;
    defaultValue: string;
    onSubmit: (value: string) => void;
  } | null>(null);
  const [panelWidth, setPanelWidth] = useState(420);

  // Drag-drop state. We track the dragged step's id and the drop target
  // (an insertion index plus the phase that drop site implies). The phase
  // is read from the section the user is hovering over so dropping into a
  // different section auto-flips the step's phase.
  const [dragId, setDragId] = useState<string | null>(null);
  const [dropHint, setDropHint] = useState<{ idx: number; phase: Phase } | null>(null);

  useEffect(() => {
    Promise.all([
      invoke<PipelineConfig>("get_pipeline_config"),
      invoke<ProfileSummary[]>("list_profiles"),
      invoke<string>("get_active_profile"),
    ])
      .then(([c, p, a]) => {
        setConfig(normalizeConfig(c));
        setProfiles(p);
        setActiveProfile(a);
        setLoading(false);
      })
      .catch((e) => {
        console.error(e);
        setLoading(false);
      });
  }, []);

  // Backend may omit fields on older profiles; fill them in locally so the
  // editor never has to guard against undefined.
  function normalizeConfig(c: PipelineConfig): PipelineConfig {
    return {
      ...c,
      merge: c.merge ?? DEFAULT_MERGE,
      extraction: c.extraction ?? DEFAULT_EXTRACTION,
      orientation_prompt: c.orientation_prompt ?? "",
      parallel_context_template: c.parallel_context_template ?? "",
    };
  }

  const refreshProfiles = async () => {
    try {
      const p = await invoke<ProfileSummary[]>("list_profiles");
      setProfiles(p);
    } catch (e) {
      console.error(e);
    }
  };

  // IDs that aren't tied to a specific step's prompt; everything else is treated
  // as a step ID and looked up in config.steps.
  const NON_STEP_EDITORS = new Set(["merge", "pipeline_settings", "extraction", "orientation"]);
  const isStepEditing = !!editing && !NON_STEP_EDITORS.has(editing);

  const editingStep = (config && isStepEditing)
    ? config.steps.find((s) => s.id === editing) ?? null
    : null;

  useEffect(() => {
    if (isStepEditing && !editingStep) {
      setEditing(null);
    }
  }, [isStepEditing, editingStep]);

  if (loading || !config) {
    return (
      <div className="flex items-center justify-center h-full text-gray-400">
        <div className="animate-spin w-6 h-6 border-2 border-gray-300 border-t-gray-600 rounded-full" />
      </div>
    );
  }

  // --- Profile management ---

  const handleSwitchProfile = async (id: string) => {
    if (dirty && !confirm("You have unsaved changes. Switch profile and discard them?")) return;
    try {
      const newConfig = await invoke<PipelineConfig>("switch_profile", { id });
      setConfig(normalizeConfig(newConfig));
      setActiveProfile(id);
      setEditing(null);
      setDirty(false);
      onProfileChange?.();
    } catch (e) {
      alert(`Failed to switch profile: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleNewProfile = () => {
    setPromptDialog({
      title: "New profile name",
      defaultValue: "",
      onSubmit: async (name) => {
        try {
          const summary = await invoke<ProfileSummary>("create_profile", { name });
          await refreshProfiles();
          await handleSwitchProfile(summary.id);
        } catch (e) {
          alert(`Failed to create profile: ${e instanceof Error ? e.message : String(e)}`);
        }
      },
    });
  };

  const handleDuplicateProfile = () => {
    setPromptDialog({
      title: "Name for the duplicate",
      defaultValue: "",
      onSubmit: async (name) => {
        try {
          const summary = await invoke<ProfileSummary>("duplicate_profile", {
            sourceId: activeProfile,
            newName: name,
          });
          await refreshProfiles();
          await handleSwitchProfile(summary.id);
        } catch (e) {
          alert(`Failed to duplicate profile: ${e instanceof Error ? e.message : String(e)}`);
        }
      },
    });
  };

  const handleRenameProfile = () => {
    const current = profiles.find((p) => p.id === activeProfile);
    setPromptDialog({
      title: "Rename profile",
      defaultValue: current?.name ?? "",
      onSubmit: async (name) => {
        try {
          await invoke<ProfileSummary>("rename_profile", { id: activeProfile, newName: name });
          await refreshProfiles();
        } catch (e) {
          alert(`Failed to rename profile: ${e instanceof Error ? e.message : String(e)}`);
        }
      },
    });
  };

  const handleDeleteProfile = async () => {
    if (BUILTIN_PROFILES.includes(activeProfile)) {
      alert("Cannot delete a built-in profile.");
      return;
    }
    const current = profiles.find((p) => p.id === activeProfile);
    if (!confirm(`Delete profile "${current?.name ?? activeProfile}"? This cannot be undone.`)) return;
    try {
      await invoke("delete_profile", { id: activeProfile });
      await refreshProfiles();
      const newConfig = await invoke<PipelineConfig>("switch_profile", { id: "deep-review" });
      setConfig(normalizeConfig(newConfig));
      setActiveProfile("deep-review");
      setEditing(null);
      setDirty(false);
      onProfileChange?.();
    } catch (e) {
      alert(`Failed to delete profile: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  // --- Config editing ---

  const updateStep = (id: string, patch: Partial<StepConfig>) => {
    setConfig({ ...config, steps: config.steps.map((s) => s.id === id ? { ...s, ...patch } : s) });
    setDirty(true);
  };

  const updateMerge = (patch: Partial<MergeConfig>) => {
    setConfig({ ...config, merge: { ...config.merge, ...patch } });
    setDirty(true);
  };

  const updateExtraction = (patch: Partial<ExtractionConfig>) => {
    setConfig({ ...config, extraction: { ...(config.extraction ?? DEFAULT_EXTRACTION), ...patch } });
    setDirty(true);
  };

  const handleSave = async () => {
    setSaving(true); setSaved(false);
    try {
      await invoke("save_pipeline_config", { config });
      setSaved(true); setDirty(false);
      await refreshProfiles();
      onProfileChange?.();
      setTimeout(() => setSaved(false), 2000);
    } catch (e) { alert(`Failed to save: ${e instanceof Error ? e.message : String(e)}`); }
    finally { setSaving(false); }
  };

  const handleReset = async () => {
    if (!confirm("Reset this profile to defaults? All customizations will be lost.")) return;
    try {
      const d = await invoke<PipelineConfig>("reset_pipeline_config");
      setConfig(normalizeConfig(d)); setEditing(null); setDirty(false);
      await refreshProfiles();
      onProfileChange?.();
    } catch (e) { alert(`Failed to reset: ${e instanceof Error ? e.message : String(e)}`); }
  };

  // --- Export/Import ---

  const handleExportItem = async () => {
    if (!editingStep) return;
    const envelope: ExportEnvelope = { type: "step", data: editingStep };
    const defaultName = `pipeline-step-${editingStep.id}.json`;
    const path = await saveDialog({
      defaultPath: defaultName,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (path) {
      try {
        await invoke("export_item", { path, json: JSON.stringify(envelope, null, 2) });
      } catch (e) { alert(`Export failed: ${e instanceof Error ? e.message : String(e)}`); }
    }
  };

  const handleExportProfile = async () => {
    const defaultName = `pipeline-profile-${activeProfile}.json`;
    const path = await saveDialog({
      defaultPath: defaultName,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (path) {
      try {
        if (dirty) await invoke("save_pipeline_config", { config });
        await invoke("export_profile", { id: activeProfile, path });
      } catch (e) { alert(`Export failed: ${e instanceof Error ? e.message : String(e)}`); }
    }
  };

  const handleExportBundle = async () => {
    const path = await saveDialog({
      defaultPath: "pipeline-settings-backup.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (path) {
      try {
        if (dirty) await invoke("save_pipeline_config", { config });
        await invoke("export_bundle", { path });
      } catch (e) { alert(`Export failed: ${e instanceof Error ? e.message : String(e)}`); }
    }
  };

  const handleImport = async () => {
    const path = await openDialog({
      multiple: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!path) return;
    try {
      const envelope = await invoke<ExportEnvelope>("import_item", { path });
      switch (envelope.type) {
        case "step": {
          const step = envelope.data as StepConfig;
          const id = config.steps.some((s) => s.id === step.id)
            ? `${step.id}_${Date.now()}`
            : step.id;
          setConfig({
            ...config,
            steps: [...config.steps, { ...step, id }],
          });
          setEditing(id);
          setDirty(true);
          break;
        }
        case "profile": {
          const summary = await invoke<ProfileSummary>("import_profile", { path });
          await refreshProfiles();
          await handleSwitchProfile(summary.id);
          break;
        }
        case "bundle": {
          if (!confirm("Import all settings and profiles from this file? Existing profiles with the same ID will be overwritten.")) return;
          await invoke("import_bundle", { path });
          const [c, p, a] = await Promise.all([
            invoke<PipelineConfig>("get_pipeline_config"),
            invoke<ProfileSummary[]>("list_profiles"),
            invoke<string>("get_active_profile"),
          ]);
          setConfig(normalizeConfig(c));
          setProfiles(p);
          setActiveProfile(a);
          setEditing(null);
          setDirty(false);
          break;
        }
      }
    } catch (e) {
      alert(`Import failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  // --- Step CRUD ---

  const addStep = (phase: "parallel" | "sequential") => {
    const id = `custom_${Date.now()}`;
    const prompt = phase === "parallel"
      ? "# Custom Step\n\nDescribe what this step should evaluate.\n\n## Method\n\n...\n\n## Output structure\n\n..."
      : "You are performing a custom analysis step.\n\nORIENTATION MAP:\n{orientation}\n\nPRIOR OUTPUTS:\n{prior_outputs}\n\nLAST OUTPUT:\n{last_output}\n\n[Your instructions here]";
    setConfig({
      ...config,
      steps: [...config.steps, {
        id, label: "Custom Step", enabled: true, phase,
        tools: [], agents: [], prompt,
      }],
    });
    setEditing(id); setDirty(true);
  };

  const removeStep = (id: string) => {
    setConfig({ ...config, steps: config.steps.filter((s) => s.id !== id) });
    if (editing === id) setEditing(null);
    setDirty(true);
  };

  // --- Drag and drop ---
  //
  // Visual layout enforces array order = [all parallel, then all sequential].
  // Each section is independently reorderable. Dropping a step into the OTHER
  // section flips its phase and moves it to the matching block, keeping the
  // sorted invariant. `posInSection` is the desired final index within the
  // section's filtered list (0 = top, sectionLength = bottom).

  const finishDrop = (targetPhase: Phase, posInSection: number) => {
    if (!config || !dragId) return;
    const fromIdx = config.steps.findIndex((s) => s.id === dragId);
    if (fromIdx < 0) return;

    const parallelCount = config.steps.filter((s) => s.phase === "parallel").length;
    // Convert the section-local position to an absolute array index.
    // For sequential, account for the parallel block sitting above.
    let absoluteTarget =
      targetPhase === "parallel" ? posInSection : parallelCount + posInSection;

    // The dragged step's removal shifts everything after `fromIdx` left by one.
    // Adjust the destination so we land at the position the user pointed at.
    if (fromIdx < absoluteTarget) absoluteTarget -= 1;

    const next = reorderSteps(config.steps, fromIdx, absoluteTarget, targetPhase);
    setConfig({ ...config, steps: next });
    setDirty(true);
    setDragId(null);
    setDropHint(null);
  };

  const cancelDrag = () => {
    setDragId(null);
    setDropHint(null);
  };

  const activeProfileName = profiles.find((p) => p.id === activeProfile)?.name ?? activeProfile;

  // Group steps for display: find boundaries between parallel and sequential
  const parallelSteps = config.steps.filter((s) => s.phase === "parallel");
  const sequentialSteps = config.steps.filter((s) => s.phase === "sequential");
  const hasMultiAgent = parallelSteps.some((s) => s.agents?.length > 1);

  return (
    <div className="flex h-full relative">
      {/* Prompt dialog */}
      {promptDialog && (
        <PromptDialog
          title={promptDialog.title}
          defaultValue={promptDialog.defaultValue}
          onSubmit={(value) => { promptDialog.onSubmit(value); setPromptDialog(null); }}
          onCancel={() => setPromptDialog(null)}
        />
      )}

      {/* Left panel — list */}
      <div
        className="border-r border-gray-200 dark:border-gray-700 flex flex-col bg-white dark:bg-gray-900 relative shrink-0"
        style={{ width: panelWidth }}
      >
        {/* Header */}
        <div className="px-4 pt-4 pb-2 flex items-center justify-between">
          <h2 className="text-lg font-bold text-gray-900 dark:text-gray-100">Workflow Editor</h2>
          <button onClick={() => {
            if (dirty && !confirm("You have unsaved changes. Leave and discard them?")) return;
            onClose();
          }} className="text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200">Back</button>
        </div>

        {/* Profile selector */}
        <div className="px-4 pb-3 space-y-2">
          <div className="flex items-center gap-2">
            <select
              value={activeProfile}
              onChange={(e) => handleSwitchProfile(e.target.value)}
              className="flex-1 py-1.5 px-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                         focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors"
            >
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
            <button
              onClick={handleNewProfile}
              className="p-1.5 text-gray-500 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors"
              title="New profile"
            >
              <svg className="w-4 h-4" viewBox="0 0 20 20" fill="currentColor">
                <path d="M10.75 4.75a.75.75 0 00-1.5 0v4.5h-4.5a.75.75 0 000 1.5h4.5v4.5a.75.75 0 001.5 0v-4.5h4.5a.75.75 0 000-1.5h-4.5v-4.5z" />
              </svg>
            </button>
          </div>
          <div className="flex gap-1">
            <button onClick={handleDuplicateProfile}
              className="flex-1 py-1 text-[11px] text-gray-500 hover:text-gray-700 hover:bg-gray-50 rounded transition-colors">
              Duplicate
            </button>
            <button onClick={handleRenameProfile}
              className="flex-1 py-1 text-[11px] text-gray-500 hover:text-gray-700 hover:bg-gray-50 rounded transition-colors">
              Rename
            </button>
            <button onClick={handleDeleteProfile}
              disabled={BUILTIN_PROFILES.includes(activeProfile)}
              className="flex-1 py-1 text-[11px] text-gray-500 hover:text-red-600 hover:bg-red-50
                         rounded transition-colors disabled:opacity-30 disabled:hover:text-gray-500
                         disabled:hover:bg-transparent">
              Delete
            </button>
          </div>
        </div>

        {/* Steps list */}
        <div className="flex-1 overflow-y-auto">
          {/* Wave diagram */}
          <WaveDiagram
            steps={config.steps}
            merge={config.merge}
            useOrientation={config.use_orientation}
            selectedId={editing}
            onSelect={(id) => setEditing(editing === id ? null : id)}
          />

          {/* Parallel section — header always visible so users know where to drop */}
          <div className="px-4 py-1.5 bg-blue-50 dark:bg-blue-950/30 sticky top-0 z-[1]">
            <span className="text-[10px] uppercase tracking-wider text-blue-500 dark:text-blue-400 font-medium">
              Parallel
            </span>
          </div>

          <DropZone
            section="parallel"
            posInSection={0}
            empty={parallelSteps.length === 0}
            active={dropHint?.phase === "parallel" && dropHint?.idx === 0}
            dragId={dragId}
            onEnter={() => setDropHint({ idx: 0, phase: "parallel" })}
            onDrop={() => finishDrop("parallel", 0)}
          />
          {parallelSteps.map((step, posInSection) => (
            <div key={step.id}>
              <StepRow
                step={step}
                selected={editing === step.id}
                isDragging={dragId === step.id}
                onToggle={() => updateStep(step.id, { enabled: !step.enabled })}
                onSelect={() => setEditing(editing === step.id ? null : step.id)}
                onDelete={() => removeStep(step.id)}
                onDragStart={() => setDragId(step.id)}
                onDragEnd={cancelDrag}
              />
              <DropZone
                section="parallel"
                posInSection={posInSection + 1}
                active={
                  dropHint?.phase === "parallel" &&
                  dropHint?.idx === posInSection + 1
                }
                dragId={dragId}
                onEnter={() => setDropHint({ idx: posInSection + 1, phase: "parallel" })}
                onDrop={() => finishDrop("parallel", posInSection + 1)}
              />
            </div>
          ))}

          {/* Merge indicator */}
          {hasMultiAgent && config.merge?.enabled && (
            <button
              onClick={() => setEditing("merge")}
              className={`w-full px-4 py-1.5 text-left ${
                editing === "merge"
                  ? "bg-amber-50 dark:bg-amber-950/30"
                  : "bg-gray-50 dark:bg-gray-800/50 hover:bg-gray-100 dark:hover:bg-gray-800"
              }`}
            >
              <span className="text-[10px] uppercase tracking-wider text-amber-600 dark:text-amber-400 font-medium">
                Merge (auto)
              </span>
            </button>
          )}

          {/* Sequential section */}
          <div className="px-4 py-1.5 bg-orange-50 dark:bg-orange-950/30 sticky top-0 z-[1]">
            <span className="text-[10px] uppercase tracking-wider text-orange-500 dark:text-orange-400 font-medium">
              Sequential
            </span>
          </div>

          <DropZone
            section="sequential"
            posInSection={0}
            empty={sequentialSteps.length === 0}
            active={dropHint?.phase === "sequential" && dropHint?.idx === 0}
            dragId={dragId}
            onEnter={() => setDropHint({ idx: 0, phase: "sequential" })}
            onDrop={() => finishDrop("sequential", 0)}
          />
          {sequentialSteps.map((step, posInSection) => (
            <div key={step.id}>
              <StepRow
                step={step}
                selected={editing === step.id}
                isDragging={dragId === step.id}
                onToggle={() => updateStep(step.id, { enabled: !step.enabled })}
                onSelect={() => setEditing(editing === step.id ? null : step.id)}
                onDelete={() => removeStep(step.id)}
                onDragStart={() => setDragId(step.id)}
                onDragEnd={cancelDrag}
              />
              <DropZone
                section="sequential"
                posInSection={posInSection + 1}
                active={
                  dropHint?.phase === "sequential" &&
                  dropHint?.idx === posInSection + 1
                }
                dragId={dragId}
                onEnter={() => setDropHint({ idx: posInSection + 1, phase: "sequential" })}
                onDrop={() => finishDrop("sequential", posInSection + 1)}
              />
            </div>
          ))}

          {/* Pipeline settings */}
          <button
            onClick={() => setEditing(editing === "pipeline_settings" ? null : "pipeline_settings")}
            className={`w-full px-4 py-2 text-left border-t border-gray-200 dark:border-gray-700 ${
              editing === "pipeline_settings"
                ? "bg-gray-100 dark:bg-gray-800"
                : "hover:bg-gray-50 dark:hover:bg-gray-800/50"
            }`}
          >
            <div className="flex items-center gap-2">
              <svg className="w-3.5 h-3.5 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M10.5 6h9.75M10.5 6a1.5 1.5 0 1 1-3 0m3 0a1.5 1.5 0 1 0-3 0M3.75 6H7.5m3 12h9.75m-9.75 0a1.5 1.5 0 0 1-3 0m3 0a1.5 1.5 0 0 0-3 0m-3.75 0H7.5m9-6h3.75m-3.75 0a1.5 1.5 0 0 1-3 0m3 0a1.5 1.5 0 0 0-3 0m-9.75 0h9.75" />
              </svg>
              <span className="text-xs font-medium text-gray-600 dark:text-gray-400">Pipeline Settings</span>
            </div>
          </button>
        </div>

        {/* Bottom actions */}
        <div className="p-3 border-t border-gray-200 dark:border-gray-700 space-y-2">
          <div className="flex gap-2">
            <button
              onClick={() => addStep("parallel")}
              className="flex-1 py-2 px-3 border border-dashed border-blue-300 rounded-lg
                         text-sm text-blue-600 hover:border-blue-400 hover:text-blue-700 transition-colors"
            >
              + Parallel
            </button>
            <button
              onClick={() => addStep("sequential")}
              className="flex-1 py-2 px-3 border border-dashed border-orange-300 rounded-lg
                         text-sm text-orange-600 hover:border-orange-400 hover:text-orange-700 transition-colors"
            >
              + Sequential
            </button>
          </div>
          <div className="flex gap-2">
            <button
              onClick={handleSave}
              disabled={saving || !dirty}
              className="flex-1 py-2 px-3 bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 rounded-lg text-sm font-medium
                         hover:bg-gray-800 dark:hover:bg-gray-200 disabled:bg-gray-300 dark:disabled:bg-gray-700 transition-colors"
            >
              {saving ? "Saving..." : "Save"}
            </button>
            <button
              onClick={handleReset}
              className="py-2 px-3 border border-gray-300 rounded-lg text-sm text-gray-600
                         hover:bg-gray-50 transition-colors"
            >
              Reset
            </button>
          </div>
          {/* Export/Import */}
          <div className="flex gap-2 relative">
            <div className="flex-1 relative">
              <button
                onClick={() => setExportMenuOpen(!exportMenuOpen)}
                className="w-full py-1.5 px-3 border border-gray-300 rounded-lg text-xs text-gray-500
                           hover:bg-gray-50 transition-colors"
              >
                Export {exportMenuOpen ? "\u25B2" : "\u25BC"}
              </button>
              {exportMenuOpen && (
                <div className="absolute bottom-full left-0 right-0 mb-1 bg-white border border-gray-200
                                rounded-lg shadow-lg overflow-hidden z-10">
                  <button
                    onClick={() => { handleExportItem(); setExportMenuOpen(false); }}
                    disabled={!editingStep}
                    className="w-full py-2 px-3 text-xs text-left text-gray-700 hover:bg-gray-50
                               disabled:text-gray-300 disabled:hover:bg-white transition-colors"
                  >
                    Export selected step
                  </button>
                  <button
                    onClick={() => { handleExportProfile(); setExportMenuOpen(false); }}
                    className="w-full py-2 px-3 text-xs text-left text-gray-700 hover:bg-gray-50 transition-colors"
                  >
                    Export profile &ldquo;{activeProfileName}&rdquo;
                  </button>
                  <button
                    onClick={() => { handleExportBundle(); setExportMenuOpen(false); }}
                    className="w-full py-2 px-3 text-xs text-left text-gray-700 hover:bg-gray-50 transition-colors"
                  >
                    Export all (profiles + settings)
                  </button>
                </div>
              )}
            </div>
            <button
              onClick={handleImport}
              className="flex-1 py-1.5 px-3 border border-gray-300 rounded-lg text-xs text-gray-500
                         hover:bg-gray-50 transition-colors"
            >
              Import
            </button>
          </div>
          {saved && <p className="text-xs text-green-600 text-center">Saved.</p>}
        </div>
        <ResizeHandle onResize={setPanelWidth} min={280} max={640} />
      </div>

      {/* Right panel — editor */}
      <div className="flex-1 flex flex-col min-h-0">
        {editing === "merge" ? (
          /* Merge prompt editor */
          <div className="flex-1 flex flex-col min-h-0">
            <div className="p-4 border-b border-gray-200 dark:border-gray-700 space-y-3">
              <div className="flex items-center gap-3">
                <button
                  onClick={() => updateMerge({ enabled: !config.merge.enabled })}
                  className={`w-8 h-5 rounded-full relative transition-colors shrink-0 ${
                    config.merge.enabled ? "bg-green-500" : "bg-gray-300 dark:bg-gray-600"
                  }`}
                >
                  <div className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${
                    config.merge.enabled ? "translate-x-3.5" : "translate-x-0.5"
                  }`} />
                </button>
                <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
                  Cross-agent merge {config.merge.enabled ? "enabled" : "disabled"}
                </span>
              </div>
              <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
                When a step runs on multiple LLM agents, this merges their independent outputs
                into a single report. Only runs when needed.
              </p>
              <AgentChips
                agents={config.merge.agents ?? []}
                onChange={(agents) => updateMerge({ agents })}
              />
            </div>
            <div className="flex-1 min-h-0 p-4">
              <PromptEditor
                value={config.merge.prompt}
                onChange={(prompt) => updateMerge({ prompt })}
                context={{ kind: "merge" }}
              />
            </div>
          </div>
        ) : editing === "extraction" ? (
          /* Text extraction editor */
          <ExtractionEditor
            extraction={config.extraction ?? DEFAULT_EXTRACTION}
            onChange={updateExtraction}
          />
        ) : editing === "orientation" ? (
          /* Orientation map editor */
          <OrientationEditor
            useOrientation={config.use_orientation}
            prompt={config.orientation_prompt}
            onToggleUse={(v) => { setConfig({ ...config, use_orientation: v }); setDirty(true); }}
            onPromptChange={(p) => { setConfig({ ...config, orientation_prompt: p }); setDirty(true); }}
          />
        ) : editing === "pipeline_settings" ? (
          /* Pipeline settings editor */
          <div className="flex-1 flex flex-col min-h-0 overflow-y-auto">
            <div className="p-4 space-y-5">
              <div>
                <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200 mb-1">Pipeline Settings</h3>
                <p className="text-xs text-gray-500 dark:text-gray-400">
                  Settings that apply to all steps in this profile.
                </p>
              </div>

              {/* Orientation map toggle */}
              <div className="flex items-start gap-3">
                <button
                  onClick={() => { setConfig({ ...config, use_orientation: !config.use_orientation }); setDirty(true); }}
                  className={`w-8 h-5 rounded-full relative transition-colors shrink-0 mt-0.5 ${
                    config.use_orientation ? "bg-green-500" : "bg-gray-300 dark:bg-gray-600"
                  }`}
                >
                  <div className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${
                    config.use_orientation ? "translate-x-3.5" : "translate-x-0.5"
                  }`} />
                </button>
                <div>
                  <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
                    Build orientation map
                  </span>
                  <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                    One LLM call surveys the input into structured JSON before any step runs.
                    Costs one call but keeps steps grounded in what the input actually
                    contains — e.g. it stops a review step criticizing something covered
                    elsewhere in the document.
                  </p>
                </div>
              </div>

              {/* Parallel context template */}
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
                    Parallel step context template
                  </label>
                  <div className="flex items-center gap-3">
                    <button
                      onClick={() => {
                        invoke<string>("get_default_prompt", { name: "parallel_context_generic" }).then((t) => {
                          setConfig({ ...config, parallel_context_template: t });
                          setDirty(true);
                        }).catch(console.error);
                      }}
                      className="text-[10px] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
                      title="Neutral wrapper for any input: survey + instructions + input path."
                    >
                      Reset to generic
                    </button>
                    <button
                      onClick={() => {
                        invoke<string>("get_default_parallel_template").then((t) => {
                          setConfig({ ...config, parallel_context_template: t });
                          setDirty(true);
                        }).catch(console.error);
                      }}
                      className="text-[10px] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
                      title="Referee briefing for academic papers: paper type, figure hints, issue-focused framing."
                    >
                      Reset to paper review
                    </button>
                  </div>
                </div>
                <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">
                  Wraps each parallel step's prompt. Controls what context the LLM receives.
                </p>
                <PromptEditor
                  value={config.parallel_context_template}
                  onChange={(parallel_context_template) => {
                    setConfig({ ...config, parallel_context_template });
                    setDirty(true);
                  }}
                  context={{ kind: "parallel_template" }}
                  rows={16}
                  fillHeight={false}
                />
              </div>
            </div>
          </div>
        ) : editingStep ? (
          <>
            <div className="p-4 border-b border-gray-200 dark:border-gray-700 space-y-3">
              <div>
                <label className="block text-xs font-medium text-gray-500 mb-1">Label</label>
                <input
                  type="text"
                  value={editingStep.label}
                  onChange={(e) => updateStep(editingStep.id, { label: e.target.value })}
                  className="w-full py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 transition-colors"
                />
              </div>
              <div>
                <label className="block text-xs font-medium text-gray-500 mb-1.5">Phase</label>
                <div className="flex gap-2">
                  {(["parallel", "sequential"] as const).map((phase) => (
                    <button
                      key={phase}
                      onClick={() => updateStep(editingStep.id, { phase })}
                      className={`px-3 py-1 text-xs rounded-full border transition-colors ${
                        editingStep.phase === phase
                          ? phase === "parallel"
                            ? "bg-blue-500 text-white border-blue-500"
                            : "bg-orange-500 text-white border-orange-500"
                          : "bg-white dark:bg-gray-800 text-gray-500 border-gray-300 dark:border-gray-600 hover:border-gray-400"
                      }`}
                    >
                      {phase.charAt(0).toUpperCase() + phase.slice(1)}
                    </button>
                  ))}
                </div>
              </div>
              <div>
                <label className="block text-xs font-medium text-gray-500 mb-1.5">Tools</label>
                <div className="flex gap-2">
                  {["WebSearch"].map((tool) => {
                    const active = editingStep.tools?.includes(tool) ?? false;
                    return (
                      <label key={tool} className="flex items-center gap-1.5 text-xs text-gray-700 dark:text-gray-300 cursor-pointer">
                        <input
                          type="checkbox"
                          checked={active}
                          onChange={() => {
                            const tools = active
                              ? (editingStep.tools ?? []).filter((t) => t !== tool)
                              : [...(editingStep.tools ?? []), tool];
                            updateStep(editingStep.id, { tools });
                          }}
                          className="rounded"
                        />
                        {tool}
                      </label>
                    );
                  })}
                </div>
              </div>
              <AgentChips
                agents={editingStep.agents ?? []}
                multi={editingStep.phase === "parallel"}
                onChange={(agents) => updateStep(editingStep.id, { agents })}
              />
              <ModelOverrides
                step={editingStep}
                onChange={(patch) => updateStep(editingStep.id, patch)}
              />
            </div>
            <div className="flex-1 min-h-0 p-4">
              <PromptEditor
                value={editingStep.prompt}
                onChange={(prompt) => updateStep(editingStep.id, { prompt })}
                context={
                  editingStep.phase === "sequential"
                    ? {
                        kind: "sequential",
                        otherStepIds: config.steps
                          .filter((s) => s.id !== editingStep.id)
                          .map((s) => s.id),
                      }
                    : { kind: "parallel" }
                }
              />
            </div>
          </>
        ) : (
          <div className="flex items-center justify-center h-full text-gray-400">
            <div className="text-center">
              <p className="text-lg">Select a step to edit</p>
              <p className="text-sm mt-1">
                Click a step on the left to view and edit its prompt
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// --- Pre-processing editors ---
//
// These edit the Stage 0 steps (text extraction + orientation map) that run
// before any review step. Both are profile-scoped: each profile can decide
// to use a different extractor or a custom orientation prompt.

const EXTRACTION_METHODS: { value: string; label: string; hint: string }[] = [
  { value: "", label: "Inherit from global Settings", hint: "Use whatever PDF extractor is configured globally." },
  { value: "auto", label: "Auto", hint: "Try the global setting; same as inherit." },
  { value: "llm", label: "LLM", hint: "Have the active provider read the PDF and convert to Markdown." },
  { value: "marker", label: "Local engine: marker-pdf", hint: "Local extraction, no LLM cost. Install from Settings → Text Extraction." },
  { value: "pdftotext", label: "pdftotext (basic)", hint: "Fast, but equations are lost. Uses bundled poppler." },
];

function ExtractionEditor({
  extraction,
  onChange,
}: {
  extraction: ExtractionConfig;
  onChange: (patch: Partial<ExtractionConfig>) => void;
}) {
  const method = extraction.method ?? "";
  const hint = EXTRACTION_METHODS.find((m) => m.value === method)?.hint;
  const showMarker = method === "marker";
  const inputMode = extraction.input_mode || "document";

  return (
    <div className="flex-1 flex flex-col min-h-0 overflow-y-auto">
      <div className="p-4 space-y-5">
        <div>
          <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200 mb-1">Input & Text Extraction</h3>
          <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
            Stage 0a. What the workflow takes as input, and how text is pulled from it before
            any LLM call.
          </p>
        </div>

        <div>
          <label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1.5">
            Input mode (per profile)
          </label>
          <select
            value={inputMode}
            onChange={(e) => onChange({ input_mode: e.target.value })}
            className="w-full py-1.5 px-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                       text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                       focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors"
          >
            <option value="document">Document — a single PDF or LaTeX file</option>
            <option value="folder">Folder — inventory a directory; steps Read files on demand</option>
            <option value="none">None — run from the step prompts alone</option>
          </select>
          <p className="text-[11px] text-gray-500 dark:text-gray-400 mt-1.5 leading-relaxed">
            Selecting a folder in the main window uses folder mode automatically, whatever this
            is set to.
          </p>
        </div>

        {inputMode !== "document" ? (
          <div className="text-[11px] text-gray-400 dark:text-gray-500 leading-relaxed border-t border-gray-100 dark:border-gray-800 pt-3">
            Extraction settings below apply only to document inputs.
          </div>
        ) : null}

        <div>
          <label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1.5">
            Method (per profile)
          </label>
          <select
            value={method}
            onChange={(e) => onChange({ method: e.target.value })}
            className="w-full py-1.5 px-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                       text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                       focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors"
          >
            {EXTRACTION_METHODS.map((m) => (
              <option key={m.value} value={m.value}>{m.label}</option>
            ))}
          </select>
          {hint && (
            <p className="text-[11px] text-gray-500 dark:text-gray-400 mt-1.5 leading-relaxed">{hint}</p>
          )}
        </div>

        {showMarker && (
          <div className="space-y-3 pl-3 border-l-2 border-gray-200 dark:border-gray-700">
            <p className="text-[11px] text-gray-500 dark:text-gray-400">
              marker_single flags. Leave on "Inherit" to use the global Settings value.
            </p>
            <TristateRow
              label="Disable OCR"
              value={extraction.marker_disable_ocr}
              onChange={(v) => onChange({ marker_disable_ocr: v })}
              hint="Skip OCR pass — much faster on text-only PDFs, fails on scans."
            />
            <TristateRow
              label="Disable image extraction"
              value={extraction.marker_disable_images}
              onChange={(v) => onChange({ marker_disable_images: v })}
              hint="Don't pull figures out of the PDF. Faster, smaller output."
            />
          </div>
        )}

        <div className="text-[11px] text-gray-400 dark:text-gray-500 leading-relaxed border-t border-gray-100 dark:border-gray-800 pt-3">
          The cascade for PDFs is: chosen method → fallback to LLM extraction if the native
          extractor fails (when the active provider supports PDF reads). LaTeX inputs bypass this
          entirely.
        </div>
      </div>
    </div>
  );
}

function TristateRow({
  label,
  value,
  onChange,
  hint,
}: {
  label: string;
  value: boolean | null;
  onChange: (v: boolean | null) => void;
  hint?: string;
}) {
  // null = inherit; true/false = override. Render as a 3-state segmented control.
  const states: { v: boolean | null; label: string }[] = [
    { v: null, label: "Inherit" },
    { v: false, label: "Off" },
    { v: true, label: "On" },
  ];
  return (
    <div>
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-gray-700 dark:text-gray-300">{label}</span>
        <div className="inline-flex rounded-lg border border-gray-300 dark:border-gray-600 overflow-hidden">
          {states.map((s) => {
            const active = (s.v === null && value == null) || s.v === value;
            return (
              <button
                key={String(s.v)}
                onClick={() => onChange(s.v)}
                className={`px-2 py-0.5 text-[11px] transition-colors ${
                  active
                    ? "bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900"
                    : "bg-white dark:bg-gray-800 text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-gray-700"
                }`}
              >
                {s.label}
              </button>
            );
          })}
        </div>
      </div>
      {hint && <p className="text-[10px] text-gray-500 dark:text-gray-500 mt-1">{hint}</p>}
    </div>
  );
}

function OrientationEditor({
  useOrientation,
  prompt,
  onToggleUse,
  onPromptChange,
}: {
  useOrientation: boolean;
  prompt: string;
  onToggleUse: (v: boolean) => void;
  onPromptChange: (p: string) => void;
}) {
  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="p-4 border-b border-gray-200 dark:border-gray-700 space-y-3">
        <div>
          <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200 mb-1">Orientation Map</h3>
          <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
            Stage 0b. One LLM call that builds a structured JSON survey of the input before any
            step runs — for a paper: sections, theorems, tables, notation. Every parallel step
            receives it via {"{orientation}"}, which keeps steps grounded in what the input
            actually contains. The survey can use any JSON schema your prompt asks for.
          </p>
        </div>

        <div className="flex items-center gap-3">
          <button
            onClick={() => onToggleUse(!useOrientation)}
            className={`w-8 h-5 rounded-full relative transition-colors shrink-0 ${
              useOrientation ? "bg-green-500" : "bg-gray-300 dark:bg-gray-600"
            }`}
          >
            <div className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${
              useOrientation ? "translate-x-3.5" : "translate-x-0.5"
            }`} />
          </button>
          <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
            Orientation map {useOrientation ? "enabled" : "disabled"}
          </span>
        </div>
      </div>

      <div className="p-4 border-b border-gray-200 dark:border-gray-700 flex items-center justify-between">
        <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
          Prompt
        </label>
        <div className="flex items-center gap-3">
          <button
            onClick={() => {
              invoke<string>("get_default_prompt", { name: "orientation_generic" })
                .then(onPromptChange)
                .catch(console.error);
            }}
            className="text-[10px] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
            title="Insert the generic survey prompt (works for any input)."
          >
            Insert generic survey
          </button>
          <button
            onClick={() => {
              invoke<string>("get_default_prompt", { name: "orientation" })
                .then(onPromptChange)
                .catch(console.error);
            }}
            className="text-[10px] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
            title="Insert the paper-review survey prompt (sections, theorems, tables, notation)."
          >
            Insert paper survey
          </button>
          <button
            onClick={() => onPromptChange("")}
            disabled={prompt.trim() === ""}
            className="text-[10px] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300
                       disabled:opacity-40 disabled:hover:text-gray-400 transition-colors"
            title="Clear the override; the default template will be used."
          >
            Use default
          </button>
        </div>
      </div>
      <div className="flex-1 min-h-0 p-4 flex flex-col">
        <div className="flex-1 min-h-0">
          <PromptEditor
            value={prompt}
            onChange={onPromptChange}
            context={{ kind: "orientation" }}
          />
        </div>
        {prompt.trim() === "" && (
          <p className="text-[11px] text-gray-500 dark:text-gray-400 mt-2 leading-relaxed shrink-0">
            Empty — using the bundled default at <span className="font-mono">prompts/orientation.md</span>{" "}
            (overridable at <span className="font-mono">~/.pipeline/prompts/orientation.md</span>).
          </p>
        )}
      </div>
    </div>
  );
}

// --- Step row ---

function StepRow({
  step,
  selected,
  isDragging,
  onToggle,
  onSelect,
  onDelete,
  onDragStart,
  onDragEnd,
}: {
  step: StepConfig;
  selected: boolean;
  isDragging: boolean;
  onToggle: () => void;
  onSelect: () => void;
  onDelete?: () => void;
  onDragStart: () => void;
  onDragEnd: () => void;
}) {
  const badges: string[] = [];
  if (step.tools?.includes("WebSearch")) badges.push("web search");
  if (step.agents?.length) {
    badges.push(...step.agents.map((a) => a.charAt(0).toUpperCase() + a.slice(1)));
  }
  if (step.model) badges.push(`model: ${step.model}`);
  if (step.effort) badges.push(`effort: ${step.effort}`);

  return (
    <div
      draggable
      onDragStart={(e) => {
        // Drag transfer is purely advisory — actual data lives in component state.
        // Setting it lets cross-window drag previews work in Tauri's WebView.
        e.dataTransfer.effectAllowed = "move";
        try { e.dataTransfer.setData("text/plain", step.id); } catch { /* webview may forbid */ }
        onDragStart();
      }}
      onDragEnd={onDragEnd}
      className={`border-b border-gray-100 dark:border-gray-800 transition-opacity ${
        selected ? "bg-blue-50 dark:bg-blue-950" : "hover:bg-gray-50 dark:hover:bg-gray-800"
      } ${isDragging ? "opacity-40" : ""}`}
    >
      <div className="flex items-center gap-2 px-2 py-3">
        {/* Drag handle */}
        <span
          className="px-1 text-gray-300 dark:text-gray-600 cursor-grab active:cursor-grabbing select-none"
          aria-label="Drag to reorder"
          title="Drag to reorder"
        >
          <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
            <circle cx="7" cy="5" r="1.3" />
            <circle cx="13" cy="5" r="1.3" />
            <circle cx="7" cy="10" r="1.3" />
            <circle cx="13" cy="10" r="1.3" />
            <circle cx="7" cy="15" r="1.3" />
            <circle cx="13" cy="15" r="1.3" />
          </svg>
        </span>
        <button
          onClick={onToggle}
          className={`w-8 h-5 rounded-full relative transition-colors shrink-0 ${
            step.enabled ? "bg-green-500" : "bg-gray-300 dark:bg-gray-600"
          }`}
        >
          <div className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${
            step.enabled ? "translate-x-3.5" : "translate-x-0.5"
          }`} />
        </button>
        <button onClick={onSelect} className="flex-1 text-left text-sm font-medium text-gray-800 dark:text-gray-200 truncate">
          {step.label}
        </button>
        <button onClick={onDelete} className="p-1 text-gray-400 hover:text-red-500 shrink-0" title="Remove">
          <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
            <path fillRule="evenodd" d="M8.75 1A2.75 2.75 0 006 3.75v.443c-.795.077-1.584.176-2.365.298a.75.75 0 10.23 1.482l.149-.022.841 10.518A2.75 2.75 0 007.596 19h4.807a2.75 2.75 0 002.742-2.53l.841-10.519.149.023a.75.75 0 00.23-1.482A41.03 41.03 0 0014 4.193V3.75A2.75 2.75 0 0011.25 1h-2.5zM10 4c.84 0 1.673.025 2.5.075V3.75c0-.69-.56-1.25-1.25-1.25h-2.5c-.69 0-1.25.56-1.25 1.25v.325C8.327 4.025 9.16 4 10 4zM8.58 7.72a.75.75 0 00-1.5.06l.3 7.5a.75.75 0 101.5-.06l-.3-7.5zm4.34.06a.75.75 0 10-1.5-.06l-.3 7.5a.75.75 0 101.5.06l.3-7.5z" />
          </svg>
        </button>
      </div>
      {badges.length > 0 && (
        <div className="flex gap-1.5 px-4 pb-2 flex-wrap">
          {badges.map((b) => (
            <span key={b} className={`text-[10px] px-1.5 py-0.5 rounded ${
              b === "web search" ? "bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300" :
              ["Claude", "Codex", "Gemini"].includes(b) ? "bg-purple-100 text-purple-700 dark:bg-purple-900 dark:text-purple-300" :
              b.startsWith("model:") || b.startsWith("effort:") ? "bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300 font-mono" :
              "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300"
            }`}>{b}</span>
          ))}
        </div>
      )}
    </div>
  );
}

// --- Drop zone ---
//
// Thin horizontal target between rows. When a drag is in progress and the user
// hovers, we set dropHint state in the parent so the active zone visually
// expands and shows a phase indicator. Native HTML5 dragenter/dragover are
// both used: dragenter sets the hint, dragover (with preventDefault) is what
// permits the drop event to fire.

function DropZone({
  section,
  posInSection,
  active,
  empty,
  dragId,
  onEnter,
  onDrop,
}: {
  section: Phase;
  posInSection: number;
  active: boolean;
  /** When true, the section is empty — render a taller invitation zone. */
  empty?: boolean;
  dragId: string | null;
  onEnter: () => void;
  onDrop: () => void;
}) {
  // When nothing is being dragged, drop zones are invisible and inert; this
  // keeps the row stack tight in the common case.
  const dragging = dragId !== null;

  if (!dragging && !empty) {
    return <div className="h-0.5" data-drop-pos={posInSection} data-drop-section={section} />;
  }

  const tone = section === "parallel" ? "border-blue-400" : "border-orange-400";
  const tint = section === "parallel" ? "bg-blue-100 dark:bg-blue-900/40" : "bg-orange-100 dark:bg-orange-900/40";

  return (
    <div
      onDragEnter={(e) => { e.preventDefault(); onEnter(); }}
      onDragOver={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = "move"; onEnter(); }}
      onDrop={(e) => { e.preventDefault(); onDrop(); }}
      className={`transition-all ${
        active
          ? `h-6 my-0.5 mx-2 rounded ${tint} border-2 border-dashed ${tone}`
          : empty
            ? `h-10 my-1 mx-2 rounded border border-dashed ${tone} text-[10px] flex items-center justify-center text-gray-400 dark:text-gray-500`
            : "h-2"
      }`}
    >
      {empty && !active && `Drop here to add to ${section === "parallel" ? "Parallel" : "Sequential"}`}
    </div>
  );
}

// --- Model / effort overrides ---
//
// Per-step overrides that take precedence over the global Settings values for
// the active provider. Empty string = inherit. Showing the inputs collapsed by
// default keeps the editor uncluttered for the common case where users don't
// override anything.

function ModelOverrides({
  step,
  onChange,
}: {
  step: StepConfig;
  onChange: (patch: Partial<StepConfig>) => void;
}) {
  const hasOverride = !!(step.model || step.effort);
  const [open, setOpen] = useState(hasOverride);

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
          <span className="text-[10px] text-gray-400 dark:text-gray-500 font-mono ml-1">
            {[step.model, step.effort].filter(Boolean).join(" / ")}
          </span>
        )}
      </button>
      {open && (
        <div className="mt-2 space-y-2 pl-3 border-l-2 border-gray-200 dark:border-gray-700">
          <p className="text-[10px] text-gray-400 dark:text-gray-500 leading-relaxed">
            Leave blank to use the global setting for this step's provider.
            Useful for spending more reasoning budget on a hard step or downgrading
            a cheap one.
          </p>
          <div>
            <label className="block text-[10px] font-medium text-gray-500 mb-0.5">
              Model
            </label>
            <input
              type="text"
              value={step.model ?? ""}
              onChange={(e) => onChange({ model: e.target.value })}
              placeholder="(inherit) e.g. opus, sonnet, haiku, gpt-5.1, gemini-2.5-pro"
              className="w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs font-mono
                         text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                         focus:outline-none focus:ring-1 focus:ring-gray-400 focus:border-transparent"
            />
          </div>
          <div>
            <label className="block text-[10px] font-medium text-gray-500 mb-0.5">
              Effort
            </label>
            <input
              type="text"
              value={step.effort ?? ""}
              onChange={(e) => onChange({ effort: e.target.value })}
              placeholder="(inherit) e.g. low, medium, high, max"
              className="w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs font-mono
                         text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                         focus:outline-none focus:ring-1 focus:ring-gray-400 focus:border-transparent"
            />
          </div>
        </div>
      )}
    </div>
  );
}

// --- Agent selector ---

const PROVIDERS = ["claude", "codex", "gemini", "local"] as const;

function AgentChips({
  agents,
  multi,
  onChange,
}: {
  agents: string[];
  multi?: boolean;
  onChange: (agents: string[]) => void;
}) {
  return (
    <div>
      <label className="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1.5">
        LLM Agents
        <span className="font-normal text-gray-400 dark:text-gray-500 ml-1">
          (empty = global setting{multi ? "; select multiple to run in parallel" : ""})
        </span>
      </label>
      <div className="flex gap-2">
        {PROVIDERS.map((provider) => {
          const active = agents.includes(provider);
          return (
            <button
              key={provider}
              onClick={() => {
                if (multi) {
                  onChange(active ? agents.filter((a) => a !== provider) : [...agents, provider]);
                } else {
                  onChange(active ? [] : [provider]);
                }
              }}
              className={`px-3 py-1 text-xs rounded-full border transition-colors ${
                active
                  ? "bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 border-gray-900 dark:border-gray-100"
                  : "bg-white dark:bg-gray-800 text-gray-500 dark:text-gray-400 border-gray-300 dark:border-gray-600 hover:border-gray-400 dark:hover:border-gray-500"
              }`}
            >
              {provider.charAt(0).toUpperCase() + provider.slice(1)}
            </button>
          );
        })}
      </div>
    </div>
  );
}

// --- Prompt dialog ---

function PromptDialog({
  title, defaultValue, onSubmit, onCancel,
}: {
  title: string;
  defaultValue: string;
  onSubmit: (value: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(defaultValue);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl w-80 p-5">
        <p className="text-sm font-medium text-gray-900 dark:text-gray-100 mb-3">{title}</p>
        <input
          type="text"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && value.trim()) onSubmit(value.trim());
            if (e.key === "Escape") onCancel();
          }}
          autoFocus
          className="w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                     text-gray-900 bg-white dark:bg-gray-700 dark:text-gray-200
                     focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors"
        />
        <div className="flex justify-end gap-2 mt-4">
          <button
            onClick={onCancel}
            className="py-1.5 px-3 text-sm text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={() => { if (value.trim()) onSubmit(value.trim()); }}
            disabled={!value.trim()}
            className="py-1.5 px-3 text-sm font-medium text-white bg-gray-900 dark:bg-gray-100 dark:text-gray-900 rounded-lg
                       hover:bg-gray-800 dark:hover:bg-gray-200 disabled:bg-gray-300 dark:disabled:bg-gray-600 transition-colors"
          >
            OK
          </button>
        </div>
      </div>
    </div>
  );
}

function ResizeHandle({
  onResize,
  min,
  max,
}: {
  onResize: (width: number) => void;
  min: number;
  max: number;
}) {
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    return () => {
      // Clean up global listeners if component unmounts during a drag
      if (cleanupRef.current) cleanupRef.current();
    };
  }, []);

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const parent = (e.target as HTMLElement).parentElement;
      if (!parent) return;
      const startX = e.clientX;
      const startWidth = parent.getBoundingClientRect().width;

      const onMouseMove = (ev: MouseEvent) => {
        const newWidth = Math.min(max, Math.max(min, startWidth + ev.clientX - startX));
        onResize(newWidth);
      };
      const onMouseUp = () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        cleanupRef.current = null;
      };
      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
      cleanupRef.current = onMouseUp;
    },
    [onResize, min, max]
  );

  return (
    <div
      onMouseDown={handleMouseDown}
      className="absolute top-0 right-0 w-1.5 h-full cursor-col-resize
                 hover:bg-gray-300 active:bg-gray-400 transition-colors z-10"
    />
  );
}

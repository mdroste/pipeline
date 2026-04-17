import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
import type { PipelineConfig, StepConfig, MergeConfig, ProfileSummary, ExportEnvelope } from "../lib/types";

interface Props {
  onClose: () => void;
  onProfileChange?: () => void;
}

const DEFAULT_MERGE: MergeConfig = { enabled: true, prompt: "", agents: [] };

export default function PipelinePage({ onClose, onProfileChange }: Props) {
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [editing, setEditing] = useState<string | "merge" | "pipeline_settings" | null>(null);

  // Profile state
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [activeProfile, setActiveProfile] = useState<string>("deep-review");
  const [exportMenuOpen, setExportMenuOpen] = useState(false);

  // Prompt dialog state
  const [promptDialog, setPromptDialog] = useState<{
    title: string;
    defaultValue: string;
    onSubmit: (value: string) => void;
  } | null>(null);
  const [panelWidth, setPanelWidth] = useState(320);

  useEffect(() => {
    Promise.all([
      invoke<PipelineConfig>("get_pipeline_config"),
      invoke<ProfileSummary[]>("list_profiles"),
      invoke<string>("get_active_profile"),
    ])
      .then(([c, p, a]) => {
        setConfig({ ...c, merge: c.merge ?? DEFAULT_MERGE });
        setProfiles(p);
        setActiveProfile(a);
        setLoading(false);
      })
      .catch((e) => {
        console.error(e);
        setLoading(false);
      });
  }, []);

  const refreshProfiles = async () => {
    try {
      const p = await invoke<ProfileSummary[]>("list_profiles");
      setProfiles(p);
    } catch (e) {
      console.error(e);
    }
  };

  const editingStep = (config && editing && editing !== "merge" && editing !== "pipeline_settings")
    ? config.steps.find((s) => s.id === editing) ?? null
    : null;

  useEffect(() => {
    if (editing && editing !== "merge" && editing !== "pipeline_settings" && !editingStep) {
      setEditing(null);
    }
  }, [editing, editingStep]);

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
      setConfig({ ...newConfig, merge: newConfig.merge ?? DEFAULT_MERGE });
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
    if (["deep-review", "quick-review"].includes(activeProfile)) {
      alert("Cannot delete a built-in profile.");
      return;
    }
    const current = profiles.find((p) => p.id === activeProfile);
    if (!confirm(`Delete profile "${current?.name ?? activeProfile}"? This cannot be undone.`)) return;
    try {
      await invoke("delete_profile", { id: activeProfile });
      await refreshProfiles();
      const newConfig = await invoke<PipelineConfig>("switch_profile", { id: "deep-review" });
      setConfig({ ...newConfig, merge: newConfig.merge ?? DEFAULT_MERGE });
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
      setConfig({ ...d, merge: d.merge ?? DEFAULT_MERGE }); setEditing(null); setDirty(false);
      await refreshProfiles();
      onProfileChange?.();
    } catch (e) { alert(`Failed to reset: ${e instanceof Error ? e.message : String(e)}`); }
  };

  // --- Export/Import ---

  const handleExportItem = async () => {
    if (!editing || editing === "merge" || editing === "pipeline_settings" || !editingStep) return;
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
          setConfig({ ...c, merge: c.merge ?? DEFAULT_MERGE });
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

  const moveStep = (idx: number, dir: -1 | 1) => {
    const next = [...config.steps];
    [next[idx], next[idx + dir]] = [next[idx + dir], next[idx]];
    setConfig({ ...config, steps: next }); setDirty(true);
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
          <h2 className="text-lg font-bold text-gray-900 dark:text-gray-100">Pipeline</h2>
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
              disabled={["deep-review", "quick-review"].includes(activeProfile)}
              className="flex-1 py-1 text-[11px] text-gray-500 hover:text-red-600 hover:bg-red-50
                         rounded transition-colors disabled:opacity-30 disabled:hover:text-gray-500
                         disabled:hover:bg-transparent">
              Delete
            </button>
          </div>
        </div>

        {/* Steps list */}
        <div className="flex-1 overflow-y-auto">
          {/* Parallel section */}
          {parallelSteps.length > 0 && (
            <div className="px-4 py-1.5 bg-blue-50 dark:bg-blue-950/30">
              <span className="text-[10px] uppercase tracking-wider text-blue-500 dark:text-blue-400 font-medium">
                Parallel
              </span>
            </div>
          )}
          {config.steps.map((step, idx) => {
            if (step.phase !== "parallel") return null;
            return (
              <StepRow
                key={step.id}
                step={step}
                selected={editing === step.id}
                onToggle={() => updateStep(step.id, { enabled: !step.enabled })}
                onSelect={() => setEditing(editing === step.id ? null : step.id)}
                onMoveUp={idx > 0 && config.steps[idx - 1]?.phase === "parallel" ? () => moveStep(idx, -1) : undefined}
                onMoveDown={idx < config.steps.length - 1 && config.steps[idx + 1]?.phase === "parallel" ? () => moveStep(idx, 1) : undefined}
                onDelete={() => removeStep(step.id)}
              />
            );
          })}

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
          {sequentialSteps.length > 0 && (
            <div className="px-4 py-1.5 bg-orange-50 dark:bg-orange-950/30">
              <span className="text-[10px] uppercase tracking-wider text-orange-500 dark:text-orange-400 font-medium">
                Sequential
              </span>
            </div>
          )}
          {config.steps.map((step, idx) => {
            if (step.phase !== "sequential") return null;
            return (
              <StepRow
                key={step.id}
                step={step}
                selected={editing === step.id}
                onToggle={() => updateStep(step.id, { enabled: !step.enabled })}
                onSelect={() => setEditing(editing === step.id ? null : step.id)}
                onMoveUp={idx > 0 && config.steps[idx - 1]?.phase === "sequential" ? () => moveStep(idx, -1) : undefined}
                onMoveDown={idx < config.steps.length - 1 && config.steps[idx + 1]?.phase === "sequential" ? () => moveStep(idx, 1) : undefined}
                onDelete={() => removeStep(step.id)}
              />
            );
          })}

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
                    disabled={!editing || editing === "merge" || editing === "pipeline_settings"}
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
        <ResizeHandle onResize={setPanelWidth} min={240} max={480} />
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
              <p className="text-[10px] text-gray-400 dark:text-gray-500">
                Placeholders:{" "}
                <code className="bg-gray-100 dark:bg-gray-700 px-1 rounded">{"{topic}"}</code>{" "}
                <code className="bg-gray-100 dark:bg-gray-700 px-1 rounded">{"{agent_reports}"}</code>
              </p>
            </div>
            <div className="flex-1 min-h-0 p-4">
              <textarea
                value={config.merge.prompt}
                onChange={(e) => updateMerge({ prompt: e.target.value })}
                spellCheck={false}
                className="w-full h-full resize-none font-mono text-xs leading-relaxed
                           p-4 border border-gray-300 dark:border-gray-600 rounded-lg bg-gray-50 dark:bg-gray-800 dark:text-gray-200
                           focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent"
              />
            </div>
          </div>
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
                    LLM builds a structured inventory of the paper (sections, theorems, tables, notation)
                    before any step runs. Costs one LLM call but prevents referees from criticizing content
                    that exists elsewhere in the paper.
                  </p>
                </div>
              </div>

              {/* Parallel context template */}
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
                    Parallel step context template
                  </label>
                  <button
                    onClick={() => {
                      invoke<string>("get_default_parallel_template").then((t) => {
                        setConfig({ ...config, parallel_context_template: t });
                        setDirty(true);
                      });
                    }}
                    className="text-[10px] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
                  >
                    Reset to default
                  </button>
                </div>
                <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">
                  Wraps each parallel step's prompt. Controls what context the LLM receives.
                </p>
                <p className="text-[10px] text-gray-400 dark:text-gray-500 mb-2">
                  Placeholders:{" "}
                  <code className="bg-gray-100 dark:bg-gray-700 px-1 rounded">{"{step_prompt}"}</code>{" "}
                  <code className="bg-gray-100 dark:bg-gray-700 px-1 rounded">{"{paper_type}"}</code>{" "}
                  <code className="bg-gray-100 dark:bg-gray-700 px-1 rounded">{"{orientation}"}</code>{" "}
                  <code className="bg-gray-100 dark:bg-gray-700 px-1 rounded">{"{paper_path}"}</code>{" "}
                  <code className="bg-gray-100 dark:bg-gray-700 px-1 rounded">{"{figure_hint}"}</code>
                </p>
                <textarea
                  value={config.parallel_context_template}
                  onChange={(e) => { setConfig({ ...config, parallel_context_template: e.target.value }); setDirty(true); }}
                  spellCheck={false}
                  rows={16}
                  className="w-full resize-none font-mono text-xs leading-relaxed
                             p-3 border border-gray-300 dark:border-gray-600 rounded-lg bg-gray-50 dark:bg-gray-800 dark:text-gray-200
                             focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent"
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
              {editingStep.phase === "sequential" && (
                <p className="text-[10px] text-gray-400 dark:text-gray-500">
                  Placeholders:{" "}
                  <code className="bg-gray-100 dark:bg-gray-700 px-1 rounded">{"{orientation}"}</code>{" "}
                  <code className="bg-gray-100 dark:bg-gray-700 px-1 rounded">{"{prior_outputs}"}</code>{" "}
                  <code className="bg-gray-100 dark:bg-gray-700 px-1 rounded">{"{last_output}"}</code>{" "}
                  <code className="bg-gray-100 dark:bg-gray-700 px-1 rounded">{"{paper_path}"}</code>
                </p>
              )}
            </div>
            <div className="flex-1 min-h-0 p-4">
              <textarea
                value={editingStep.prompt}
                onChange={(e) => updateStep(editingStep.id, { prompt: e.target.value })}
                spellCheck={false}
                className="w-full h-full resize-none font-mono text-xs leading-relaxed
                           p-4 border border-gray-300 dark:border-gray-600 rounded-lg bg-gray-50 dark:bg-gray-800 dark:text-gray-200
                           focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent"
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

// --- Step row ---

function StepRow({
  step, selected, onToggle, onSelect, onMoveUp, onMoveDown, onDelete,
}: {
  step: StepConfig;
  selected: boolean;
  onToggle: () => void;
  onSelect: () => void;
  onMoveUp?: () => void;
  onMoveDown?: () => void;
  onDelete?: () => void;
}) {
  const badges: string[] = [];
  if (step.tools?.includes("WebSearch")) badges.push("web search");
  if (step.agents?.length) {
    badges.push(...step.agents.map((a) => a.charAt(0).toUpperCase() + a.slice(1)));
  }

  return (
    <div className={`border-b border-gray-100 dark:border-gray-800 ${selected ? "bg-blue-50 dark:bg-blue-950" : "hover:bg-gray-50 dark:hover:bg-gray-800"}`}>
      <div className="flex items-center gap-2 px-4 py-3">
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
        <div className="flex items-center gap-0.5 shrink-0">
          {onMoveUp && (
            <button onClick={onMoveUp} className="p-1 text-gray-400 hover:text-gray-600" title="Move up">
              <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
                <path fillRule="evenodd" d="M14.77 12.79a.75.75 0 01-1.06-.02L10 8.832 6.29 12.77a.75.75 0 11-1.08-1.04l4.25-4.5a.75.75 0 011.08 0l4.25 4.5a.75.75 0 01-.02 1.06z" />
              </svg>
            </button>
          )}
          {onMoveDown && (
            <button onClick={onMoveDown} className="p-1 text-gray-400 hover:text-gray-600" title="Move down">
              <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
                <path fillRule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" />
              </svg>
            </button>
          )}
          <button onClick={onDelete} className="p-1 text-gray-400 hover:text-red-500" title="Remove">
            <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
              <path fillRule="evenodd" d="M8.75 1A2.75 2.75 0 006 3.75v.443c-.795.077-1.584.176-2.365.298a.75.75 0 10.23 1.482l.149-.022.841 10.518A2.75 2.75 0 007.596 19h4.807a2.75 2.75 0 002.742-2.53l.841-10.519.149.023a.75.75 0 00.23-1.482A41.03 41.03 0 0014 4.193V3.75A2.75 2.75 0 0011.25 1h-2.5zM10 4c.84 0 1.673.025 2.5.075V3.75c0-.69-.56-1.25-1.25-1.25h-2.5c-.69 0-1.25.56-1.25 1.25v.325C8.327 4.025 9.16 4 10 4zM8.58 7.72a.75.75 0 00-1.5.06l.3 7.5a.75.75 0 101.5-.06l-.3-7.5zm4.34.06a.75.75 0 10-1.5-.06l-.3 7.5a.75.75 0 101.5.06l.3-7.5z" />
            </svg>
          </button>
        </div>
      </div>
      {badges.length > 0 && (
        <div className="flex gap-1.5 px-4 pb-2">
          {badges.map((b) => (
            <span key={b} className={`text-[10px] px-1.5 py-0.5 rounded ${
              b === "web search" ? "bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300" :
              ["Claude", "Codex", "Gemini"].includes(b) ? "bg-purple-100 text-purple-700 dark:bg-purple-900 dark:text-purple-300" :
              "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300"
            }`}>{b}</span>
          ))}
        </div>
      )}
    </div>
  );
}

// --- Agent selector ---

const PROVIDERS = ["claude", "codex", "gemini"] as const;

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

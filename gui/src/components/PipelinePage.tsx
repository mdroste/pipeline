import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  Phase,
  PipelineConfig,
  StepConfig,
  MergeConfig,
  ExtractionConfig,
  ProfileSummary,
  ExportEnvelope,
  Settings,
  ModelCatalog,
  ArtifactSelector,
} from "../lib/types";
import { PROVIDERS } from "../lib/providers";
import WaveDiagram, { type WaveSelection } from "./WaveDiagram";
import AutoReviewCatalogDialog from "./AutoReviewCatalogDialog";
import ResizeHandle from "./ResizeHandle";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";
import PromptDialog from "./pipeline-editor/PromptDialog";
import AddStepDialog, { type AddStepDraft } from "./pipeline-editor/AddStepDialog";
import {
  MergeEditorPanel,
  PipelineSettingsEditorPanel,
  StepEditorPanel,
} from "./pipeline-editor/EditorPanels";
import {
  MemoizedExtractionEditor as ExtractionEditor,
  MemoizedOrientationEditor as OrientationEditor,
} from "./pipeline-editor/PreprocessingEditors";
import { MemoizedStepRow as StepRow } from "./pipeline-editor/StepListItems";
import {
  AdaptiveAgentsEditorPanel,
  AdaptiveAgentsRow,
  type AdaptiveSlotKind,
} from "./pipeline-editor/AdaptiveReviewEditors";
import {
  adaptiveAgentCount as getAdaptiveAgentCount,
  adaptiveAgentRange as getAdaptiveAgentRange,
  isAutoReview,
  withAdaptiveAgentCount,
} from "../lib/autoReview";
import {
  conditionUpstreamIds,
  defaultStepContext,
  normalizeConfig,
  primaryPartsForMode,
} from "./pipeline-editor/utils";
import { describeStep } from "./pipeline-editor/stepSummary";
import { ISSUES_SCHEMA } from "./pipeline-editor/stepTemplates";

interface Props {
  onClose: () => void;
  onDirtyChange?: (dirty: boolean) => void;
  onOpenGallery?: () => void;
  onProfileChange?: () => void;
  showBack?: boolean;
}

const DEFAULT_EXTRACTION: ExtractionConfig = {
  method: "",
};

type EditingMode = WaveSelection | null;

export default function PipelinePage({
  onClose,
  onDirtyChange,
  onOpenGallery,
  onProfileChange,
  showBack = true,
}: Props) {
  // Editor state is profile-scoped; switching profiles replaces the active draft.
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [editing, setEditing] = useState<EditingMode>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [catalogs, setCatalogs] = useState<Record<string, ModelCatalog>>({});

  // Profile state
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [activeProfile, setActiveProfile] = useState<string>("auto-review");
  const [profileMutationPending, setProfileMutationPending] = useState(false);
  const [exportMenuOpen, setExportMenuOpen] = useState(false);
  const [navigatorView, setNavigatorView] = useState<"steps" | "overview">("steps");
  const [catalogTab, setCatalogTab] = useState<"subjects" | "methods" | null>(null);
  const [undoRewrite, setUndoRewrite] = useState<{
    before: PipelineConfig;
    after: string;
    dirtyBefore: boolean;
    editingBefore: EditingMode;
    message: string;
  } | null>(null);
  const configRef = useRef<PipelineConfig | null>(config);
  const activeProfileRef = useRef(activeProfile);
  const profileMutationRequestRef = useRef(0);
  const profileMutationActiveRef = useRef(false);
  const promptResetRequestRef = useRef(0);
  configRef.current = config;
  activeProfileRef.current = activeProfile;

  // Prompt dialog state
  const [promptDialog, setPromptDialog] = useState<{
    title: string;
    defaultValue: string;
    onSubmit: (value: string) => void;
  } | null>(null);
  const [addStepOpen, setAddStepOpen] = useState(false);
  const [panelWidth, setPanelWidth] = usePersistentPanelWidth(
    "pipeline.ui.workflowPanelWidth",
    420,
    280,
    640,
  );

  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);

  useEffect(() => () => onDirtyChange?.(false), [onDirtyChange]);

  useEffect(
    () => () => {
      profileMutationRequestRef.current += 1;
      profileMutationActiveRef.current = false;
    },
    [],
  );

  useEffect(() => {
    let live = true;
    setLoading(true);
    setLoadError(null);
    Promise.all([
      invoke<PipelineConfig>("get_pipeline_config"),
      invoke<ProfileSummary[]>("list_profiles"),
      invoke<string>("get_active_profile"),
    ])
      .then(([c, p, a]) => {
        if (!live) return;
        setConfig(normalizeConfig(c));
        setProfiles(p);
        setActiveProfile(a);
        setLoading(false);
      })
      .catch((e) => {
        console.error(e);
        if (live) {
          setConfig(null);
          setLoadError(e instanceof Error ? e.message : String(e));
          setLoading(false);
        }
      });
    return () => {
      live = false;
    };
  }, [loadAttempt]);

  useEffect(() => {
    let live = true;
    invoke<{ settings: Settings; warnings: string[] }>("get_settings")
      .then((response) => {
        if (!live) return;
        setSettings(response.settings);
        for (const provider of PROVIDERS) {
          invoke<ModelCatalog>("get_model_catalog", {
            provider,
            settings: response.settings,
            refresh: false,
          }).then((catalog) => {
            if (live) setCatalogs((old) => ({ ...old, [provider]: catalog }));
          }).catch(() => {});
        }
      })
      .catch(() => {
        // Model overrides remain usable with legacy fields against an older
        // backend; catalog loading is an enhancement, not an editor blocker.
      });
    return () => {
      live = false;
    };
  }, []);

  const profileMutationIsCurrent = (request: number) =>
    request === profileMutationRequestRef.current;

  const beginProfileMutation = (): number | null => {
    if (profileMutationActiveRef.current) return null;
    profileMutationActiveRef.current = true;
    const request = ++profileMutationRequestRef.current;
    setProfileMutationPending(true);
    return request;
  };

  const finishProfileMutation = (request: number) => {
    if (!profileMutationIsCurrent(request)) return;
    profileMutationActiveRef.current = false;
    setProfileMutationPending(false);
  };

  const refreshProfiles = async (request?: number) => {
    try {
      const p = await invoke<ProfileSummary[]>("list_profiles");
      if (request !== undefined && !profileMutationIsCurrent(request)) return;
      setProfiles(p);
    } catch (e) {
      if (request === undefined || profileMutationIsCurrent(request)) {
        console.error(e);
      }
    }
  };

  const switchProfileForMutation = async (id: string, request: number) => {
    const newConfig = await invoke<PipelineConfig>("switch_profile", { id });
    if (!profileMutationIsCurrent(request)) return false;
    setConfig(normalizeConfig(newConfig));
    setActiveProfile(id);
    setEditing(null);
    setDirty(false);
    onProfileChange?.();
    return true;
  };

  // IDs that aren't tied to a specific step's prompt; everything else is treated
  // as a step ID and looked up in config.steps.
  const NON_STEP_EDITORS = new Set([
    "merge",
    "pipeline_settings",
    "extraction",
    "orientation",
    "auto_adaptive_agents",
  ]);
  const isStepEditing = !!editing && !NON_STEP_EDITORS.has(editing);

  const editingStep = (config && isStepEditing)
    ? config.steps.find((s) => s.id === editing) ?? null
    : null;
  const conditionStepIds = config && editingStep
    ? conditionUpstreamIds(config.steps, editingStep.id)
    : [];

  useEffect(() => {
    if (isStepEditing && !editingStep) {
      setEditing(null);
    }
  }, [isStepEditing, editingStep]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-gray-400">
        <div className="animate-spin w-6 h-6 border-2 border-gray-300 border-t-gray-600 rounded-full" />
      </div>
    );
  }
  if (loadError || !config) {
    return (
      <div className="flex items-center justify-center h-full p-8">
        <div className="max-w-md text-center space-y-4">
          <h2 className="text-base font-semibold text-gray-800 dark:text-gray-100">
            Pipeline editor could not be loaded
          </h2>
          <p className="text-sm text-red-600 dark:text-red-400 break-words">
            {loadError ?? "The profile configuration was unavailable."}
          </p>
          <div className="flex justify-center gap-2">
            <button
              type="button"
              onClick={() => setLoadAttempt((attempt) => attempt + 1)}
              className="px-3 py-1.5 rounded bg-blue-600 text-white text-sm hover:bg-blue-700"
            >
              Retry
            </button>
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-1.5 rounded border border-gray-300 dark:border-gray-700 text-sm"
            >
              Close
            </button>
          </div>
        </div>
      </div>
    );
  }

  // --- Profile management ---

  const handleSwitchProfile = async (id: string) => {
    if (id === activeProfileRef.current) return;
    if (dirty && !confirm("You have unsaved changes. Switch profile and discard them?")) return;
    const request = beginProfileMutation();
    if (request === null) return;
    try {
      await switchProfileForMutation(id, request);
    } catch (e) {
      if (profileMutationIsCurrent(request)) {
        alert(`Failed to switch profile: ${e instanceof Error ? e.message : String(e)}`);
      }
    } finally {
      finishProfileMutation(request);
    }
  };

  const handleNewProfile = () => {
    setPromptDialog({
      title: "New profile name",
      defaultValue: "",
      onSubmit: async (name) => {
        if (dirty && !confirm("Create this profile and discard the current unsaved changes?")) {
          return;
        }
        const request = beginProfileMutation();
        if (request === null) return;
        try {
          const summary = await invoke<ProfileSummary>("create_profile", { name });
          if (!profileMutationIsCurrent(request)) return;
          await refreshProfiles(request);
          await switchProfileForMutation(summary.id, request);
        } catch (e) {
          if (profileMutationIsCurrent(request)) {
            alert(`Failed to create profile: ${e instanceof Error ? e.message : String(e)}`);
          }
        } finally {
          finishProfileMutation(request);
        }
      },
    });
  };

  const handleDuplicateProfile = () => {
    setPromptDialog({
      title: "Name for the duplicate",
      defaultValue: "",
      onSubmit: async (name) => {
        if (dirty && !confirm("Duplicate this profile and discard the current unsaved changes?")) {
          return;
        }
        const request = beginProfileMutation();
        if (request === null) return;
        const sourceId = activeProfileRef.current;
        try {
          const summary = await invoke<ProfileSummary>("duplicate_profile", {
            sourceId,
            newName: name,
          });
          if (!profileMutationIsCurrent(request)) return;
          await refreshProfiles(request);
          await switchProfileForMutation(summary.id, request);
        } catch (e) {
          if (profileMutationIsCurrent(request)) {
            alert(`Failed to duplicate profile: ${e instanceof Error ? e.message : String(e)}`);
          }
        } finally {
          finishProfileMutation(request);
        }
      },
    });
  };

  const handleRenameProfile = () => {
    const current = profiles.find((p) => p.id === activeProfile);
    const profile = activeProfile;
    setPromptDialog({
      title: "Rename profile",
      defaultValue: current?.name ?? "",
      onSubmit: async (name) => {
        const request = beginProfileMutation();
        if (request === null) return;
        try {
          await invoke<ProfileSummary>("rename_profile", { id: profile, newName: name });
          if (
            !profileMutationIsCurrent(request) ||
            activeProfileRef.current !== profile
          ) return;
          await refreshProfiles(request);
        } catch (e) {
          if (profileMutationIsCurrent(request)) {
            alert(`Failed to rename profile: ${e instanceof Error ? e.message : String(e)}`);
          }
        } finally {
          finishProfileMutation(request);
        }
      },
    });
  };

  const handleDeleteProfile = async () => {
    const profile = activeProfileRef.current;
    if (profiles.find((candidate) => candidate.id === profile)?.builtin) {
      alert("Cannot delete a built-in profile.");
      return;
    }
    const current = profiles.find((candidate) => candidate.id === profile);
    if (!confirm(`Delete profile "${current?.name ?? profile}"? This cannot be undone.`)) return;
    const request = beginProfileMutation();
    if (request === null) return;
    try {
      await invoke("delete_profile", { id: profile });
      if (!profileMutationIsCurrent(request)) return;
      await refreshProfiles(request);
      await switchProfileForMutation("auto-review", request);
    } catch (e) {
      if (profileMutationIsCurrent(request)) {
        alert(`Failed to delete profile: ${e instanceof Error ? e.message : String(e)}`);
      }
    } finally {
      finishProfileMutation(request);
    }
  };

  // --- Config editing ---

  const updateStep = (id: string, patch: Partial<StepConfig>) => {
    setConfig((current) => current ? {
      ...current,
      steps: current.steps.map((step) => step.id === id ? { ...step, ...patch } : step),
    } : current);
    setDirty(true);
  };

  const updateStepPhase = (id: string, phase: Phase) => {
    setConfig((current) => current ? {
      ...current,
      steps: current.steps.map((step) => step.id === id ? { ...step, phase } : step),
    } : current);
    setDirty(true);
  };

  const updateMerge = (patch: Partial<MergeConfig>) => {
    setConfig((current) => current ? {
      ...current,
      merge: { ...current.merge, ...patch },
    } : current);
    setDirty(true);
  };

  const updateExtraction = (patch: Partial<ExtractionConfig>) => {
    const previous = config.extraction ?? DEFAULT_EXTRACTION;
    const extraction = { ...previous, ...patch };
    const previousInputs = previous.extra_inputs ?? [];
    const nextInputs = extraction.extra_inputs ?? [];
    const renamedInputs = new Map<string, string>();
    if (previousInputs.length === nextInputs.length) {
      previousInputs.forEach((slot, index) => {
        const nextKey = nextInputs[index]?.key;
        if (nextKey !== undefined && slot.key !== nextKey) {
          renamedInputs.set(slot.key, nextKey);
        }
      });
    }
    const namedKeys = new Set((extraction.extra_inputs ?? []).map((slot) => slot.key));
    let removedSelectors = 0;
    const steps = config.steps.map((step) => {
      const include = step.context.include
        .map((selector): ArtifactSelector => {
          if (selector.kind === "named_input" && renamedInputs.has(selector.key)) {
            return { ...selector, key: renamedInputs.get(selector.key)! };
          }
          return selector;
        })
        .filter((selector) => {
          const remove =
            (selector.kind === "primary" && extraction.input_mode === "none") ||
            (selector.kind === "named_input" && !namedKeys.has(selector.key));
          if (remove) removedSelectors += 1;
          return !remove;
        });
      return { ...step, context: { include } };
    });
    if (
      removedSelectors > 0 &&
      !window.confirm(
        `This change removes ${removedSelectors} artifact access rule${removedSelectors === 1 ? "" : "s"} from workflow steps. Continue?`,
      )
    ) {
      return;
    }
    const nextConfig = { ...config, extraction, steps };
    if (removedSelectors > 0) {
      setUndoRewrite({
        before: config,
        after: JSON.stringify(nextConfig),
        dirtyBefore: dirty,
        editingBefore: editing,
        message: "Artifact access rules were updated.",
      });
    }
    setConfig(nextConfig);
    setDirty(true);
  };

  const updateStepEnabled = (id: string, enabled: boolean) => {
    const affected = enabled
      ? []
      : config.steps.filter((step) =>
          (step.after ?? []).includes(id) ||
          step.context.include.some(
            (selector) => selector.kind === "step" && selector.step === id,
          ) ||
          (step.run_if?.kind === "output_matches" && step.run_if.step === id),
        );
    if (
      affected.length > 0 &&
      !window.confirm(
        `Disabling this step removes dependencies or artifact access from ${affected.length} downstream step${affected.length === 1 ? "" : "s"}. Continue?`,
      )
    ) {
      return;
    }
    const nextConfig = {
      ...config,
      steps: config.steps.map((step) => {
        if (step.id === id) return { ...step, enabled };
        if (enabled) return step;
        return {
          ...step,
          after: (step.after ?? []).filter((dependency) => dependency !== id),
          context: {
            include: step.context.include.filter(
              (selector) => selector.kind !== "step" || selector.step !== id,
            ),
          },
          run_if: step.run_if?.kind === "output_matches" && step.run_if.step === id
            ? null
            : step.run_if,
        };
      }),
    };
    if (affected.length > 0) {
      setUndoRewrite({
        before: config,
        after: JSON.stringify(nextConfig),
        dirtyBefore: dirty,
        editingBefore: editing,
        message: "Downstream connections were removed.",
      });
    }
    setConfig(nextConfig);
    setDirty(true);
  };

  const persistCurrentConfig = async (): Promise<boolean> => {
    const configToSave = config;
    const profileToSave = activeProfile;
    const savedSnapshot = JSON.stringify(configToSave);
    setSaving(true); setSaved(false);
    try {
      await invoke("save_pipeline_config", {
        config: configToSave,
        profileId: profileToSave,
      });
      const isCurrentVersion =
        activeProfileRef.current === profileToSave &&
        JSON.stringify(configRef.current) === savedSnapshot;
      setSaved(isCurrentVersion);
      if (isCurrentVersion) setDirty(false);
      await refreshProfiles();
      onProfileChange?.();
      if (isCurrentVersion) setTimeout(() => setSaved(false), 2000);
      return isCurrentVersion;
    } catch (e) {
      alert(`Failed to save: ${e instanceof Error ? e.message : String(e)}`);
      return false;
    }
    finally { setSaving(false); }
  };

  const handleSave = async () => {
    await persistCurrentConfig();
  };

  const saveBeforeExport = async (what: string): Promise<boolean> => {
    if (!dirty) return true;
    if (!window.confirm(
      `Exporting ${what} requires saving this profile's unsaved edits first. Save and continue?`,
    )) {
      return false;
    }
    const currentSaved = await persistCurrentConfig();
    if (!currentSaved) {
      alert("Export cancelled because the workflow changed while it was being saved.");
    }
    return currentSaved;
  };

  const resetParallelTemplate = async (
    source: "generic" | "paper",
  ) => {
    const request = ++promptResetRequestRef.current;
    const profile = activeProfileRef.current;
    try {
      const template = source === "generic"
        ? await invoke<string>("get_default_prompt", { name: "parallel_context_generic" })
        : await invoke<string>("get_default_parallel_template");
      if (
        request !== promptResetRequestRef.current ||
        activeProfileRef.current !== profile
      ) return;
      setConfig((current) =>
        current ? { ...current, parallel_context_template: template } : current,
      );
      setDirty(true);
    } catch (error) {
      if (
        request === promptResetRequestRef.current &&
        activeProfileRef.current === profile
      ) {
        alert(
          `Failed to reset the parallel context template: ${
            error instanceof Error ? error.message : String(error)
          }`,
        );
      }
    }
  };

  const handleReset = async () => {
    if (!confirm("Reset this profile to defaults? All customizations will be lost.")) return;
    const request = beginProfileMutation();
    if (request === null) return;
    const profile = activeProfileRef.current;
    try {
      const d = await invoke<PipelineConfig>("reset_pipeline_config");
      if (
        !profileMutationIsCurrent(request) ||
        activeProfileRef.current !== profile
      ) return;
      setConfig(normalizeConfig(d)); setEditing(null); setDirty(false);
      await refreshProfiles(request);
      if (
        !profileMutationIsCurrent(request) ||
        activeProfileRef.current !== profile
      ) return;
      onProfileChange?.();
    } catch (e) {
      if (profileMutationIsCurrent(request)) {
        alert(`Failed to reset: ${e instanceof Error ? e.message : String(e)}`);
      }
    } finally {
      finishProfileMutation(request);
    }
  };

  // --- Export/Import ---

  // The backend opens the native save dialog and writes only to the chosen
  // path; the webview passes a suggested name but never a filesystem path.
  const handleExportItem = async () => {
    if (!editingStep) return;
    const envelope: ExportEnvelope = { type: "step", data: editingStep };
    const suggestedName = `pipeline-step-${editingStep.id}.json`;
    try {
      await invoke("export_item", { json: JSON.stringify(envelope, null, 2), suggestedName });
    } catch (e) {
      alert(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleExportProfile = async () => {
    const suggestedName = `pipeline-profile-${activeProfile}.json`;
    try {
      if (!(await saveBeforeExport("the profile"))) return;
      await invoke("export_profile", { id: activeProfile, suggestedName });
    } catch (e) {
      alert(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleExportBundle = async () => {
    try {
      if (!(await saveBeforeExport("the settings bundle"))) return;
      await invoke("export_bundle", { suggestedName: "pipeline-settings-backup.json" });
    } catch (e) {
      alert(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleImport = async () => {
    try {
      const path = await openDialog({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      const envelope = await invoke<ExportEnvelope>("import_item", { path });
      switch (envelope.type) {
        case "step": {
          const imported = envelope.data as StepConfig;
          const step: StepConfig = {
            ...imported,
            after: imported.after ?? [],
            context: imported.context ?? { include: [] },
          };
          const agents = step.agents?.length || 1;
          const fanOut = step.for_each ? `; fan-out up to ${step.for_each.max} items` : "";
          const logicalCalls = (step.for_each?.max ?? 1) * agents;
          const attempts = logicalCalls * ((settings?.max_retries ?? 0) + 1);
          if (!confirm(
            `Import step “${step.label}”?\n\n` +
            `Tools: ${step.tools?.join(", ") || "none"}\n` +
            `Agents: ${step.agents?.join(", ") || "profile default"}${fanOut}\n` +
            `Maximum provider attempts from this step: ${attempts}`,
          )) return;
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
          const enabled = envelope.steps.filter((step) => step.enabled);
          const tools = [...new Set(enabled.flatMap((step) => step.tools ?? []))];
          const agents = [...new Set(enabled.flatMap((step) => step.agents ?? []))];
          const logicalCalls = enabled.reduce((total, step) => {
            const agentCount = Math.max(1, step.agents?.length ?? 0);
            return total + agentCount * (step.for_each?.max ?? 1);
          }, 0);
          const mergeCalls = envelope.merge?.enabled
            ? enabled.reduce((total, step) => {
                const agentCount = Math.max(1, step.agents?.length ?? 0);
                return total + (agentCount > 1 ? (step.for_each?.max ?? 1) : 0);
              }, 0)
            : 0;
          const maxAttempts = logicalCalls * ((settings?.max_retries ?? 0) + 1) + mergeCalls;
          if (!confirm(
            `Import and activate profile “${envelope.name}”?\n\n` +
            `${enabled.length} enabled steps; up to ${maxAttempts} provider attempts per report (including retries and merges).\n` +
            `Tools: ${tools.join(", ") || "none"}\n` +
            `Agents: ${agents.join(", ") || "profile default"}\n\n` +
            "Review the imported prompts in the editor before generating a report.",
          )) return;
          const summary = await invoke<ProfileSummary>("import_profile", { path });
          await refreshProfiles();
          await handleSwitchProfile(summary.id);
          break;
        }
        case "bundle": {
          if (!confirm(
            `Import ${envelope.profiles.length} profiles plus provider/settings configuration?\n\n` +
            "Existing profiles with the same ID will be overwritten. API-key fields and the active profile may change. Review the active profile before running it.",
          )) return;
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

  const addStep = async (draft: AddStepDraft) => {
    const inputMode = config.extraction?.input_mode || "document";
    let template: StepConfig | null = null;
    if (draft.mode === "adaptive") {
      if (!draft.templateId) throw new Error("Select an adaptive agent to copy.");
      template = await invoke<StepConfig>("get_auto_review_specialist_step", {
        id: draft.templateId,
      });
    }

    const baseId = template
      ? `manual_${template.id}`
      : draft.label
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "_")
        .replace(/^_+|_+$/g, "") || "custom_step";
    let id = baseId;
    let suffix = 2;
    while (config.steps.some((step) => step.id === id)) {
      id = `${baseId}_${suffix++}`;
    }

    const priorReports: ArtifactSelector[] = config.steps
      .filter((step) => step.enabled)
      .map((step) => ({ kind: "step", step: step.id, parts: ["report"] }));
    const primaryParts = primaryPartsForMode(inputMode);
    const context = (() => {
      if (draft.mode === "adaptive" || draft.mode === "blank" || draft.contextPreset === "standard") {
        return defaultStepContext(
          draft.phase,
          inputMode,
          true,
          config.steps,
        );
      }
      if (draft.contextPreset === "input") {
        return {
          include: primaryParts.length
            ? [{ kind: "primary" as const, parts: primaryParts }]
            : [],
        };
      }
      if (draft.contextPreset === "prior_reports") {
        return { include: priorReports };
      }
      return { include: [] };
    })();

    const prompt = (() => {
      if (template) return template.prompt;
      if (draft.mode === "blank") {
        return draft.phase === "parallel"
          ? `# ${draft.label}\n\nDescribe what this step should evaluate.\n\n## Method\n\n...\n\n## Output structure\n\n...`
          : `You are performing a custom analysis step.\n\nORIENTATION MAP:\n{orientation}\n\nPRIOR OUTPUTS:\n{prior_outputs}\n\nLAST OUTPUT:\n{last_output}\n\n[Your instructions here]`;
      }
      const contextBlocks: string[] = [];
      if (context.include.some((selector) => selector.kind === "survey")) {
        contextBlocks.push("ORIENTATION MAP:\n{orientation}");
      }
      if (context.include.some(
        (selector) => selector.kind === "step" && selector.parts.includes("report"),
      )) {
        contextBlocks.push("PRIOR OUTPUTS:\n{prior_outputs}");
      }
      const outputInstruction = draft.output === "issues"
        ? "Return JSON matching the configured issues schema."
        : "Return a clear, self-contained markdown report.";
      return [
        `# ${draft.label}`,
        ...contextBlocks,
        `## Task\n\n${draft.instructions}`,
        `## Output\n\n${outputInstruction}`,
      ].join("\n\n");
    })();

    const step: StepConfig = template
      ? {
          ...template,
          id,
          enabled: true,
          after: [...(template.after ?? [])],
          context,
        }
      : {
          id,
          label: draft.label,
          enabled: true,
          phase: draft.phase,
          tools: [],
          agents: [],
          prompt,
          after: [],
          context,
          output_schema: draft.mode === "guided" && draft.output === "issues"
            ? ISSUES_SCHEMA
            : null,
        };
    const insertionIndex = step.phase === "parallel"
      ? config.steps.findIndex((candidate) => candidate.phase === "sequential")
      : -1;
    const steps = config.steps.slice();
    steps.splice(insertionIndex < 0 ? steps.length : insertionIndex, 0, step);
    setConfig({ ...config, steps });
    setAddStepOpen(false);
    setNavigatorView("steps");
    setEditing(id);
    setDirty(true);
  };

  const removeStep = (id: string) => {
    const step = config.steps.find((candidate) => candidate.id === id);
    const affected = config.steps.filter((candidate) =>
      (candidate.after ?? []).includes(id) ||
      candidate.context.include.some(
        (selector) => selector.kind === "step" && selector.step === id,
      ) ||
      (candidate.run_if?.kind === "output_matches" && candidate.run_if.step === id),
    );
    const consequence = affected.length > 0
      ? ` It will also remove references from ${affected.length} downstream step${affected.length === 1 ? "" : "s"}.`
      : "";
    if (!window.confirm(`Remove “${step?.label ?? id}”?${consequence}`)) return;
    const nextConfig = {
      ...config,
      steps: config.steps
        .filter((step) => step.id !== id)
        .map((step) => ({
          ...step,
          after: (step.after ?? []).filter((dependency) => dependency !== id),
          context: {
            include: step.context.include.filter(
              (selector) => selector.kind !== "step" || selector.step !== id,
            ),
          },
          run_if: step.run_if?.kind === "output_matches" && step.run_if.step === id
            ? null
            : step.run_if,
        })),
    };
    setUndoRewrite({
      before: config,
      after: JSON.stringify(nextConfig),
      dirtyBefore: dirty,
      editingBefore: editing,
      message: `“${step?.label ?? id}” and its connections were removed.`,
    });
    setConfig(nextConfig);
    if (editing === id) setEditing(null);
    setDirty(true);
  };

  const moveSequentialStep = (id: string, direction: -1 | 1) => {
    const step = config.steps.find((candidate) => candidate.id === id);
    if (!step || step.phase !== "sequential") return;
    const peers = config.steps.filter((candidate) => candidate.phase === "sequential");
    const position = peers.findIndex((candidate) => candidate.id === id);
    const target = peers[position + direction];
    if (!target) return;
    const fromIndex = config.steps.findIndex((candidate) => candidate.id === id);
    const toIndex = config.steps.findIndex((candidate) => candidate.id === target.id);
    const steps = config.steps.slice();
    [steps[fromIndex], steps[toIndex]] = [steps[toIndex], steps[fromIndex]];
    setConfig({ ...config, steps });
    setDirty(true);
  };

  const activeProfileName = profiles.find((p) => p.id === activeProfile)?.name ?? activeProfile;

  // Group steps for display: find boundaries between parallel and sequential
  const parallelSteps = config.steps.filter((s) => s.phase === "parallel");
  const sequentialSteps = config.steps.filter((s) => s.phase === "sequential");
  const hasMultiAgent = parallelSteps.some((s) => s.agents?.length > 1);
  const autoReview = isAutoReview(config);
  const adaptiveAgentCount = getAdaptiveAgentCount(config);
  const adaptiveAgentRange = getAdaptiveAgentRange(config);
  const browseSpecialists = (kind: AdaptiveSlotKind) => {
    setCatalogTab(kind === "subject" ? "subjects" : "methods");
  };

  return (
    <div className="flex h-full relative">
      {undoRewrite && JSON.stringify(config) === undoRewrite.after && (
        <div
          role="status"
          className="fixed bottom-5 left-1/2 z-40 flex -translate-x-1/2 items-center gap-3 rounded-lg bg-gray-900 px-4 py-2 text-xs text-white shadow-xl dark:bg-gray-100 dark:text-gray-900"
        >
          <span>{undoRewrite.message}</span>
          <button
            type="button"
            onClick={() => {
              setConfig(undoRewrite.before);
              setDirty(undoRewrite.dirtyBefore);
              setEditing(undoRewrite.editingBefore);
              setUndoRewrite(null);
            }}
            className="font-semibold underline underline-offset-2"
          >
            Undo
          </button>
        </div>
      )}
      {/* Prompt dialog */}
      {promptDialog && (
        <PromptDialog
          title={promptDialog.title}
          defaultValue={promptDialog.defaultValue}
          onSubmit={(value) => { promptDialog.onSubmit(value); setPromptDialog(null); }}
          onCancel={() => setPromptDialog(null)}
        />
      )}
      {addStepOpen && (
        <AddStepDialog
          onCreate={addStep}
          onCancel={() => setAddStepOpen(false)}
        />
      )}
      {catalogTab && (
        <AutoReviewCatalogDialog
          initialTab={catalogTab}
          onClose={() => setCatalogTab(null)}
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
          {showBack && (
            <button onClick={() => {
              if (dirty && !confirm("You have unsaved changes. Leave and discard them?")) return;
              onClose();
            }} className="text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200">Back</button>
          )}
        </div>

        {/* Profile selector */}
        <div className="px-4 pb-3 space-y-2">
          <div className="flex items-center gap-2">
            <select
              aria-label="Active workflow profile"
              aria-busy={profileMutationPending}
              value={activeProfile}
              onChange={(e) => handleSwitchProfile(e.target.value)}
              disabled={profileMutationPending}
              className="flex-1 py-1.5 px-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                         focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors
                         disabled:cursor-wait disabled:opacity-60"
            >
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
            <button
              onClick={handleNewProfile}
              aria-label="New profile"
              disabled={profileMutationPending}
              className="p-1.5 text-gray-500 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors disabled:opacity-40"
              title="New profile"
            >
              <svg className="w-4 h-4" viewBox="0 0 20 20" fill="currentColor">
                <path d="M10.75 4.75a.75.75 0 00-1.5 0v4.5h-4.5a.75.75 0 000 1.5h4.5v4.5a.75.75 0 001.5 0v-4.5h4.5a.75.75 0 000-1.5h-4.5v-4.5z" />
              </svg>
            </button>
          </div>
          <div className="flex gap-1">
            <button onClick={handleDuplicateProfile}
              disabled={profileMutationPending}
              className="flex-1 py-1 text-[11px] text-gray-500 hover:text-gray-700 hover:bg-gray-50 rounded transition-colors disabled:opacity-40">
              Duplicate
            </button>
            <button onClick={handleRenameProfile}
              disabled={profileMutationPending}
              className="flex-1 py-1 text-[11px] text-gray-500 hover:text-gray-700 hover:bg-gray-50 rounded transition-colors disabled:opacity-40">
              Rename
            </button>
            <button onClick={handleDeleteProfile}
              disabled={
                profileMutationPending ||
                profiles.find((profile) => profile.id === activeProfile)?.builtin
              }
              className="flex-1 py-1 text-[11px] text-gray-500 hover:text-red-600 hover:bg-red-50
                         rounded transition-colors disabled:opacity-30 disabled:hover:text-gray-500
                         disabled:hover:bg-transparent">
              Delete
            </button>
          </div>
        </div>

        <div className="px-4 pb-2">
          <div
            role="tablist"
            aria-label="Workflow navigator view"
            className="grid grid-cols-2 rounded-lg bg-gray-100 p-0.5 dark:bg-gray-800"
          >
            {(["steps", "overview"] as const).map((view) => (
              <button
                key={view}
                type="button"
                role="tab"
                aria-selected={navigatorView === view}
                onClick={() => setNavigatorView(view)}
                className={`rounded-md px-2 py-1.5 text-xs font-medium transition-colors ${
                  navigatorView === view
                    ? "bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-gray-100"
                    : "text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-200"
                }`}
              >
                {view === "steps" ? "Steps" : "Overview"}
              </button>
            ))}
          </div>
        </div>

        {/* Primary workflow navigator. The overview replaces rather than duplicates the list. */}
        <div className="flex-1 overflow-y-auto">
          {navigatorView === "overview" ? (
            <WaveDiagram
              steps={config.steps}
              merge={config.merge}
              adaptiveReview={autoReview}
              adaptiveAgentCount={adaptiveAgentCount}
              adaptiveAgentRange={adaptiveAgentRange}
              selectedId={editing}
              onSelect={(id) => setEditing(editing === id ? null : id)}
            />
          ) : (
            <>
          <div className="px-4 py-1.5 bg-gray-50 dark:bg-gray-800/50 sticky top-0 z-[1]">
            <span className="text-[10px] uppercase tracking-wider text-gray-500 dark:text-gray-400 font-medium">
              Workflow setup
            </span>
          </div>
          <button
            type="button"
            aria-label="Input & extraction"
            onClick={() => setEditing(editing === "extraction" ? null : "extraction")}
            className={`w-full border-b border-gray-100 px-4 py-2.5 text-left dark:border-gray-800 ${
              editing === "extraction"
                ? "bg-gray-100 dark:bg-gray-800"
                : "hover:bg-gray-50 dark:hover:bg-gray-800/50"
            }`}
          >
            <span className="block text-sm font-medium text-gray-800 dark:text-gray-200">Input &amp; extraction</span>
            <span className="mt-0.5 block text-[11px] text-gray-500 dark:text-gray-400">
              Choose the input type and PDF extraction method
            </span>
          </button>
          <button
            type="button"
            aria-label={autoReview ? "Orientation & classification" : "Orientation map"}
            onClick={() => setEditing(editing === "orientation" ? null : "orientation")}
            className={`w-full border-b border-gray-100 px-4 py-2.5 text-left dark:border-gray-800 ${
              editing === "orientation"
                ? "bg-gray-100 dark:bg-gray-800"
                : "hover:bg-gray-50 dark:hover:bg-gray-800/50"
            }`}
          >
            <span className="block text-sm font-medium text-gray-800 dark:text-gray-200">
              {autoReview ? "Orientation & classification" : "Orientation map"}
            </span>
            <span className="mt-0.5 block text-[11px] text-gray-500 dark:text-gray-400">
              {autoReview
                ? "Build the paper map and select its review plan in one LLM call"
                : "Survey the input before review steps run"}
            </span>
          </button>

          {/* Parallel section */}
          <div className="px-4 py-1.5 bg-blue-50 dark:bg-blue-950/30 sticky top-0 z-[1]">
            <span className="text-[10px] uppercase tracking-wider text-blue-700 dark:text-blue-300 font-medium">
              Parallel
            </span>
          </div>

          {parallelSteps.map((step) => (
            <div key={step.id}>
              <StepRow
                step={step}
                summary={describeStep(step, config).compact}
                selected={editing === step.id}
                onToggle={() => updateStepEnabled(step.id, !step.enabled)}
                onSelect={() => setEditing(editing === step.id ? null : step.id)}
                onDelete={() => removeStep(step.id)}
              />
            </div>
          ))}
          {autoReview && (
            <div className="border-t border-blue-100 dark:border-blue-950/60">
              <div className="bg-blue-50/60 px-4 py-1.5 text-[10px] font-semibold uppercase tracking-wider text-blue-700 dark:bg-blue-950/25 dark:text-blue-300">
                Auto-filled from orientation
              </div>
              <AdaptiveAgentsRow
                count={adaptiveAgentCount}
                range={adaptiveAgentRange}
                selected={editing === "auto_adaptive_agents"}
                onSelect={() => setEditing(editing === "auto_adaptive_agents" ? null : "auto_adaptive_agents")}
              />
            </div>
          )}

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
              <span className="text-[10px] uppercase tracking-wider text-amber-700 dark:text-amber-300 font-medium">
                Merge (auto)
              </span>
            </button>
          )}

          {/* Sequential section */}
          <div className="px-4 py-1.5 bg-orange-50 dark:bg-orange-950/30 sticky top-0 z-[1]">
            <span className="text-[10px] uppercase tracking-wider text-orange-700 dark:text-orange-300 font-medium">
              Sequential
            </span>
          </div>

          {sequentialSteps.map((step, posInSection) => (
            <div key={step.id}>
              <StepRow
                step={step}
                summary={describeStep(step, config).compact}
                selected={editing === step.id}
                onToggle={() => updateStepEnabled(step.id, !step.enabled)}
                onSelect={() => setEditing(editing === step.id ? null : step.id)}
                onDelete={() => removeStep(step.id)}
                canMoveUp={posInSection > 0}
                canMoveDown={posInSection < sequentialSteps.length - 1}
                onMoveUp={() => moveSequentialStep(step.id, -1)}
                onMoveDown={() => moveSequentialStep(step.id, 1)}
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
            </>
          )}
        </div>

        {/* Bottom actions */}
        <div className="p-3 border-t border-gray-200 dark:border-gray-700 space-y-2">
          <button
            type="button"
            onClick={() => setAddStepOpen(true)}
            className="w-full rounded-lg border border-dashed border-gray-400 px-3 py-2 text-sm font-medium text-gray-700 transition-colors hover:border-gray-600 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-300 dark:hover:border-gray-400 dark:hover:bg-gray-800"
          >
            + Add step
          </button>
          <div className="flex gap-2">
            <button
              onClick={handleSave}
              disabled={saving || profileMutationPending || !dirty}
              className="flex-1 py-2 px-3 bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 rounded-lg text-sm font-medium
                         hover:bg-gray-800 dark:hover:bg-gray-200 disabled:bg-gray-300 dark:disabled:bg-gray-700 transition-colors"
            >
              {saving ? "Saving..." : "Save"}
            </button>
            <button
              onClick={handleReset}
              disabled={profileMutationPending}
              className="py-2 px-3 border border-gray-300 rounded-lg text-sm text-gray-600
                         hover:bg-gray-50 transition-colors disabled:cursor-wait disabled:opacity-50"
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
            {onOpenGallery && (
              <button
                onClick={onOpenGallery}
                title="Browse curated workflow templates"
                className="flex-1 py-1.5 px-3 border border-gray-300 rounded-lg text-xs text-gray-500
                           hover:bg-gray-50 transition-colors"
              >
                Gallery
              </button>
            )}
          </div>
          {saved && <p className="text-xs text-green-700 dark:text-green-400 text-center">Saved.</p>}
        </div>
        <ResizeHandle
          currentWidth={panelWidth}
          defaultWidth={420}
          label="Resize workflow panel"
          min={280}
          max={640}
          onResize={setPanelWidth}
        />
      </div>

      {/* Right panel — independently memoizable editors */}
      <div className="flex-1 flex flex-col min-h-0">
        {editing === "merge" ? (
          <MergeEditorPanel merge={config.merge} onChange={updateMerge} />
        ) : editing === "extraction" ? (
          <ExtractionEditor
            extraction={config.extraction ?? DEFAULT_EXTRACTION}
            onChange={updateExtraction}
          />
        ) : editing === "orientation" ? (
          <OrientationEditor
            prompt={config.orientation_prompt}
            schema={config.orientation_schema}
            autoReview={autoReview}
            onPromptChange={(orientation_prompt) => {
              setConfig((current) => current ? { ...current, orientation_prompt } : current);
              setDirty(true);
            }}
            onSchemaChange={(orientation_schema) => {
              setConfig((current) => current ? { ...current, orientation_schema } : current);
              setDirty(true);
            }}
          />
        ) : editing === "auto_adaptive_agents" ? (
          <AdaptiveAgentsEditorPanel
            count={adaptiveAgentCount}
            range={adaptiveAgentRange}
            coreStepLabels={parallelSteps.filter((step) => step.enabled).map((step) => step.label)}
            onCountChange={(count) => {
              setConfig(withAdaptiveAgentCount(config, count));
              setDirty(true);
            }}
            onBrowse={browseSpecialists}
          />
        ) : editing === "pipeline_settings" ? (
          <PipelineSettingsEditorPanel
            config={config}
            activeProfile={activeProfile}
            onContextCacheChange={(enabled) => {
              setConfig({ ...config, context_cache: { enabled } });
              setDirty(true);
            }}
            onVariablesChange={(variables) => {
              setConfig({ ...config, variables });
              setDirty(true);
            }}
            onCalibrationAppend={(stepId, text) => {
              setConfig({
                ...config,
                steps: config.steps.map((step) =>
                  step.id === stepId
                    ? { ...step, prompt: `${step.prompt}\n\n${text}` }
                    : step,
                ),
              });
              setDirty(true);
            }}
            onResetParallelTemplate={(source) => void resetParallelTemplate(source)}
            onParallelTemplateChange={(parallel_context_template) => {
              promptResetRequestRef.current += 1;
              setConfig({ ...config, parallel_context_template });
              setDirty(true);
            }}
          />
        ) : editingStep ? (
          <StepEditorPanel
            step={editingStep}
            config={config}
            settings={settings}
            catalogs={catalogs}
            conditionStepIds={conditionStepIds}
            onUpdate={updateStep}
            onPhaseChange={updateStepPhase}
          />
        ) : (
          <div className="flex items-center justify-center h-full text-gray-500 dark:text-gray-400">
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

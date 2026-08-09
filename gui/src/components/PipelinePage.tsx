import { useState, useEffect, useId, useRef } from "react";
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
  VarSpec,
  InputSlot,
  Settings,
  ModelCatalog,
  ArtifactSelector,
  PrimaryArtifactPart,
  NamedInputArtifactPart,
  StepArtifactPart,
  StepContext,
} from "../lib/types";
import { reorderSteps } from "../lib/pipelineHelpers";
import {
  decodeModelSelection,
  effortOptions,
  encodeModelSelection,
  providerTransport,
  PROVIDERS,
} from "../lib/providers";
import WaveDiagram, { type WaveSelection } from "./WaveDiagram";
import PromptEditor from "./PromptEditor";
import ResizeHandle from "./ResizeHandle";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";
import useModalDialog from "../hooks/useModalDialog";

interface Props {
  onClose: () => void;
  onDirtyChange?: (dirty: boolean) => void;
  onProfileChange?: () => void;
  showBack?: boolean;
}

const DEFAULT_MERGE: MergeConfig = { enabled: true, prompt: "", agents: [] };
const DEFAULT_EXTRACTION: ExtractionConfig = {
  method: "",
};

type EditingMode = WaveSelection | null;

/** Mirror the backend's effective dependency graph and return the target
 * step's transitive upstreams in profile order. */
function conditionUpstreamIds(steps: StepConfig[], targetId: string): string[] {
  const enabled = steps.filter((step) => step.enabled);
  const ids = new Set(enabled.map((step) => step.id));
  const deps = new Map<string, string[]>();
  for (const step of enabled) {
    const selectedProducers = (step.context?.include ?? [])
      .filter((selector): selector is Extract<ArtifactSelector, { kind: "step" }> =>
        selector.kind === "step")
      .map((selector) => selector.step);
    deps.set(
      step.id,
      [...new Set([...(step.after ?? []), ...selectedProducers])]
        .filter((id) => ids.has(id)),
    );
  }
  const upstream = new Set<string>();
  const pending = [...(deps.get(targetId) ?? [])];
  while (pending.length) {
    const id = pending.pop()!;
    if (upstream.has(id)) continue;
    upstream.add(id);
    pending.push(...(deps.get(id) ?? []));
  }
  return enabled.filter((step) => upstream.has(step.id)).map((step) => step.id);
}

function primaryPartsForMode(inputMode: string): PrimaryArtifactPart[] {
  if (inputMode === "none") return [];
  if (inputMode === "folder") return ["text", "source"];
  return ["text", "structure", "visuals", "source"];
}

function defaultStepContext(
  phase: Phase,
  inputMode: string,
  useOrientation: boolean,
  priorSteps: Array<Pick<StepConfig, "id" | "enabled">>,
): StepContext {
  const include: ArtifactSelector[] = [];
  const primary = primaryPartsForMode(inputMode);
  if (phase === "parallel" && primary.length) {
    include.push({ kind: "primary", parts: primary });
  }
  if (useOrientation) include.push({ kind: "survey" });
  if (phase === "sequential") {
    include.push(...priorSteps
      .filter((step) => step.enabled)
      .map((step): ArtifactSelector => ({
        kind: "step",
        step: step.id,
        parts: ["report"],
      })));
  }
  return { include };
}

const OUTPUT_SCHEMA_TYPES = new Set([
  "object", "array", "string", "number", "integer", "boolean", "null",
]);

function outputSchemaError(schema: unknown, path = "$", depth = 0): string | null {
  if (depth > 32) return `${path}: nesting exceeds 32 levels`;
  if (!schema || typeof schema !== "object" || Array.isArray(schema)) {
    return `${path}: schema must be a JSON object`;
  }
  const object = schema as Record<string, unknown>;
  if (object.type !== undefined &&
      (typeof object.type !== "string" || !OUTPUT_SCHEMA_TYPES.has(object.type))) {
    return `${path}.type: unsupported type`;
  }
  if (object.required !== undefined) {
    if (!Array.isArray(object.required) || object.required.some((key) => typeof key !== "string")) {
      return `${path}.required: expected an array of strings`;
    }
    if (new Set(object.required).size !== object.required.length) {
      return `${path}.required: entries must be unique`;
    }
    if (object.type !== undefined && object.type !== "object") {
      return `${path}.required: only valid for an object schema`;
    }
  }
  if (object.properties !== undefined) {
    if (!object.properties || typeof object.properties !== "object" ||
        Array.isArray(object.properties)) {
      return `${path}.properties: expected an object`;
    }
    if (object.type !== undefined && object.type !== "object") {
      return `${path}.properties: only valid for an object schema`;
    }
    for (const [key, child] of Object.entries(object.properties)) {
      const error = outputSchemaError(child, `${path}.properties.${key}`, depth + 1);
      if (error) return error;
    }
  }
  if (object.items !== undefined) {
    if (object.type !== undefined && object.type !== "array") {
      return `${path}.items: only valid for an array schema`;
    }
    return outputSchemaError(object.items, `${path}.items`, depth + 1);
  }
  return null;
}

export default function PipelinePage({
  onClose,
  onDirtyChange,
  onProfileChange,
  showBack = true,
}: Props) {
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
  const [activeProfile, setActiveProfile] = useState<string>("deep-review");
  const [profileMutationPending, setProfileMutationPending] = useState(false);
  const [exportMenuOpen, setExportMenuOpen] = useState(false);
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
  const [panelWidth, setPanelWidth] = usePersistentPanelWidth(
    "pipeline.ui.workflowPanelWidth",
    420,
    280,
    640,
  );

  // Drag-drop state. We track the dragged step's id and the drop target
  // (an insertion index plus the phase that drop site implies). The phase
  // is read from the section the user is hovering over so dropping into a
  // different section auto-flips the step's phase.
  const [dragId, setDragId] = useState<string | null>(null);
  const [dropHint, setDropHint] = useState<{ idx: number; phase: Phase } | null>(null);

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

  // Fill optional collection fields so edits always serialize the current
  // profile format.
  function normalizeConfig(c: PipelineConfig): PipelineConfig {
    return {
      ...c,
      steps: (c.steps ?? []).map((step) => ({
        ...step,
        after: step.after ?? [],
        context: step.context ?? { include: [] },
      })),
      merge: c.merge ?? DEFAULT_MERGE,
      context_cache: c.context_cache ?? { enabled: false },
      extraction: c.extraction ?? DEFAULT_EXTRACTION,
      orientation_prompt: c.orientation_prompt ?? "",
      parallel_context_template: c.parallel_context_template ?? "",
      variables: c.variables ?? [],
    };
  }

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
  const NON_STEP_EDITORS = new Set(["merge", "pipeline_settings", "extraction", "orientation"]);
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
      await switchProfileForMutation("deep-review", request);
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
    setConfig({ ...config, steps: config.steps.map((s) => s.id === id ? { ...s, ...patch } : s) });
    setDirty(true);
  };

  const updateStepPhase = (id: string, phase: Phase) => {
    setConfig({
      ...config,
      steps: config.steps.map((step) => step.id === id ? { ...step, phase } : step),
    });
    setDirty(true);
  };

  const updateMerge = (patch: Partial<MergeConfig>) => {
    setConfig({ ...config, merge: { ...config.merge, ...patch } });
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

  const updateUseOrientation = (enabled: boolean) => {
    const affected = enabled
      ? 0
      : config.steps.reduce(
          (count, step) =>
            count + step.context.include.filter((selector) => selector.kind === "survey").length,
          0,
        );
    if (
      affected > 0 &&
      !window.confirm(
        `Disabling the orientation map removes survey access from ${affected} workflow step${affected === 1 ? "" : "s"}. Continue?`,
      )
    ) {
      return;
    }
    const nextConfig = {
      ...config,
      use_orientation: enabled,
      steps: enabled
        ? config.steps
        : config.steps.map((step) => ({
            ...step,
            context: {
              include: step.context.include.filter((selector) => selector.kind !== "survey"),
            },
          })),
    };
    if (affected > 0) {
      setUndoRewrite({
        before: config,
        after: JSON.stringify(nextConfig),
        dirtyBefore: dirty,
        editingBefore: editing,
        message: "Survey access rules were removed.",
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

  const handleExportItem = async () => {
    if (!editingStep) return;
    const envelope: ExportEnvelope = { type: "step", data: editingStep };
    const defaultName = `pipeline-step-${editingStep.id}.json`;
    try {
      const path = await saveDialog({
        defaultPath: defaultName,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      await invoke("export_item", { path, json: JSON.stringify(envelope, null, 2) });
    } catch (e) {
      alert(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleExportProfile = async () => {
    const defaultName = `pipeline-profile-${activeProfile}.json`;
    try {
      const path = await saveDialog({
        defaultPath: defaultName,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      if (!(await saveBeforeExport("the profile"))) return;
      await invoke("export_profile", { id: activeProfile, path });
    } catch (e) {
      alert(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleExportBundle = async () => {
    try {
      const path = await saveDialog({
        defaultPath: "pipeline-settings-backup.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      if (!(await saveBeforeExport("the settings bundle"))) return;
      await invoke("export_bundle", { path });
    } catch (e) {
      alert(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleImportUrl = async () => {
    const url = window.prompt("Profile URL (a shared profile JSON):");
    if (!url) return;
    try {
      const summary = await invoke<ProfileSummary>("import_profile_from_url", { url: url.trim() });
      await refreshProfiles();
      alert(
        `Imported “${summary.name}” after safety validation. It was not activated. ` +
        "Select it from the profile list to inspect its prompts, tools, agents, and fan-out settings before running it.",
      );
    } catch (e) {
      alert(`Import failed: ${e instanceof Error ? e.message : String(e)}`);
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
            `${enabled.length} enabled steps; up to ${maxAttempts} provider attempts per run (including retries and merges).\n` +
            `Tools: ${tools.join(", ") || "none"}\n` +
            `Agents: ${agents.join(", ") || "profile default"}\n\n` +
            "Review the imported prompts in the editor before starting a run.",
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

  const addStep = (phase: "parallel" | "sequential") => {
    const id = `custom_${Date.now()}`;
    const prompt = phase === "parallel"
      ? "# Custom Step\n\nDescribe what this step should evaluate.\n\n## Method\n\n...\n\n## Output structure\n\n..."
      : "You are performing a custom analysis step.\n\nORIENTATION MAP:\n{orientation}\n\nPRIOR OUTPUTS:\n{prior_outputs}\n\nLAST OUTPUT:\n{last_output}\n\n[Your instructions here]";
    setConfig({
      ...config,
      steps: [...config.steps, {
        id, label: "Custom Step", enabled: true, phase,
        tools: [], agents: [], prompt, after: [],
        context: defaultStepContext(
          phase,
          config.extraction?.input_mode || "document",
          config.use_orientation,
          config.steps,
        ),
      }],
    });
    setEditing(id); setDirty(true);
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

  const moveStepWithinPhase = (id: string, direction: -1 | 1) => {
    const step = config.steps.find((candidate) => candidate.id === id);
    if (!step) return;
    const peers = config.steps.filter((candidate) => candidate.phase === step.phase);
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
            <span className="text-[10px] uppercase tracking-wider text-blue-700 dark:text-blue-300 font-medium">
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
                onToggle={() => updateStepEnabled(step.id, !step.enabled)}
                onSelect={() => setEditing(editing === step.id ? null : step.id)}
                onDelete={() => removeStep(step.id)}
                canMoveUp={posInSection > 0}
                canMoveDown={posInSection < parallelSteps.length - 1}
                onMoveUp={() => moveStepWithinPhase(step.id, -1)}
                onMoveDown={() => moveStepWithinPhase(step.id, 1)}
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
                onToggle={() => updateStepEnabled(step.id, !step.enabled)}
                onSelect={() => setEditing(editing === step.id ? null : step.id)}
                onDelete={() => removeStep(step.id)}
                canMoveUp={posInSection > 0}
                canMoveDown={posInSection < sequentialSteps.length - 1}
                onMoveUp={() => moveStepWithinPhase(step.id, -1)}
                onMoveDown={() => moveStepWithinPhase(step.id, 1)}
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
              className="flex-1 py-2 px-3 border border-dashed border-orange-400 rounded-lg
                         text-sm text-orange-700 hover:border-orange-500 hover:text-orange-900 transition-colors"
            >
              + Sequential
            </button>
          </div>
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
            <button
              onClick={handleImportUrl}
              title="Import a shared profile from a URL"
              className="flex-1 py-1.5 px-3 border border-gray-300 rounded-lg text-xs text-gray-500
                         hover:bg-gray-50 transition-colors"
            >
              From URL…
            </button>
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

      {/* Right panel — editor */}
      <div className="flex-1 flex flex-col min-h-0">
        {editing === "merge" ? (
          /* Merge prompt editor */
          <div className="flex-1 flex flex-col min-h-0">
            <div className="p-4 border-b border-gray-200 dark:border-gray-700 space-y-3">
              <div className="flex items-center gap-3">
                <button
                  type="button"
                  role="switch"
                  aria-label="Cross-agent merge"
                  aria-checked={config.merge.enabled}
                  onClick={() => updateMerge({ enabled: !config.merge.enabled })}
                  className={`w-8 h-5 rounded-full relative transition-colors shrink-0 ${
                    config.merge.enabled ? "bg-green-600" : "bg-gray-300 dark:bg-gray-600"
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
                ariaLabel="Cross-agent merge prompt"
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
            onToggleUse={updateUseOrientation}
            onPromptChange={(orientation_prompt) => {
              setConfig((current) =>
                current ? { ...current, orientation_prompt } : current,
              );
              setDirty(true);
            }}
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

              {/* Shared context cache */}
              <div className="flex items-start gap-3">
                <button
                  type="button"
                  role="switch"
                  aria-label="Reuse shared input context"
                  aria-checked={config.context_cache?.enabled ?? false}
                  onClick={() => {
                    setConfig({
                      ...config,
                      context_cache: { enabled: !(config.context_cache?.enabled ?? false) },
                    });
                    setDirty(true);
                  }}
                  className={`w-8 h-5 rounded-full relative transition-colors shrink-0 mt-0.5 ${
                    config.context_cache?.enabled ? "bg-green-600" : "bg-gray-300 dark:bg-gray-600"
                  }`}
                >
                  <div className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${
                    config.context_cache?.enabled ? "translate-x-3.5" : "translate-x-0.5"
                  }`} />
                </button>
                <div>
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
                      Reuse shared input context
                    </span>
                    <span className="rounded bg-blue-50 px-1.5 py-0.5 text-[10px] font-medium text-blue-600 dark:bg-blue-950/50 dark:text-blue-300">
                      {activeProfile === "deep-review" ? "Full Review default" : "Optional"}
                    </span>
                  </div>
                  <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                    Paper Review (Full) enables this by default. It prepares the extracted input
                    and orientation map once for all review steps.
                    Pipeline automatically uses provider prompt caches for API calls and forked
                    base sessions for Claude or Codex CLI. For other profiles, turn it on for
                    large, multi-step reviews; unsupported providers fall back safely to ordinary
                    calls.
                  </p>
                  {config.context_cache?.enabled && (
                    <p className="text-[11px] text-green-700 dark:text-green-400 mt-1">
                      Enabled for this profile. Cache reads and writes will appear in token usage.
                    </p>
                  )}
                </div>
              </div>

              {/* Orientation map toggle */}
              <div className="flex items-start gap-3">
                <button
                  type="button"
                  role="switch"
                  aria-label="Build orientation map"
                  aria-checked={config.use_orientation}
                  onClick={() => updateUseOrientation(!config.use_orientation)}
                  className={`w-8 h-5 rounded-full relative transition-colors shrink-0 mt-0.5 ${
                    config.use_orientation ? "bg-green-600" : "bg-gray-300 dark:bg-gray-600"
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

              {/* Run-time variables */}
              <VariablesEditor
                variables={config.variables ?? []}
                onChange={(variables) => { setConfig({ ...config, variables }); setDirty(true); }}
              />

              {/* Calibrate from rejected issues */}
              <CalibrateSection
                onAppend={(stepId, text) => {
                  setConfig({
                    ...config,
                    steps: config.steps.map((s) => (s.id === stepId ? { ...s, prompt: `${s.prompt}\n\n${text}` } : s)),
                  });
                  setDirty(true);
                }}
              />

              {/* Parallel context template */}
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
                    Parallel step context template
                  </label>
                  <div className="flex items-center gap-3">
                    <button
                      type="button"
                      onClick={() => void resetParallelTemplate("generic")}
                      className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
                      title="Neutral wrapper for any input: survey + instructions + input path."
                    >
                      Reset to generic
                    </button>
                    <button
                      type="button"
                      onClick={() => void resetParallelTemplate("paper")}
                      className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
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
                    promptResetRequestRef.current += 1;
                    setConfig({ ...config, parallel_context_template });
                    setDirty(true);
                  }}
                  context={{ kind: "parallel_template" }}
                  ariaLabel="Parallel step context template"
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
                  aria-label="Step label"
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
                      type="button"
                      aria-pressed={editingStep.phase === phase}
                      onClick={() => updateStepPhase(editingStep.id, phase)}
                      className={`px-3 py-1 text-xs rounded-full border transition-colors ${
                        editingStep.phase === phase
                          ? phase === "parallel"
                            ? "bg-blue-700 text-white border-blue-700"
                            : "bg-orange-700 text-white border-orange-700"
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
                settings={settings}
                catalogs={catalogs}
                onChange={(patch) => updateStep(editingStep.id, patch)}
              />
              <AdvancedStepOptions
                key={editingStep.id}
                step={editingStep}
                otherSteps={config.steps
                  .filter((s) => s.enabled && s.id !== editingStep.id)
                  .map(({ id, label }) => ({ id, label }))}
                defaultReportStepIds={config.steps
                  .slice(0, config.steps.findIndex((step) => step.id === editingStep.id))
                  .filter((step) => step.enabled)
                  .map((step) => step.id)}
                namedInputs={config.extraction?.extra_inputs ?? []}
                inputMode={config.extraction?.input_mode || "document"}
                surveyEnabled={config.use_orientation}
                conditionStepIds={conditionStepIds}
                onChange={(patch) => updateStep(editingStep.id, patch)}
              />
            </div>
            <div className="flex-1 min-h-0 p-4">
              <PromptEditor
                value={editingStep.prompt}
                onChange={(prompt) => updateStep(editingStep.id, { prompt })}
                ariaLabel={`Prompt for ${editingStep.label}`}
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

// --- Pre-processing editors ---
//
// These edit the Stage 0 steps (text extraction + orientation map) that run
// before any review step. Both are profile-scoped: each profile can decide
// to use a different extractor or a custom orientation prompt.

const EXTRACTION_METHODS: { value: string; label: string; hint: string }[] = [
  { value: "", label: "Inherit from global Settings", hint: "Use whatever PDF extractor is configured globally." },
  { value: "auto", label: "Auto", hint: "Try the global setting; same as inherit." },
  { value: "llm", label: "LLM", hint: "Bounded, page-verified transcription through the active provider. Slower, but preserves equations and original typos." },
  { value: "paddleocr-vl-full", label: "Local engine: PaddleOCR-VL 1.6 Full Parser", hint: "Official layout-aware client with structured regions, title hierarchy, formula metadata, and cross-page table reconstruction. Reuses Pipeline's managed llama.cpp server." },
  { value: "paddleocr-vl", label: "Local engine: PaddleOCR-VL 1.6 Q8 (fast)", hint: "Managed direct page transcription with retries and resumable checkpoints. Tune it in Settings → PDF Extraction." },
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
  const markerRetired = method === "marker";
  const hint = markerRetired
    ? "Marker is unavailable in Pipeline 1.0.1 because its compatible Python dependencies contain known security vulnerabilities. Choose a supported method before running this workflow."
    : EXTRACTION_METHODS.find((m) => m.value === method)?.hint;
  const inputMode = extraction.input_mode || "document";

  return (
    <div className="flex-1 flex flex-col min-h-0 overflow-y-auto">
      <div className="p-4 space-y-5">
        <div>
          <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200 mb-1">Input & PDF Extraction</h3>
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
            aria-label="Workflow input mode"
            value={inputMode}
            onChange={(e) => onChange({ input_mode: e.target.value })}
            className="w-full py-1.5 px-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                       text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                       focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors"
          >
            <option value="document">Document — a PDF, LaTeX, or Word file</option>
            <option value="folder">Folder — inventory a directory; steps Read files on demand</option>
            <option value="none">None — run from the step prompts alone</option>
          </select>
          <p className="text-[11px] text-gray-500 dark:text-gray-400 mt-1.5 leading-relaxed">
            Selecting a folder in the main window uses folder mode automatically, whatever this
            is set to.
          </p>
        </div>

        {/* Extra named inputs */}
        <ExtraInputsEditor
          slots={extraction.extra_inputs ?? []}
          onChange={(extra_inputs) => onChange({ extra_inputs })}
        />

        {inputMode !== "document" ? (
          <div className="text-[11px] text-gray-600 dark:text-gray-400 leading-relaxed border-t border-gray-100 dark:border-gray-800 pt-3">
            Extraction settings below apply only to document inputs.
          </div>
        ) : null}

        <div>
          <label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1.5">
            Method (per profile)
          </label>
          <select
            aria-label="PDF extraction method"
            value={method}
            onChange={(e) => onChange({ method: e.target.value })}
            className="w-full py-1.5 px-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                       text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                       focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors"
          >
            {markerRetired && (
              <option value="marker" disabled>
                Marker (unavailable — choose a replacement)
              </option>
            )}
            {EXTRACTION_METHODS.map((m) => (
              <option key={m.value} value={m.value}>{m.label}</option>
            ))}
          </select>
          {hint && (
            <p
              role={markerRetired ? "alert" : undefined}
              className={`text-[11px] mt-1.5 leading-relaxed ${
                markerRetired
                  ? "text-amber-700 dark:text-amber-300"
                  : "text-gray-500 dark:text-gray-400"
              }`}
            >
              {hint}
            </p>
          )}
        </div>

        <div className="text-[11px] text-gray-600 dark:text-gray-400 leading-relaxed border-t border-gray-100 dark:border-gray-800 pt-3">
          The chosen PDF method is authoritative: incomplete or failed extraction stops before
          orientation instead of silently switching engines. Parser-specific speed, memory, OCR,
          and image settings are configured once in Settings → PDF Extraction. LaTeX inputs bypass
          PDF extraction.
        </div>
      </div>
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
  const resetRequest = useRef(0);
  const mounted = useRef(true);
  useEffect(() => () => {
    mounted.current = false;
    resetRequest.current += 1;
  }, []);

  const insertDefault = async (name: string) => {
    const request = ++resetRequest.current;
    try {
      const template = await invoke<string>("get_default_prompt", { name });
      if (mounted.current && request === resetRequest.current) {
        onPromptChange(template);
      }
    } catch (error) {
      if (mounted.current && request === resetRequest.current) {
        alert(
          `Failed to load the default orientation prompt: ${
            error instanceof Error ? error.message : String(error)
          }`,
        );
      }
    }
  };

  const changePrompt = (next: string) => {
    resetRequest.current += 1;
    onPromptChange(next);
  };

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="p-4 border-b border-gray-200 dark:border-gray-700 space-y-3">
        <div>
          <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200 mb-1">Orientation Map</h3>
          <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
            Stage 0b. One LLM call that builds a structured JSON survey of the input before any
            step runs — for a paper: sections, theorems, tables, notation. Steps that select the
            survey receive it via {"{orientation}"}, which keeps them grounded in what the input
            actually contains. The survey can use any JSON schema your prompt asks for.
          </p>
        </div>

        <div className="flex items-center gap-3">
          <button
            type="button"
            role="switch"
            aria-label="Build orientation map"
            aria-checked={useOrientation}
            onClick={() => onToggleUse(!useOrientation)}
            className={`w-8 h-5 rounded-full relative transition-colors shrink-0 ${
              useOrientation ? "bg-green-600" : "bg-gray-300 dark:bg-gray-600"
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
            type="button"
            onClick={() => void insertDefault("orientation_generic")}
            className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
            title="Insert the generic survey prompt (works for any input)."
          >
            Insert generic survey
          </button>
          <button
            type="button"
            onClick={() => void insertDefault("orientation_folder")}
            className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
            title="Insert the folder survey prompt (explores the folder with the Read tool)."
          >
            Insert folder survey
          </button>
          <button
            type="button"
            onClick={() => void insertDefault("orientation")}
            className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
            title="Insert the paper-review survey prompt (sections, theorems, tables, notation)."
          >
            Insert paper survey
          </button>
          <button
            type="button"
            onClick={() => changePrompt("")}
            disabled={prompt.trim() === ""}
            className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100
                       disabled:opacity-40 disabled:hover:text-gray-600 dark:disabled:hover:text-gray-400 transition-colors"
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
            onChange={changePrompt}
            context={{ kind: "orientation" }}
            ariaLabel="Orientation map prompt"
          />
        </div>
        {prompt.trim() === "" && (
          <p className="text-[11px] text-gray-500 dark:text-gray-400 mt-2 leading-relaxed shrink-0">
            Empty — using the bundled default: <span className="font-mono">prompts/orientation.md</span>{" "}
            for document inputs, <span className="font-mono">prompts/orientation_folder.md</span> for
            folder inputs (overridable at <span className="font-mono">~/.pipeline/prompts/</span>).
            Stock prompts adapt to the input mode; a customized prompt is used as-is.
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
  canMoveUp,
  canMoveDown,
  onMoveUp,
  onMoveDown,
  onDragStart,
  onDragEnd,
}: {
  step: StepConfig;
  selected: boolean;
  isDragging: boolean;
  onToggle: () => void;
  onSelect: () => void;
  onDelete?: () => void;
  canMoveUp: boolean;
  canMoveDown: boolean;
  onMoveUp: () => void;
  onMoveDown: () => void;
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
  if (Object.keys(step.model_overrides ?? {}).length) {
    badges.push(`${Object.keys(step.model_overrides ?? {}).length} model ${Object.keys(step.model_overrides ?? {}).length === 1 ? "policy" : "policies"}`);
  }
  if (Object.keys(step.effort_overrides ?? {}).length) {
    badges.push(`${Object.keys(step.effort_overrides ?? {}).length} effort ${Object.keys(step.effort_overrides ?? {}).length === 1 ? "override" : "overrides"}`);
  }

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
          aria-hidden="true"
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
        <span className="flex shrink-0 gap-0.5">
          <button
            type="button"
            onClick={onMoveUp}
            disabled={!canMoveUp}
            aria-label={`Move ${step.label} up`}
            className="flex h-6 w-6 items-center justify-center text-[10px] leading-none text-gray-600
                       hover:text-gray-900 disabled:opacity-25 dark:text-gray-400 dark:hover:text-gray-100"
          >
            ▲
          </button>
          <button
            type="button"
            onClick={onMoveDown}
            disabled={!canMoveDown}
            aria-label={`Move ${step.label} down`}
            className="flex h-6 w-6 items-center justify-center text-[10px] leading-none text-gray-600
                       hover:text-gray-900 disabled:opacity-25 dark:text-gray-400 dark:hover:text-gray-100"
          >
            ▼
          </button>
        </span>
        <button
          type="button"
          role="switch"
          aria-label={`Enable ${step.label}`}
          aria-checked={step.enabled}
          onClick={onToggle}
          className={`w-8 h-5 rounded-full relative transition-colors shrink-0 ${
            step.enabled ? "bg-green-600" : "bg-gray-300 dark:bg-gray-600"
          }`}
        >
          <div className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${
            step.enabled ? "translate-x-3.5" : "translate-x-0.5"
          }`} />
        </button>
        <button type="button" onClick={onSelect} className="flex-1 text-left text-sm font-medium text-gray-800 dark:text-gray-200 truncate">
          {step.label}
        </button>
        <button
          type="button"
          onClick={onDelete}
          aria-label={`Remove ${step.label}`}
          className="p-1 text-gray-500 hover:text-red-600 shrink-0"
        >
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
            ? `h-10 my-1 mx-2 rounded border border-dashed ${tone} text-[10px] flex items-center justify-center text-gray-600 dark:text-gray-400`
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
  settings,
  catalogs,
  onChange,
}: {
  step: StepConfig;
  settings: Settings | null;
  catalogs: Record<string, ModelCatalog>;
  onChange: (patch: Partial<StepConfig>) => void;
}) {
  const hasOverride = !!(step.model || step.effort || Object.keys(step.model_overrides ?? {}).length || Object.keys(step.effort_overrides ?? {}).length);
  const [open, setOpen] = useState(hasOverride);
  const providers = Array.from(new Set(
    (step.agents?.length ? step.agents : [settings?.preferred_provider || "claude"])
      .map((provider) => provider || "claude"),
  ));

  const transportFor = (provider: string): "cli" | "api" =>
    settings ? providerTransport(settings, provider) : "api";

  const updateModel = (key: string, value: string) => {
    const next = { ...(step.model_overrides ?? {}) };
    const selection = decodeModelSelection(value);
    if (selection) next[key] = selection;
    else delete next[key];
    onChange({ model: "", model_overrides: next });
  };

  const updateEffort = (key: string, value: string) => {
    const next = { ...(step.effort_overrides ?? {}) };
    if (value) next[key] = value;
    else delete next[key];
    onChange({ effort: "", effort_overrides: next });
  };

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
          <span className="text-[10px] text-gray-600 dark:text-gray-400 font-mono ml-1">
            {[
              step.model,
              step.effort,
              ...Object.entries(step.model_overrides ?? {}).map(([key, selection]) =>
                `${key}=${selection.mode === "automatic" ? "auto" : selection.mode === "role" ? selection.role : selection.model}`
              ),
            ].filter(Boolean).join(" / ")}
          </span>
        )}
      </button>
      {open && (
        <div className="mt-2 space-y-2 pl-3 border-l-2 border-gray-200 dark:border-gray-700">
          <p className="text-[10px] text-gray-600 dark:text-gray-400 leading-relaxed">
            Each provider can inherit its global policy, follow its own current
            default, use a stable role, or pin an exact discovered model.
          </p>
          {providers.map((provider) => {
            const key = `${provider}:${transportFor(provider)}`;
            const catalog = catalogs[provider];
            const selection = step.model_overrides?.[key];
            const value = encodeModelSelection(selection);
            const known = value === "inherit" || value === "automatic"
              || catalog?.roles.some((role) => value === `role:${role.id}`)
              || catalog?.models.some((model) => value === `pinned:${model.id}`);
            const efforts = effortOptions(
              catalog,
              selection,
              provider === "claude" ? ["low", "medium", "high", "max"]
                : provider === "codex" ? ["low", "medium", "high"] : [],
            );
            return (
              <div key={key} className="space-y-1.5">
                <div className="text-[10px] font-semibold uppercase tracking-wide text-gray-500">
                  {provider} · {transportFor(provider)}
                </div>
                <select
                  aria-label={`${provider} ${transportFor(provider)} model override`}
                  value={value}
                  onChange={(event) => updateModel(key, event.target.value)}
                  className="w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200"
                >
                  <option value="inherit">Inherit global policy</option>
                  <option value="automatic">Automatic — provider default</option>
                  {!!catalog?.roles.length && <optgroup label="Stable roles">
                    {catalog.roles.map((role) => <option key={role.id} value={`role:${role.id}`}>{role.label} — {role.model}</option>)}
                  </optgroup>}
                  {!!catalog?.models.length && <optgroup label="Pin exact model">
                    {catalog.models.map((model) => <option key={model.id} value={`pinned:${model.id}`} disabled={model.deprecated}>{model.display_name || model.id}</option>)}
                  </optgroup>}
                  {!known && <option value={value}>Saved selection (not currently listed)</option>}
                </select>
                {!!efforts.length && (
                  <select
                    aria-label={`${provider} ${transportFor(provider)} effort override`}
                    value={step.effort_overrides?.[key] ?? ""}
                    onChange={(event) => updateEffort(key, event.target.value)}
                    className="w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200"
                  >
                    <option value="">Inherit effort</option>
                    {efforts.map((effort) => <option key={effort} value={effort}>{effort.charAt(0).toUpperCase() + effort.slice(1)}</option>)}
                  </select>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

// --- Calibrate a synthesis step from rejected issues (Release 1.4) ---

interface CalibrationDraft {
  addendum: string;
  target_step_id: string;
  target_label: string;
  rejected_count: number;
}

function CalibrateSection({ onAppend }: { onAppend: (stepId: string, text: string) => void }) {
  const [draft, setDraft] = useState<CalibrationDraft | null>(null);
  const [text, setText] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const titleId = useId();
  const descriptionId = useId();
  const textareaId = useId();
  const closeDraft = () => setDraft(null);
  const dialogRef = useModalDialog<HTMLDivElement>(closeDraft, draft !== null);

  const run = async () => {
    setLoading(true);
    setError(null);
    try {
      const d = await invoke<CalibrationDraft>("draft_calibration");
      setDraft(d);
      setText(d.addendum);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div>
      <label className="text-sm font-medium text-gray-700 dark:text-gray-300">Calibrate from feedback</label>
      <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5 mb-2 leading-relaxed">
        Draft an instruction from the issues you've rejected (in the Issues view) so this profile stops
        flagging them, and append it to the synthesis step.
      </p>
      <button
        type="button"
        onClick={run}
        disabled={loading}
        className="text-xs px-3 py-1.5 rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-50"
      >
        {loading ? "Drafting…" : "Draft from my rejected issues"}
      </button>
      {error && <p className="text-xs text-red-600 dark:text-red-400 mt-1.5">{error}</p>}

      {draft && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={() => setDraft(null)}>
          <div
            ref={dialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
            aria-describedby={descriptionId}
            tabIndex={-1}
            className="w-full max-w-lg rounded-xl bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 shadow-xl p-5"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 id={titleId} className="text-base font-semibold text-gray-900 dark:text-gray-100 mb-1">Calibration draft</h3>
            <p id={descriptionId} className="text-xs text-gray-500 dark:text-gray-400 mb-3">
              From {draft.rejected_count} rejected issue{draft.rejected_count === 1 ? "" : "s"}. Will be appended to
              step <span className="font-medium">{draft.target_label}</span>. Edit before applying.
            </p>
            <label htmlFor={textareaId} className="sr-only">Calibration instruction</label>
            <textarea
              id={textareaId}
              data-autofocus
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={6}
              className="w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
            />
            <div className="flex justify-end gap-2 mt-4">
              <button
                type="button"
                onClick={() => setDraft(null)}
                className="px-4 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => { if (text.trim()) onAppend(draft.target_step_id, text.trim()); setDraft(null); }}
                disabled={!text.trim()}
                className="px-4 py-2 text-sm rounded-lg bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 hover:opacity-90 disabled:opacity-40"
              >
                Append &amp; keep
              </button>
            </div>
            <p className="text-[10px] text-gray-600 dark:text-gray-400 mt-2">Save the profile to persist the change.</p>
          </div>
        </div>
      )}
    </div>
  );
}

// --- Extra named inputs (profile-level) ---
//
// Additional inputs beyond the primary one; each is extracted at run time and
// available for selection by each step. When text is selected, {input:key}
// resolves to the step-private staged path.

function ExtraInputsEditor({
  slots,
  onChange,
}: {
  slots: InputSlot[];
  onChange: (s: InputSlot[]) => void;
}) {
  const update = (i: number, patch: Partial<InputSlot>) =>
    onChange(slots.map((s, j) => (j === i ? { ...s, ...patch } : s)));
  const remove = (i: number) => onChange(slots.filter((_, j) => j !== i));
  const add = () =>
    onChange([...slots, { key: `input${slots.length + 1}`, label: "", mode: "document", required: false }]);

  const inputClass =
    "py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-1 focus:ring-gray-400";

  return (
    <div>
      <label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1">
        Extra inputs
      </label>
      <p className="text-[11px] text-gray-500 dark:text-gray-400 mb-2 leading-relaxed">
        Additional files/folders the Run action asks for. A step can select the extracted text,
        original source, or both; selected text is available as{" "}
        <code className="font-mono">{"{input:key}"}</code>. Useful for a response letter, rubric,
        or prior report alongside the main input.
      </p>
      <div className="space-y-2">
        {slots.map((s, i) => (
          <div key={i} className="flex flex-wrap items-center gap-1.5 p-2 border border-gray-200 dark:border-gray-700 rounded">
            <input
              aria-label={`Extra input ${i + 1} key`}
              value={s.key}
              onChange={(e) => update(i, { key: e.target.value.replace(/[^a-zA-Z0-9_]/g, "") })}
              placeholder="key"
              className={`${inputClass} w-24 font-mono`}
            />
            <input
              aria-label={`Extra input ${i + 1} label`}
              value={s.label ?? ""}
              onChange={(e) => update(i, { label: e.target.value })}
              placeholder="label"
              className={`${inputClass} flex-1 min-w-[6rem]`}
            />
            <select
              aria-label={`Extra input ${i + 1} mode`}
              value={s.mode ?? "document"}
              onChange={(e) => update(i, { mode: e.target.value })}
              className={inputClass}
            >
              <option value="document">document</option>
              <option value="folder">folder</option>
            </select>
            <label className="flex items-center gap-1 text-[10px] text-gray-500">
              <input
                type="checkbox"
                aria-label={`Require extra input ${s.label || s.key || i + 1}`}
                checked={!!s.required}
                onChange={(e) => update(i, { required: e.target.checked })}
              />
              required
            </label>
            <button
              type="button"
              onClick={() => remove(i)}
              aria-label={`Remove extra input ${s.label || s.key || i + 1}`}
              className="text-xs text-red-600 hover:text-red-800 dark:text-red-400 dark:hover:text-red-300 px-1"
            >
              ✕
            </button>
          </div>
        ))}
      </div>
      <button
        type="button"
        onClick={add}
        className="mt-2 text-xs text-gray-600 dark:text-gray-300 hover:text-gray-900 dark:hover:text-gray-100 border border-dashed border-gray-300 dark:border-gray-600 rounded px-2 py-1"
      >
        + Add input
      </button>
    </div>
  );
}

// --- Run-time variables (profile-level) ---
//
// Declared variables turn a profile into a template: the Run action prompts for
// values, and steps reference them as {var:key}.

function VariablesEditor({
  variables,
  onChange,
}: {
  variables: VarSpec[];
  onChange: (v: VarSpec[]) => void;
}) {
  const update = (i: number, patch: Partial<VarSpec>) =>
    onChange(variables.map((v, j) => (j === i ? { ...v, ...patch } : v)));
  const remove = (i: number) => onChange(variables.filter((_, j) => j !== i));
  const add = () =>
    onChange([...variables, { key: `var${variables.length + 1}`, label: "", kind: "text", default: "" }]);

  const inputClass =
    "py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-1 focus:ring-gray-400";

  return (
    <div>
      <label className="text-sm font-medium text-gray-700 dark:text-gray-300">Variables</label>
      <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5 mb-2">
        Values the Run action asks for, substituted into prompts as{" "}
        <code className="font-mono">{"{var:key}"}</code>.
      </p>
      <div className="space-y-2">
        {variables.map((v, i) => (
          <div key={i} className="flex flex-wrap items-center gap-1.5 p-2 border border-gray-200 dark:border-gray-700 rounded">
            <input
              aria-label={`Variable ${i + 1} key`}
              value={v.key}
              onChange={(e) => update(i, { key: e.target.value.replace(/[^a-zA-Z0-9_]/g, "") })}
              placeholder="key"
              className={`${inputClass} w-24 font-mono`}
            />
            <input
              aria-label={`Variable ${i + 1} label`}
              value={v.label ?? ""}
              onChange={(e) => update(i, { label: e.target.value })}
              placeholder="label"
              className={`${inputClass} flex-1 min-w-[6rem]`}
            />
            <select
              aria-label={`Variable ${i + 1} type`}
              value={v.kind ?? "text"}
              onChange={(e) => update(i, { kind: e.target.value })}
              className={inputClass}
            >
              <option value="text">text</option>
              <option value="choice">choice</option>
              <option value="file">file</option>
            </select>
            <input
              aria-label={`Variable ${i + 1} default`}
              value={v.default ?? ""}
              onChange={(e) => update(i, { default: e.target.value })}
              placeholder="default"
              className={`${inputClass} w-24`}
            />
            <button
              type="button"
              onClick={() => remove(i)}
              aria-label={`Remove variable ${v.label || v.key || i + 1}`}
              className="text-xs text-red-600 hover:text-red-800 dark:text-red-400 dark:hover:text-red-300 px-1"
            >
              ✕
            </button>
            {v.kind === "choice" && (
              <input
                aria-label={`Variable ${v.label || v.key || i + 1} choices`}
                value={(v.choices ?? []).join(", ")}
                onChange={(e) => update(i, { choices: e.target.value.split(",").map((s) => s.trim()).filter(Boolean) })}
                placeholder="choices, comma-separated"
                className={`${inputClass} w-full`}
              />
            )}
          </div>
        ))}
      </div>
      <button
        type="button"
        onClick={add}
        className="mt-2 text-xs text-gray-600 dark:text-gray-300 hover:text-gray-900 dark:hover:text-gray-100 border border-dashed border-gray-300 dark:border-gray-600 rounded px-2 py-1"
      >
        + Add variable
      </button>
    </div>
  );
}

// --- Artifact access and advanced execution rules ---

function replaceArtifactSelector(
  include: ArtifactSelector[],
  matches: (selector: ArtifactSelector) => boolean,
  replacement: ArtifactSelector | null,
): ArtifactSelector[] {
  const next = include.filter((selector) => !matches(selector));
  if (replacement) next.push(replacement);
  return next;
}

function artifactContextSummary(context: StepContext): string {
  const parts: string[] = [];
  for (const selector of context.include) {
    if (selector.kind === "primary") parts.push(`input ${selector.parts.length}`);
    if (selector.kind === "survey") parts.push("survey");
    if (selector.kind === "named_input") parts.push(selector.key);
    if (selector.kind === "step") {
      const labels = [
        selector.parts.includes("report") ? "report" : "",
        selector.parts.includes("files") ? "files" : "",
      ].filter(Boolean).join("+");
      parts.push(`${selector.step} ${labels}`);
    }
  }
  return parts.length ? parts.join(" · ") : "isolated";
}

function ArtifactContextEditor({
  step,
  otherSteps,
  defaultReportStepIds,
  namedInputs,
  inputMode,
  surveyEnabled,
  onChange,
}: {
  step: StepConfig;
  otherSteps: Array<Pick<StepConfig, "id" | "label">>;
  defaultReportStepIds: string[];
  namedInputs: InputSlot[];
  inputMode: string;
  surveyEnabled: boolean;
  onChange: (context: StepContext) => void;
}) {
  const include = step.context?.include ?? [];
  const primary = include.find(
    (selector): selector is Extract<ArtifactSelector, { kind: "primary" }> =>
      selector.kind === "primary",
  );
  const hasSurvey = include.some((selector) => selector.kind === "survey");
  const availablePrimary = primaryPartsForMode(inputMode);

  const setInclude = (next: ArtifactSelector[]) => onChange({ include: next });
  const setPrimaryPart = (part: PrimaryArtifactPart, checked: boolean) => {
    const parts = new Set(primary?.parts ?? []);
    if (checked) parts.add(part);
    else parts.delete(part);
    setInclude(replaceArtifactSelector(
      include,
      (selector) => selector.kind === "primary",
      parts.size ? { kind: "primary", parts: [...parts] } : null,
    ));
  };
  const setSurvey = (checked: boolean) => {
    setInclude(replaceArtifactSelector(
      include,
      (selector) => selector.kind === "survey",
      checked ? { kind: "survey" } : null,
    ));
  };
  const setNamedPart = (
    key: string,
    part: NamedInputArtifactPart,
    checked: boolean,
  ) => {
    const existing = include.find(
      (selector): selector is Extract<ArtifactSelector, { kind: "named_input" }> =>
        selector.kind === "named_input" && selector.key === key,
    );
    const parts = new Set(existing?.parts ?? []);
    if (checked) parts.add(part);
    else parts.delete(part);
    setInclude(replaceArtifactSelector(
      include,
      (selector) => selector.kind === "named_input" && selector.key === key,
      parts.size ? { kind: "named_input", key, parts: [...parts] } : null,
    ));
  };
  const setStepPart = (
    producer: string,
    part: StepArtifactPart,
    checked: boolean,
  ) => {
    const existing = include.find(
      (selector): selector is Extract<ArtifactSelector, { kind: "step" }> =>
        selector.kind === "step" && selector.step === producer,
    );
    const parts = new Set(existing?.parts ?? []);
    if (checked) parts.add(part);
    else parts.delete(part);
    setInclude(replaceArtifactSelector(
      include,
      (selector) => selector.kind === "step" && selector.step === producer,
      parts.size
        ? { kind: "step", step: producer, parts: [...parts], ...(existing?.glob ? { glob: existing.glob } : {}) }
        : null,
    ));
  };
  const setStepGlob = (producer: string, glob: string) => {
    const existing = include.find(
      (selector): selector is Extract<ArtifactSelector, { kind: "step" }> =>
        selector.kind === "step" && selector.step === producer,
    );
    if (!existing) return;
    setInclude(replaceArtifactSelector(
      include,
      (selector) => selector.kind === "step" && selector.step === producer,
      { ...existing, ...(glob ? { glob } : { glob: undefined }) },
    ));
  };

  const standardContext = () =>
    defaultStepContext(
      step.phase,
      inputMode,
      surveyEnabled,
      defaultReportStepIds.map((id) => ({ id, enabled: true })),
    );
  const reportsOnly = (): StepContext => ({
    include: defaultReportStepIds.map((producer) => ({
      kind: "step",
      step: producer,
      parts: ["report"],
    })),
  });

  const checkboxClass = "rounded border-gray-300 dark:border-gray-600";
  const chipClass =
    "text-[10px] px-2 py-1 rounded-md border border-gray-200 dark:border-gray-700 hover:border-gray-400 dark:hover:border-gray-500";

  return (
    <div className="space-y-2.5">
      <div>
        <div className="flex items-center justify-between gap-2">
          <label className="text-[10px] font-semibold uppercase tracking-wide text-gray-500">
            Artifact access
          </label>
          <span className="text-[10px] text-gray-600 dark:text-gray-400 truncate" title={artifactContextSummary(step.context)}>
            {artifactContextSummary(step.context)}
          </span>
        </div>
        <p className="text-[10px] text-gray-600 dark:text-gray-400 mt-0.5 leading-relaxed">
          {step.phase === "parallel"
            ? "This is an exact allowlist. Parallel steps are independent and cannot read another step's output."
            : "This is an exact allowlist. Selecting a step artifact also makes this step wait for its producer."}
        </p>
      </div>

      <div className="flex flex-wrap gap-1">
        <button type="button" onClick={() => onChange(standardContext())} className={chipClass}>
          Standard
        </button>
        <button
          type="button"
          onClick={() => onChange({
            include: availablePrimary.length
              ? [{ kind: "primary", parts: availablePrimary }]
              : [],
          })}
          className={chipClass}
        >
          Input only
        </button>
        {step.phase === "sequential" && (
          <button type="button" onClick={() => onChange(reportsOnly())} className={chipClass}>
            Prior reports
          </button>
        )}
        <button type="button" onClick={() => onChange({ include: [] })} className={chipClass}>
          Isolated
        </button>
      </div>

      <div className="rounded-md border border-gray-200 dark:border-gray-700 divide-y divide-gray-100 dark:divide-gray-800">
        <div className="p-2">
          <p className="text-[10px] font-medium text-gray-600 dark:text-gray-300 mb-1">Primary input</p>
          {availablePrimary.length ? (
            <div className="flex flex-wrap gap-x-3 gap-y-1">
              {([
                ["text", "Readable text"],
                ["structure", "Document structure"],
                ["visuals", "Pages & figures"],
                ["source", "Original source"],
              ] as Array<[PrimaryArtifactPart, string]>)
                .filter(([part]) => availablePrimary.includes(part))
                .map(([part, label]) => (
                  <label key={part} className="flex items-center gap-1 text-[10px] text-gray-600 dark:text-gray-300">
                    <input
                      type="checkbox"
                      className={checkboxClass}
                      checked={primary?.parts.includes(part) ?? false}
                      onChange={(event) => setPrimaryPart(part, event.target.checked)}
                    />
                    {label}
                  </label>
                ))}
            </div>
          ) : (
            <p className="text-[10px] text-gray-600 dark:text-gray-400">This profile has no primary input.</p>
          )}
        </div>

        <label className="flex items-center gap-2 p-2 text-[10px] text-gray-600 dark:text-gray-300">
          <input
            type="checkbox"
            className={checkboxClass}
            checked={hasSurvey}
            disabled={!surveyEnabled}
            onChange={(event) => setSurvey(event.target.checked)}
          />
          Survey / orientation JSON
          {!surveyEnabled && <span className="text-gray-600 dark:text-gray-400">(disabled for profile)</span>}
        </label>

        {namedInputs.map((slot) => {
          const selector = include.find(
            (item): item is Extract<ArtifactSelector, { kind: "named_input" }> =>
              item.kind === "named_input" && item.key === slot.key,
          );
          return (
            <div key={slot.key} className="p-2">
              <p className="text-[10px] font-medium text-gray-600 dark:text-gray-300 mb-1">
                {slot.label || slot.key} <span className="font-mono text-gray-600 dark:text-gray-400">({slot.key})</span>
              </p>
              <div className="flex gap-3">
                {([
                  ["text", "Extracted text"],
                  ["source", "Original source"],
                ] as Array<[NamedInputArtifactPart, string]>).map(([part, label]) => (
                  <label key={part} className="flex items-center gap-1 text-[10px] text-gray-600 dark:text-gray-300">
                    <input
                      type="checkbox"
                      className={checkboxClass}
                      checked={selector?.parts.includes(part) ?? false}
                      onChange={(event) => setNamedPart(slot.key, part, event.target.checked)}
                    />
                    {label}
                  </label>
                ))}
              </div>
            </div>
          );
        })}

        {step.phase === "parallel" && otherSteps.length > 0 && (
          <p className="p-2 text-[10px] text-gray-600 dark:text-gray-400">
            Step reports and supporting files become selectable only in Sequential steps.
          </p>
        )}

        {step.phase === "sequential" && otherSteps.map((producer) => {
          const selector = include.find(
            (item): item is Extract<ArtifactSelector, { kind: "step" }> =>
              item.kind === "step" && item.step === producer.id,
          );
          return (
            <div key={producer.id} className="p-2">
              <div className="flex items-center justify-between gap-2">
                <p className="text-[10px] font-medium text-gray-600 dark:text-gray-300 truncate">
                  {producer.label} <span className="font-mono text-gray-600 dark:text-gray-400">({producer.id})</span>
                </p>
                <div className="flex gap-3 shrink-0">
                  {([
                    ["report", "Report"],
                    ["files", "Files"],
                  ] as Array<[StepArtifactPart, string]>).map(([part, label]) => (
                    <label key={part} className="flex items-center gap-1 text-[10px] text-gray-600 dark:text-gray-300">
                      <input
                        type="checkbox"
                        className={checkboxClass}
                        checked={selector?.parts.includes(part) ?? false}
                        onChange={(event) => setStepPart(producer.id, part, event.target.checked)}
                      />
                      {label}
                    </label>
                  ))}
                </div>
              </div>
              {selector?.parts.includes("files") && (
                <input
                  type="text"
                  aria-label={`File filter for ${producer.label}`}
                  value={selector.glob ?? ""}
                  onChange={(event) => setStepGlob(producer.id, event.target.value)}
                  placeholder="All files, or filter with a glob such as **/*.csv"
                  className="mt-1.5 w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-[10px] font-mono text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200"
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// Artifact access is the core dataflow control. Conditions, output contracts,
// and fan-out remain advanced execution rules in the same compact panel.

/** Schema that makes a step emit a list of issues, enabling the Issues table +
 *  annotations in the report view (Release 1.4). Pair it with the issues
 *  synthesis prompt. */
const ISSUES_SCHEMA = {
  type: "object",
  required: ["issues"],
  properties: {
    issues: {
      type: "array",
      items: { type: "object", required: ["title", "severity", "body"] },
    },
  },
};

function AdvancedStepOptions({
  step,
  otherSteps,
  defaultReportStepIds,
  namedInputs,
  inputMode,
  surveyEnabled,
  conditionStepIds,
  onChange,
}: {
  step: StepConfig;
  otherSteps: Array<Pick<StepConfig, "id" | "label">>;
  defaultReportStepIds: string[];
  namedInputs: InputSlot[];
  inputMode: string;
  surveyEnabled: boolean;
  conditionStepIds: string[];
  onChange: (patch: Partial<StepConfig>) => void;
}) {
  const hasAny = !!(step.context.include.length || step.after?.length || step.run_if || step.output_schema);
  const [open, setOpen] = useState(!!(step.run_if || step.output_schema));
  const [schemaText, setSchemaText] = useState(
    step.output_schema ? JSON.stringify(step.output_schema, null, 2) : ""
  );
  const [schemaError, setSchemaError] = useState<string | null>(null);

  const cond = step.run_if ?? null;
  const condKind = cond?.kind ?? "none";

  const setCondKind = (kind: string) => {
    if (kind === "none") return onChange({ run_if: null });
    if (kind === "output_matches")
      return onChange({ run_if: { kind: "output_matches", step: conditionStepIds[0] ?? "", pattern: "" } });
    return onChange({ run_if: { kind: "survey_path", pointer: "", exists: true } });
  };

  const applySchema = (text: string) => {
    setSchemaText(text);
    if (!text.trim()) {
      setSchemaError(null);
      onChange({ output_schema: null });
      return;
    }
    try {
      const parsed = JSON.parse(text);
      const error = outputSchemaError(parsed);
      if (error) {
        setSchemaError(error);
        return;
      }
      setSchemaError(null);
      onChange({ output_schema: parsed });
    } catch (e) {
      setSchemaError(e instanceof Error ? e.message : "invalid JSON");
    }
  };

  const inputClass =
    "w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs font-mono text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-1 focus:ring-gray-400";

  return (
    <div>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="text-[11px] text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 flex items-center gap-1"
      >
        <span>{open ? "▾" : "▸"}</span>
        <span>Artifact access &amp; execution rules</span>
        {hasAny && !open && <span className="text-[10px] text-gray-600 dark:text-gray-400 ml-1">set</span>}
      </button>
      {open && (
        <div className="mt-2 space-y-3 pl-3 border-l-2 border-gray-200 dark:border-gray-700">
          <ArtifactContextEditor
            step={step}
            otherSteps={otherSteps}
            defaultReportStepIds={defaultReportStepIds}
            namedInputs={namedInputs}
            inputMode={inputMode}
            surveyEnabled={surveyEnabled}
            onChange={(context) => onChange({ context })}
          />

          {/* Order-only dependencies */}
          <div>
            <label className="block text-[10px] font-medium text-gray-500 mb-0.5">
              Wait for (order only)
            </label>
            <p className="text-[10px] text-gray-600 dark:text-gray-400 mb-1">
              Adds timing constraints without exposing the producer's artifacts.
            </p>
            {otherSteps.length ? (
              <div className="flex flex-wrap gap-1">
                {otherSteps.map((producer) => {
                  const checked = (step.after ?? []).includes(producer.id);
                  return (
                    <label
                      key={producer.id}
                      className={`flex items-center gap-1 text-[10px] px-2 py-1 rounded-md border cursor-pointer ${
                        checked
                          ? "border-gray-500 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-200"
                          : "border-gray-200 dark:border-gray-700 text-gray-500"
                      }`}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={(event) => onChange({
                          after: event.target.checked
                            ? [...(step.after ?? []), producer.id]
                            : (step.after ?? []).filter((id) => id !== producer.id),
                        })}
                        className="rounded border-gray-300 dark:border-gray-600"
                      />
                      {producer.label}
                    </label>
                  );
                })}
              </div>
            ) : (
              <p className="text-[10px] text-gray-600 dark:text-gray-400">No other steps.</p>
            )}
          </div>

          {/* run_if condition */}
          <div>
            <label className="block text-[10px] font-medium text-gray-500 mb-0.5">
              Run only if…
            </label>
            <select
              aria-label="Run condition"
              value={condKind}
              onChange={(e) => setCondKind(e.target.value)}
              className={inputClass}
            >
              <option value="none">Always run</option>
              <option value="output_matches" disabled={conditionStepIds.length === 0}>
                An upstream step's output matches a pattern
              </option>
              <option value="survey_path">The survey (orientation) JSON matches</option>
            </select>
            {cond?.kind === "output_matches" && (
              <div className="mt-1.5 space-y-1.5">
                <select
                  aria-label="Condition source step"
                  value={cond.step}
                  onChange={(e) => onChange({ run_if: { ...cond, step: e.target.value } })}
                  className={inputClass}
                >
                  {!conditionStepIds.includes(cond.step) && cond.step && (
                    <option value={cond.step} disabled>{cond.step} (not upstream)</option>
                  )}
                  {conditionStepIds.map((id) => (
                    <option key={id} value={id}>{id}</option>
                  ))}
                </select>
                <input
                  type="text"
                  aria-label="Condition regular expression"
                  value={cond.pattern}
                  onChange={(e) => onChange({ run_if: { ...cond, pattern: e.target.value } })}
                  placeholder="regular expression, e.g. SEVERITY:\s*high"
                  className={inputClass}
                />
                <label className="flex items-center gap-1.5 text-[10px] text-gray-500">
                  <input
                    type="checkbox"
                    checked={!!cond.negate}
                    onChange={(e) => onChange({ run_if: { ...cond, negate: e.target.checked } })}
                  />
                  Invert (run when it does NOT match)
                </label>
              </div>
            )}
            {cond?.kind === "survey_path" && (
              <div className="mt-1.5 space-y-1.5">
                <input
                  type="text"
                  aria-label="Survey JSON pointer"
                  value={cond.pointer}
                  onChange={(e) => onChange({ run_if: { ...cond, pointer: e.target.value } })}
                  placeholder="JSON pointer, e.g. /metadata/paper_type"
                  className={inputClass}
                />
                <input
                  type="text"
                  aria-label="Survey value to equal"
                  value={
                    cond.equals === undefined
                      ? ""
                      : typeof cond.equals === "string"
                        ? cond.equals
                        : JSON.stringify(cond.equals)
                  }
                  onChange={(e) => {
                    const raw = e.target.value;
                    if (!raw) {
                      const { equals: _drop, ...rest } = cond;
                      onChange({ run_if: { ...rest, exists: true } });
                      return;
                    }
                    let val: unknown = raw;
                    try {
                      val = JSON.parse(raw);
                    } catch {
                      /* keep as string */
                    }
                    onChange({ run_if: { kind: "survey_path", pointer: cond.pointer, equals: val } });
                  }}
                  placeholder='equals (optional), e.g. "empirical" or true'
                  className={inputClass}
                />
                <p className="text-[10px] text-gray-600 dark:text-gray-400">
                  Leave "equals" blank to require only that the pointer exists.
                </p>
              </div>
            )}
          </div>

          {/* output_schema */}
          <div>
            <div className="flex items-center justify-between mb-0.5">
              <label className="block text-[10px] font-medium text-gray-500">
                Output JSON schema (optional)
              </label>
              <button
                type="button"
                onClick={() => applySchema(JSON.stringify(ISSUES_SCHEMA, null, 2))}
                className="text-[10px] text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100 underline"
                title="Fill with the issues schema, which enables the Issues table + annotations in the report view"
              >
                Use issues schema
              </button>
            </div>
            <textarea
              aria-label="Output JSON schema"
              value={schemaText}
              onChange={(e) => applySchema(e.target.value)}
              rows={4}
              placeholder='{ "type": "array", "items": { "type": "object", "required": ["id", "severity"] } }'
              className={`${inputClass} resize-y`}
            />
            {schemaError ? (
              <p className="text-[10px] text-red-600 dark:text-red-400 mt-0.5">Invalid schema: {schemaError}</p>
            ) : (
              <p className="text-[10px] text-gray-600 dark:text-gray-400 mt-0.5">
                When set, the step must emit JSON matching this shape (with one retry).
              </p>
            )}
          </div>

          {/* Fan-out (map) */}
          <div>
            <label className="flex items-center gap-1.5 text-[10px] font-medium text-gray-500 mb-1">
              <input
                type="checkbox"
                checked={!!step.for_each}
                onChange={(e) =>
                  onChange({ for_each: e.target.checked ? { glob: "*", max: 20 } : null })
                }
              />
              Fan out (run once per matching file)
            </label>
            {step.for_each && (
              <div className="space-y-1.5 pl-4">
                <input
                  type="text"
                  aria-label="Fan-out file pattern"
                  value={step.for_each.glob}
                  onChange={(e) => onChange({ for_each: { glob: e.target.value, max: step.for_each!.max } })}
                  placeholder="glob, e.g. chapters/*.tex or **/*.py"
                  className={inputClass}
                />
                <label className="flex items-center gap-2 text-[10px] text-gray-500">
                  Max files
                  <input
                    type="number"
                    min={1}
                    value={step.for_each.max}
                    onChange={(e) =>
                      onChange({ for_each: { glob: step.for_each!.glob, max: Math.max(1, parseInt(e.target.value, 10) || 1) } })
                    }
                    className={`${inputClass} w-20`}
                  />
                </label>
                <p className="text-[10px] text-gray-600 dark:text-gray-400">
                  Bind <code className="font-mono">{"{item}"}</code> in the prompt to each file. Outputs
                  are merged (enable merge) or read together downstream via{" "}
                  <code className="font-mono">{"{step:" + step.id + "}"}</code>.
                </p>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

// --- Agent selector ---

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
        <span className="font-normal text-gray-600 dark:text-gray-400 ml-1">
          (empty = global setting{multi ? "; select multiple to run in parallel" : ""})
        </span>
      </label>
      <div
        role="group"
        aria-label={multi ? "LLM agents (multiple allowed)" : "LLM agent"}
        className="flex gap-2"
      >
        {PROVIDERS.map((provider) => {
          const active = agents.includes(provider);
          return (
            <button
              key={provider}
              type="button"
              aria-pressed={active}
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
  const titleId = useId();
  const inputId = useId();
  const dialogRef = useModalDialog<HTMLDivElement>(onCancel);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className="bg-white dark:bg-gray-800 rounded-xl shadow-xl w-80 p-5"
      >
        <p id={titleId} className="text-sm font-medium text-gray-900 dark:text-gray-100 mb-3">{title}</p>
        <label htmlFor={inputId} className="sr-only">{title}</label>
        <input
          id={inputId}
          data-autofocus
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
            type="button"
            onClick={onCancel}
            className="py-1.5 px-3 text-sm text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
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

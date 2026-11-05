import { workflowProfileClient } from "../../lib/workflowProfileClient";
import { useState, useEffect, useRef } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  PipelineConfig,
  StepConfig,
  ProfileSummary,
  ExportEnvelope,
  Settings,
} from "../../lib/types";
import { DEFAULT_PROFILE_ID } from "../../lib/autoReview";
import {
  forgetWorkflowDraft,
  workflowDraftKey,
} from "../../lib/workflowDraftRecovery";

import { confirmDialog, notify } from "../DialogService";

import { uniqueStepId } from "./editorState";
import { normalizeConfig } from "./utils";

import type { WorkflowEditor } from "./useWorkflowEditor";
export function useProfileOperations(
  editor: WorkflowEditor,
  settings: Settings | null,
  /** Called after any change to the active workflow. A save passes the saved
   *  configuration, so the run setup can keep an input that still fits. */
  onProfileChange?: (config?: PipelineConfig) => void,
) {
  const {
    config,
    activeProfile,
    dirty,
    editingStep,
    setConfig,
    setActiveProfile,
    setEditing,
    setDirty,
    setSchemaDraftValid,
    setSchemaEditorEpoch,
    replaceDraft,
  } = editor;
  const { configRef, activeProfileRef, savedConfigRef } = editor.identity;
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [profileMutationPending, setProfileMutationPending] = useState(false);
  const profileMutationRequestRef = useRef(0);
  const profileMutationActiveRef = useRef(false);
  const [promptDialog, setPromptDialog] = useState<{
    title: string;
    defaultValue: string;
    onSubmit: (value: string) => void;
  } | null>(null);
  useEffect(
    () => () => {
      profileMutationRequestRef.current += 1;
      profileMutationActiveRef.current = false;
    },
    [],
  );

  // The stored draft of a workflow, if it differs from the saved one and the
  // researcher wants it back.
  const offerStoredDraft = async (
    profile: string,
    saved: PipelineConfig,
  ): Promise<PipelineConfig | null> => {
    const draftKey = workflowDraftKey(profile);
    let storedDraft: string | null = null;
    try {
      storedDraft = window.localStorage.getItem(draftKey);
    } catch {
      editor.disableRecoveryCache();
    }
    if (!storedDraft) return null;
    try {
      const candidate = normalizeConfig(
        JSON.parse(storedDraft) as PipelineConfig,
      );
      if (
        JSON.stringify(candidate) !== JSON.stringify(saved) &&
        (await confirmDialog(
          "Pipeline found an unsaved draft for this review. Restore it?",
          {
            title: "Recover review draft",
            confirmLabel: "Restore draft",
          },
        ))
      ) {
        return candidate;
      }
    } catch {
      try {
        window.localStorage.removeItem(draftKey);
      } catch {
        editor.disableRecoveryCache();
      }
    }
    return null;
  };

  useEffect(() => {
    let live = true;
    setLoading(true);
    setLoadError(null);
    Promise.all([
      workflowProfileClient.config(),
      workflowProfileClient.profiles(),
      workflowProfileClient.active(),
    ])
      .then(async ([c, p, a]) => {
        if (!live) return;
        const normalized = normalizeConfig(c);
        const draft = await offerStoredDraft(a, normalized);
        if (draft) setDirty(true);
        if (!live) return;
        replaceDraft(draft ?? normalized, normalized, a, false);
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
      const p = await workflowProfileClient.profiles();
      if (request !== undefined && !profileMutationIsCurrent(request)) return;
      setProfiles(p);
    } catch (e) {
      if (request === undefined || profileMutationIsCurrent(request)) {
        console.error(e);
      }
    }
  };

  const switchProfileForMutation = async (id: string, request: number) => {
    const previous = activeProfileRef.current;
    const newConfig = await workflowProfileClient.switch({ id });
    if (!profileMutationIsCurrent(request)) return false;
    const normalized = normalizeConfig(newConfig);
    const draft = await offerStoredDraft(id, normalized);
    if (!profileMutationIsCurrent(request)) return false;
    // Every caller confirmed leaving the previous workflow and its edits, so
    // its recovery copy must not come back on the next visit.
    if (previous !== id) forgetWorkflowDraft(previous);
    replaceDraft(draft ?? normalized, normalized, id, true);
    if (draft) setDirty(true);
    onProfileChange?.();
    return true;
  };

  // --- Profile management ---

  const handleSwitchProfile = async (id: string) => {
    if (id === activeProfileRef.current) return;
    if (
      dirty &&
      !(await confirmDialog(
        "You have unsaved changes. Switch review and discard them?",
        { destructive: true },
      ))
    )
      return;
    const request = beginProfileMutation();
    if (request === null) return;
    try {
      await switchProfileForMutation(id, request);
    } catch (e) {
      if (profileMutationIsCurrent(request)) {
        notify(
          `Failed to switch review: ${e instanceof Error ? e.message : String(e)}`,
        );
      }
    } finally {
      finishProfileMutation(request);
    }
  };

  const handleNewProfile = () => {
    setPromptDialog({
      title: "New review name",
      defaultValue: "",
      onSubmit: async (name) => {
        if (
          dirty &&
          !(await confirmDialog(
            "Create this review and discard the current unsaved changes?",
            { destructive: true },
          ))
        ) {
          return;
        }
        const request = beginProfileMutation();
        if (request === null) return;
        try {
          const summary = await workflowProfileClient.create({
            name,
          });
          if (!profileMutationIsCurrent(request)) return;
          await refreshProfiles(request);
          await switchProfileForMutation(summary.id, request);
        } catch (e) {
          if (profileMutationIsCurrent(request)) {
            notify(
              `Failed to create review: ${e instanceof Error ? e.message : String(e)}`,
            );
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
        if (
          dirty &&
          !(await confirmDialog(
            "Duplicate this review and discard the current unsaved changes?",
            { destructive: true },
          ))
        ) {
          return;
        }
        const request = beginProfileMutation();
        if (request === null) return;
        const sourceId = activeProfileRef.current;
        try {
          const summary = await workflowProfileClient.duplicate({
            sourceId,
            newName: name,
          });
          if (!profileMutationIsCurrent(request)) return;
          await refreshProfiles(request);
          await switchProfileForMutation(summary.id, request);
        } catch (e) {
          if (profileMutationIsCurrent(request)) {
            notify(
              `Failed to duplicate review: ${e instanceof Error ? e.message : String(e)}`,
            );
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
      title: "Rename review",
      defaultValue: current?.name ?? "",
      onSubmit: async (name) => {
        const request = beginProfileMutation();
        if (request === null) return;
        try {
          await workflowProfileClient.rename({
            id: profile,
            newName: name,
          });
          if (
            !profileMutationIsCurrent(request) ||
            activeProfileRef.current !== profile
          )
            return;
          await refreshProfiles(request);
        } catch (e) {
          if (profileMutationIsCurrent(request)) {
            notify(
              `Failed to rename review: ${e instanceof Error ? e.message : String(e)}`,
            );
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
      notify("Built-in reviews cannot be deleted.", "info");
      return;
    }
    const current = profiles.find((candidate) => candidate.id === profile);
    if (
      !(await confirmDialog(
        `Delete review “${current?.name ?? profile}”? This cannot be undone.`,
        { title: "Delete review", confirmLabel: "Delete", destructive: true },
      ))
    )
      return;
    const request = beginProfileMutation();
    if (request === null) return;
    try {
      await workflowProfileClient.remove({ id: profile });
      if (!profileMutationIsCurrent(request)) return;
      await refreshProfiles(request);
      await switchProfileForMutation(DEFAULT_PROFILE_ID, request);
    } catch (e) {
      if (profileMutationIsCurrent(request)) {
        notify(
          `Failed to delete review: ${e instanceof Error ? e.message : String(e)}`,
        );
      }
    } finally {
      finishProfileMutation(request);
    }
  };

  const persistCurrentConfig = async (): Promise<boolean> => {
    if (!config) return false;
    const configToSave = config;
    const profileToSave = activeProfile;
    const savedSnapshot = JSON.stringify(configToSave);
    setSaving(true);
    setSaved(false);
    try {
      await workflowProfileClient.save({
        config: configToSave,
        profileId: profileToSave,
      });
      const isCurrentVersion =
        activeProfileRef.current === profileToSave &&
        JSON.stringify(configRef.current) === savedSnapshot;
      setSaved(isCurrentVersion);
      if (isCurrentVersion) {
        savedConfigRef.current = configToSave;
        setDirty(false);
      }
      await refreshProfiles();
      onProfileChange?.(
        activeProfileRef.current === profileToSave ? configToSave : undefined,
      );
      if (isCurrentVersion) setTimeout(() => setSaved(false), 2000);
      return isCurrentVersion;
    } catch (e) {
      notify(`Failed to save: ${e instanceof Error ? e.message : String(e)}`);
      return false;
    } finally {
      setSaving(false);
    }
  };

  const handleSave = async () => {
    await persistCurrentConfig();
  };

  const saveBeforeExport = async (what: string): Promise<boolean> => {
    if (!dirty) return true;
    if (
      !(await confirmDialog(
        `Exporting ${what} requires saving this review's unsaved edits first. Save and continue?`,
      ))
    ) {
      return false;
    }
    const currentSaved = await persistCurrentConfig();
    if (!currentSaved) {
      notify(
        "Export cancelled because the review changed while it was being saved.",
        "info",
      );
    }
    return currentSaved;
  };

  const handleReset = async () => {
    if (
      !(await confirmDialog(
        "Reset this review to defaults? All customizations will be lost.",
        { title: "Reset review", confirmLabel: "Reset", destructive: true },
      ))
    )
      return;
    const request = beginProfileMutation();
    if (request === null) return;
    const profile = activeProfileRef.current;
    try {
      const d = await workflowProfileClient.reset();
      if (
        !profileMutationIsCurrent(request) ||
        activeProfileRef.current !== profile
      )
        return;
      setConfig(normalizeConfig(d));
      setEditing(null);
      setDirty(false);
      savedConfigRef.current = normalizeConfig(d);
      setSchemaDraftValid(true);
      setSchemaEditorEpoch((current) => current + 1);
      await refreshProfiles(request);
      if (
        !profileMutationIsCurrent(request) ||
        activeProfileRef.current !== profile
      )
        return;
      onProfileChange?.();
    } catch (e) {
      if (profileMutationIsCurrent(request)) {
        notify(
          `Failed to reset: ${e instanceof Error ? e.message : String(e)}`,
        );
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
      await workflowProfileClient.exportItem({
        json: JSON.stringify(envelope, null, 2),
        suggestedName,
      });
    } catch (e) {
      notify(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleExportProfile = async () => {
    const suggestedName = `pipeline-profile-${activeProfile}.json`;
    try {
      if (!(await saveBeforeExport("the review"))) return;
      await workflowProfileClient.exportProfile({
        id: activeProfile,
        suggestedName,
      });
    } catch (e) {
      notify(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleExportBundle = async () => {
    try {
      if (!(await saveBeforeExport("the settings bundle"))) return;
      await workflowProfileClient.exportBundle({
        suggestedName: "pipeline-settings-backup.json",
      });
    } catch (e) {
      notify(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleImport = async () => {
    if (!config) return;
    try {
      const path = await openDialog({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      const envelope = await workflowProfileClient.importItem({ path });
      switch (envelope.type) {
        case "step": {
          const imported = envelope.data as StepConfig;
          const step: StepConfig = {
            ...imported,
            after: imported.after ?? [],
            context: imported.context ?? { include: [] },
          };
          const agents = step.agents?.length || 1;
          const fanOut = step.for_each
            ? `; fan-out up to ${step.for_each.max} items`
            : "";
          const logicalCalls = (step.for_each?.max ?? 1) * agents;
          const attempts = logicalCalls * ((settings?.max_retries ?? 0) + 1);
          if (
            !(await confirmDialog(
              `Import step “${step.label}”?\n\n` +
                `Tools: ${step.tools?.join(", ") || "none"}\n` +
                `Agents: ${step.agents?.join(", ") || "review default"}${fanOut}\n` +
                `Maximum provider attempts from this step: ${attempts}`,
            ))
          )
            return;
          const id = uniqueStepId(step.id, config.steps);
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
          const tools = [
            ...new Set(enabled.flatMap((step) => step.tools ?? [])),
          ];
          const agents = [
            ...new Set(enabled.flatMap((step) => step.agents ?? [])),
          ];
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
          const maxAttempts =
            logicalCalls * ((settings?.max_retries ?? 0) + 1) + mergeCalls;
          if (
            !(await confirmDialog(
              `Import and activate review “${envelope.name}”?\n\n` +
                `${enabled.length} enabled steps; up to ${maxAttempts} provider attempts per report (including retries and merges).\n` +
                `Tools: ${tools.join(", ") || "none"}\n` +
                `Agents: ${agents.join(", ") || "review default"}\n\n` +
                "Check the imported prompts in the designer before you start the review.",
            ))
          )
            return;
          const summary = await workflowProfileClient.importProfile({
            path,
          });
          await refreshProfiles();
          await handleSwitchProfile(summary.id);
          break;
        }
        case "bundle": {
          if (
            !(await confirmDialog(
              `Import ${envelope.profiles.length} reviews plus provider/settings configuration?\n\n` +
                "Existing reviews with the same ID will be overwritten. API-key fields and the active review may change. Check the active review before running it.",
            ))
          )
            return;
          const request = beginProfileMutation();
          if (request === null) return;
          try {
            await workflowProfileClient.importBundle({ path });
            const [c, p, a] = await Promise.all([
              workflowProfileClient.config(),
              workflowProfileClient.profiles(),
              workflowProfileClient.active(),
            ]);
            if (!profileMutationIsCurrent(request)) return;
            const normalized = normalizeConfig(c);
            const draft = await offerStoredDraft(a, normalized);
            if (!profileMutationIsCurrent(request)) return;
            replaceDraft(draft ?? normalized, normalized, a, true);
            if (draft) setDirty(true);
            setProfiles(p);
            onProfileChange?.();
          } finally {
            finishProfileMutation(request);
          }
          break;
        }
      }
    } catch (e) {
      notify(`Import failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  return {
    loading,
    loadError,
    retryLoad: () => setLoadAttempt((attempt) => attempt + 1),
    saving,
    saved,
    profiles,
    profileMutationPending,
    promptDialog,
    setPromptDialog,
    handleSwitchProfile,
    handleNewProfile,
    handleDuplicateProfile,
    handleRenameProfile,
    handleDeleteProfile,
    handleSave,
    handleReset,
    handleExportItem,
    handleExportProfile,
    handleExportBundle,
    handleImport,
  };
}
export type ProfileOperations = ReturnType<typeof useProfileOperations>;

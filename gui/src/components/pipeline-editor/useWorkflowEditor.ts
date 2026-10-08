import { useState, useEffect, useRef } from "react";

import type { PipelineConfig } from "../../lib/types";

import { type WaveSelection } from "../WaveDiagram";

import { conditionUpstreamIds } from "./utils";

type EditingMode = WaveSelection | null;
const RECOVERY_WARNING =
  "Draft recovery storage is unavailable. Your edits remain open; save or export this workflow before leaving.";
export function useWorkflowEditor(onDirtyChange?: (dirty: boolean) => void) {
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [recoveryWarning, setRecoveryWarning] = useState("");
  const recoveryDisabled = useRef(false);
  const disableRecoveryCache = () => {
    recoveryDisabled.current = true;
    setRecoveryWarning(RECOVERY_WARNING);
  };
  const [dirty, setDirty] = useState(false);
  const [schemaDraftValid, setSchemaDraftValid] = useState(true);
  const [schemaEditorEpoch, setSchemaEditorEpoch] = useState(0);
  const [editing, setEditing] = useState<EditingMode>(null);
  const [activeProfile, setActiveProfile] = useState<string>("auto-review");
  const [, setHistoryVersion] = useState(0);
  const configRef = useRef<PipelineConfig | null>(config);
  const activeProfileRef = useRef(activeProfile);
  const promptResetRequestRef = useRef(0);
  const undoHistoryRef = useRef<PipelineConfig[]>([]);
  const redoHistoryRef = useRef<PipelineConfig[]>([]);
  const lastConfigRef = useRef<PipelineConfig | null>(null);
  const savedConfigRef = useRef<PipelineConfig | null>(null);
  const applyingHistoryRef = useRef(false);
  configRef.current = config;
  activeProfileRef.current = activeProfile;
  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);

  useEffect(() => () => onDirtyChange?.(false), [onDirtyChange]);

  useEffect(() => {
    if (!config) return;
    if (applyingHistoryRef.current) {
      applyingHistoryRef.current = false;
      lastConfigRef.current = config;
      return;
    }
    const previous = lastConfigRef.current;
    if (previous && JSON.stringify(previous) !== JSON.stringify(config)) {
      undoHistoryRef.current = [...undoHistoryRef.current.slice(-99), previous];
      redoHistoryRef.current = [];
      setHistoryVersion((version) => version + 1);
    }
    lastConfigRef.current = config;
  }, [config]);

  useEffect(() => {
    // Loading must inspect recovery storage before any save/discard effect.
    if (!config) return;
    if (recoveryDisabled.current) return;
    const key = `pipeline.workflowDraft.${activeProfile}`;
    try {
      if (dirty) window.localStorage.setItem(key, JSON.stringify(config));
      else window.localStorage.removeItem(key);
      setRecoveryWarning("");
    } catch {
      setRecoveryWarning(RECOVERY_WARNING);
    }
  }, [activeProfile, config, dirty]);

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

  const editingStep =
    config && isStepEditing
      ? (config.steps.find((s) => s.id === editing) ?? null)
      : null;
  const conditionStepIds =
    config && editingStep
      ? conditionUpstreamIds(config.steps, editingStep.id)
      : [];

  useEffect(() => {
    if (isStepEditing && !editingStep) {
      setEditing(null);
    }
  }, [isStepEditing, editingStep]);

  const undo = () => {
    const previous = undoHistoryRef.current.pop();
    if (!previous || !configRef.current) return;
    redoHistoryRef.current.push(configRef.current);
    setHistoryVersion((version) => version + 1);
    applyingHistoryRef.current = true;
    setConfig(previous);
    setDirty(
      JSON.stringify(previous) !== JSON.stringify(savedConfigRef.current),
    );
  };

  const redo = () => {
    const next = redoHistoryRef.current.pop();
    if (!next || !configRef.current) return;
    undoHistoryRef.current.push(configRef.current);
    setHistoryVersion((version) => version + 1);
    applyingHistoryRef.current = true;
    setConfig(next);
    setDirty(JSON.stringify(next) !== JSON.stringify(savedConfigRef.current));
  };

  // Loading and switching replace the draft and its history as one transition.
  const replaceDraft = (
    draft: PipelineConfig,
    saved: PipelineConfig,
    profile: string,
    resetSelection: boolean,
  ) => {
    applyingHistoryRef.current = true;
    undoHistoryRef.current = [];
    redoHistoryRef.current = [];
    setHistoryVersion((version) => version + 1);
    lastConfigRef.current = draft;
    savedConfigRef.current = saved;
    setConfig(draft);
    setActiveProfile(profile);
    if (resetSelection) {
      setEditing(null);
      setSchemaDraftValid(true);
      setSchemaEditorEpoch((current) => current + 1);
      setDirty(false);
    }
  };
  return {
    config,
    recoveryWarning,
    disableRecoveryCache,
    activeProfile,
    dirty,
    schemaDraftValid,
    schemaEditorEpoch,
    editing,
    editingStep,
    conditionStepIds,
    setConfig,
    setActiveProfile,
    setDirty,
    setEditing,
    setSchemaDraftValid,
    setSchemaEditorEpoch,
    replaceDraft,
    identity: {
      configRef,
      activeProfileRef,
      promptResetRequestRef,
      savedConfigRef,
    },
    history: {
      undo,
      redo,
      canUndo: undoHistoryRef.current.length > 0,
      canRedo: redoHistoryRef.current.length > 0,
    },
  };
}
export type WorkflowEditor = ReturnType<typeof useWorkflowEditor>;

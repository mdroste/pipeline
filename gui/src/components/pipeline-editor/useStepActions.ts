import { invoke } from "@tauri-apps/api/core";

import type {
  Phase,
  StepConfig,
  MergeConfig,
  ExtractionConfig,
  ArtifactSelector,
} from "../../lib/types";

import { confirmDialog, notify } from "../DialogService";

import { type AddStepDraft } from "./AddStepDialog";

import { defaultStepContext, primaryPartsForMode } from "./utils";

import { ISSUES_SCHEMA } from "./stepTemplates";

import type { WorkflowEditor } from "./useWorkflowEditor";
import {
  prepareExtractionChange,
  referencingSteps,
  withoutStepReferences,
} from "./editorState";
export function useStepActions(
  editor: WorkflowEditor,
  onStepAdded: () => void,
) {
  const { config, editing, setConfig, setDirty, setEditing } = editor;
  const { activeProfileRef, promptResetRequestRef } = editor.identity;
  const updateStep = (id: string, patch: Partial<StepConfig>) => {
    setConfig((current) => {
      if (!current) return current;
      const outputs = { ...(current.outputs ?? {}) };
      if (patch.output_schema === null && outputs.findings_step === id) {
        outputs.findings_step = "";
      }
      return {
        ...current,
        outputs,
        steps: current.steps.map((step) =>
          step.id === id ? { ...step, ...patch } : step,
        ),
      };
    });
    setDirty(true);
  };

  const updateStepPhase = (id: string, phase: Phase) => {
    setConfig((current) => {
      if (!current) return current;
      const outputs = { ...(current.outputs ?? {}) };
      if (phase !== "sequential") {
        if (outputs.primary_step === id) outputs.primary_step = "";
        if (outputs.findings_step === id) outputs.findings_step = "";
      }
      return {
        ...current,
        outputs,
        steps: current.steps.map((step) =>
          step.id === id ? { ...step, phase } : step,
        ),
      };
    });
    setDirty(true);
  };

  const updateOutputRole = (
    id: string,
    role: "primary_step" | "findings_step",
    enabled: boolean,
  ) => {
    setConfig((current) => {
      if (!current) return current;
      const outputs = { ...(current.outputs ?? {}) };
      outputs[role] = enabled ? id : outputs[role] === id ? "" : outputs[role];
      return {
        ...current,
        outputs,
        steps:
          role === "findings_step" && enabled
            ? current.steps.map((step) =>
                step.id === id && !step.output_schema
                  ? { ...step, output_schema: ISSUES_SCHEMA }
                  : step,
              )
            : current.steps,
      };
    });
    setDirty(true);
  };

  const updateMerge = (patch: Partial<MergeConfig>) => {
    setConfig((current) =>
      current
        ? {
            ...current,
            merge: { ...current.merge, ...patch },
          }
        : current,
    );
    setDirty(true);
  };

  const updateExtraction = async (patch: Partial<ExtractionConfig>) => {
    if (!config) return;
    const { config: nextConfig, removedSelectors } = prepareExtractionChange(
      config,
      patch,
    );
    if (
      removedSelectors > 0 &&
      !(await confirmDialog(
        `This change removes ${removedSelectors} artifact access rule${removedSelectors === 1 ? "" : "s"} from workflow steps. Continue?`,
        { destructive: true },
      ))
    ) {
      return;
    }
    setConfig(nextConfig);
    setDirty(true);
  };

  const updateStepEnabled = async (id: string, enabled: boolean) => {
    if (!config) return;
    const affected = enabled ? [] : referencingSteps(config, id);
    if (
      affected.length > 0 &&
      !(await confirmDialog(
        `Disabling this step removes dependencies or artifact access from ${affected.length} downstream step${affected.length === 1 ? "" : "s"}. Continue?`,
        { destructive: true },
      ))
    ) {
      return;
    }
    const nextConfig = {
      ...config,
      outputs: enabled
        ? config.outputs
        : {
            ...(config.outputs ?? {}),
            primary_step:
              config.outputs?.primary_step === id
                ? ""
                : config.outputs?.primary_step,
            findings_step:
              config.outputs?.findings_step === id
                ? ""
                : config.outputs?.findings_step,
          },
      steps: config.steps.map((step) => {
        if (step.id === id) return { ...step, enabled };
        if (enabled) return step;
        return withoutStepReferences(step, id);
      }),
    };
    setConfig(nextConfig);
    setDirty(true);
  };

  const resetParallelTemplate = async (source: "generic" | "paper") => {
    const request = ++promptResetRequestRef.current;
    const profile = activeProfileRef.current;
    try {
      const template =
        source === "generic"
          ? await invoke<string>("get_default_prompt", {
              name: "parallel_context_generic",
            })
          : await invoke<string>("get_default_parallel_template");
      if (
        request !== promptResetRequestRef.current ||
        activeProfileRef.current !== profile
      )
        return;
      setConfig((current) =>
        current ? { ...current, parallel_context_template: template } : current,
      );
      setDirty(true);
    } catch (error) {
      if (
        request === promptResetRequestRef.current &&
        activeProfileRef.current === profile
      ) {
        notify(
          `Failed to reset the parallel context template: ${
            error instanceof Error ? error.message : String(error)
          }`,
        );
      }
    }
  };

  const addStep = async (draft: AddStepDraft) => {
    if (!config) return;
    const inputMode = config.extraction?.input_mode || "document";
    let template: StepConfig | null = null;
    if (draft.mode === "adaptive") {
      if (!draft.templateId)
        throw new Error("Select an adaptive agent to copy.");
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
      if (
        draft.mode === "adaptive" ||
        draft.mode === "blank" ||
        draft.contextPreset === "standard"
      ) {
        return defaultStepContext(draft.phase, inputMode, true, config.steps);
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
      if (
        context.include.some(
          (selector) =>
            selector.kind === "step" && selector.parts.includes("report"),
        )
      ) {
        contextBlocks.push("PRIOR OUTPUTS:\n{prior_outputs}");
      }
      const outputInstruction =
        draft.output === "issues"
          ? "Populate the configured issues schema with the completed findings."
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
          output_schema:
            draft.mode === "guided" && draft.output === "issues"
              ? ISSUES_SCHEMA
              : null,
        };
    const insertionIndex =
      step.phase === "parallel"
        ? config.steps.findIndex(
            (candidate) => candidate.phase === "sequential",
          )
        : -1;
    const steps = config.steps.slice();
    steps.splice(insertionIndex < 0 ? steps.length : insertionIndex, 0, step);
    const publishesFindings =
      draft.mode === "guided" &&
      draft.output === "issues" &&
      step.phase === "sequential";
    setConfig({
      ...config,
      steps,
      outputs: publishesFindings
        ? { ...(config.outputs ?? {}), primary_step: id, findings_step: id }
        : config.outputs,
    });
    onStepAdded();
    setEditing(id);
    setDirty(true);
  };

  const removeStep = async (id: string) => {
    if (!config) return;
    const step = config.steps.find((candidate) => candidate.id === id);
    const affected = referencingSteps(config, id);
    const consequence =
      affected.length > 0
        ? ` It will also remove references from ${affected.length} downstream step${affected.length === 1 ? "" : "s"}.`
        : "";
    if (
      !(await confirmDialog(`Remove “${step?.label ?? id}”?${consequence}`, {
        title: "Remove step",
        confirmLabel: "Remove",
        destructive: true,
      }))
    )
      return;
    const nextConfig = {
      ...config,
      outputs: {
        ...(config.outputs ?? {}),
        primary_step:
          config.outputs?.primary_step === id
            ? ""
            : config.outputs?.primary_step,
        findings_step:
          config.outputs?.findings_step === id
            ? ""
            : config.outputs?.findings_step,
      },
      steps: config.steps
        .filter((step) => step.id !== id)
        .map((step) => withoutStepReferences(step, id)),
    };
    setConfig(nextConfig);
    if (editing === id) setEditing(null);
    setDirty(true);
  };

  const moveSequentialStep = (id: string, direction: -1 | 1) => {
    if (!config) return;
    const step = config.steps.find((candidate) => candidate.id === id);
    if (!step || step.phase !== "sequential") return;
    const peers = config.steps.filter(
      (candidate) => candidate.phase === "sequential",
    );
    const position = peers.findIndex((candidate) => candidate.id === id);
    const target = peers[position + direction];
    if (!target) return;
    const fromIndex = config.steps.findIndex(
      (candidate) => candidate.id === id,
    );
    const toIndex = config.steps.findIndex(
      (candidate) => candidate.id === target.id,
    );
    const steps = config.steps.slice();
    [steps[fromIndex], steps[toIndex]] = [steps[toIndex], steps[fromIndex]];
    setConfig({ ...config, steps });
    setDirty(true);
  };

  return {
    updateStep,
    updateStepPhase,
    updateOutputRole,
    updateMerge,
    updateExtraction,
    updateStepEnabled,
    resetParallelTemplate,
    addStep,
    removeStep,
    moveSequentialStep,
  };
}
export type StepActions = ReturnType<typeof useStepActions>;

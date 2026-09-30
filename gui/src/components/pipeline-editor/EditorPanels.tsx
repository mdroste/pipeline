import { memo, useEffect, useState } from "react";
import type { KeyboardEvent } from "react";
import type {
  MergeConfig,
  ModelCatalog,
  PipelineConfig,
  Phase,
  Settings,
  StepConfig,
  VarSpec,
} from "../../lib/types";
import PromptEditor from "../PromptEditor";
import AdvancedStepOptions from "./AdvancedStepOptions";
import AgentChips from "./AgentChips";
import ModelOverrides from "./ModelOverrides";
import { describeStep } from "./stepSummary";
import {
  MemoizedCalibrateSection as CalibrateSection,
  MemoizedVariablesEditor as VariablesEditor,
} from "./ProfileSettingsEditors";

export const MergeEditorPanel = memo(function MergeEditorPanel({
  merge,
  onChange,
}: {
  merge: MergeConfig;
  onChange: (patch: Partial<MergeConfig>) => void;
}) {
  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="p-4 border-b border-gray-200 dark:border-gray-700 space-y-3">
        <div className="flex items-center gap-3">
          <button
            type="button"
            role="switch"
            aria-label="Cross-agent merge"
            aria-checked={merge.enabled}
            onClick={() => onChange({ enabled: !merge.enabled })}
            className={`w-8 h-5 rounded-full relative transition-colors shrink-0 ${
              merge.enabled ? "bg-green-600" : "bg-gray-300 dark:bg-gray-600"
            }`}
          >
            <div
              className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${
                merge.enabled ? "translate-x-3.5" : "translate-x-0.5"
              }`}
            />
          </button>
          <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
            Cross-agent merge {merge.enabled ? "enabled" : "disabled"}
          </span>
        </div>
        <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
          When a step runs on multiple LLM agents, this merges their independent
          outputs into a single report. Only runs when needed.
        </p>
        <AgentChips
          agents={merge.agents ?? []}
          onChange={(agents) => onChange({ agents })}
        />
      </div>
      <div className="flex-1 min-h-0 p-4">
        <PromptEditor
          value={merge.prompt}
          onChange={(prompt) => onChange({ prompt })}
          context={{ kind: "merge" }}
          ariaLabel="Cross-agent merge prompt"
        />
      </div>
    </div>
  );
});

export const PipelineSettingsEditorPanel = memo(
  function PipelineSettingsEditorPanel({
    config,
    activeProfile,
    onContextCacheChange,
    onVariablesChange,
    onCalibrationAppend,
    onResetParallelTemplate,
    onParallelTemplateChange,
  }: {
    config: PipelineConfig;
    activeProfile: string;
    onContextCacheChange: (enabled: boolean) => void;
    onVariablesChange: (variables: VarSpec[]) => void;
    onCalibrationAppend: (stepId: string, text: string) => void;
    onResetParallelTemplate: (source: "generic" | "paper") => void;
    onParallelTemplateChange: (template: string) => void;
  }) {
    return (
      <div className="flex-1 flex flex-col min-h-0 overflow-y-auto">
        <div className="p-4 space-y-5">
          <div>
            <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200 mb-1">
              Pipeline Settings
            </h3>
            <p className="text-xs text-gray-500 dark:text-gray-400">
              Settings that apply to all steps in this profile.
            </p>
          </div>

          <div className="flex items-start gap-3">
            <button
              type="button"
              role="switch"
              aria-label="Reuse shared input context"
              aria-checked={config.context_cache?.enabled ?? false}
              onClick={() =>
                onContextCacheChange(!(config.context_cache?.enabled ?? false))
              }
              className={`w-8 h-5 rounded-full relative transition-colors shrink-0 mt-0.5 ${
                config.context_cache?.enabled
                  ? "bg-green-600"
                  : "bg-gray-300 dark:bg-gray-600"
              }`}
            >
              <div
                className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${
                  config.context_cache?.enabled
                    ? "translate-x-3.5"
                    : "translate-x-0.5"
                }`}
              />
            </button>
            <div>
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
                  Reuse shared input context
                </span>
                <span className="rounded bg-blue-50 px-1.5 py-0.5 text-[10px] font-medium text-blue-600 dark:bg-blue-950/50 dark:text-blue-300">
                  {["auto-review", "auto-review-quick"].includes(activeProfile)
                    ? "Automatic review default"
                    : "Optional"}
                </span>
              </div>
              <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                Automatic Paper Review enables this by default. It prepares the
                extracted input and orientation map once for all review steps.
                Pipeline automatically uses provider prompt caches for API calls
                and forked base sessions for Claude or Codex CLI. For other
                profiles, turn it on for large, multi-step reviews; unsupported
                providers fall back safely to ordinary calls.
              </p>
              {config.context_cache?.enabled && (
                <p className="text-[11px] text-green-700 dark:text-green-400 mt-1">
                  Enabled for this profile. Cache reads and writes will appear
                  in token usage.
                </p>
              )}
            </div>
          </div>

          <VariablesEditor
            variables={config.variables ?? []}
            onChange={onVariablesChange}
          />
          <CalibrateSection onAppend={onCalibrationAppend} />

          <div>
            <div className="flex items-center justify-between mb-1.5">
              <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
                Parallel step context template
              </label>
              <div className="flex items-center gap-3">
                <button
                  type="button"
                  onClick={() => onResetParallelTemplate("generic")}
                  className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
                  title="Neutral wrapper for any input: survey + instructions + input path."
                >
                  Reset to generic
                </button>
                <button
                  type="button"
                  onClick={() => onResetParallelTemplate("paper")}
                  className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
                  title="Referee briefing for academic papers: paper type, figure hints, issue-focused framing."
                >
                  Reset to paper review
                </button>
              </div>
            </div>
            <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">
              Wraps each parallel step's prompt. Controls what context the LLM
              receives.
            </p>
            <PromptEditor
              value={config.parallel_context_template}
              onChange={onParallelTemplateChange}
              context={{ kind: "parallel_template" }}
              ariaLabel="Parallel step context template"
              rows={16}
              fillHeight={false}
            />
          </div>
        </div>
      </div>
    );
  },
);

export const StepEditorPanel = memo(function StepEditorPanel({
  step,
  config,
  settings,
  catalogs,
  conditionStepIds,
  advanced,
  onUpdate,
  onPhaseChange,
  onOutputRoleChange,
}: {
  step: StepConfig;
  config: PipelineConfig;
  settings: Settings | null;
  catalogs: Record<string, ModelCatalog>;
  conditionStepIds: string[];
  advanced: boolean;
  onUpdate: (id: string, patch: Partial<StepConfig>) => void;
  onPhaseChange: (id: string, phase: Phase) => void;
  onOutputRoleChange: (
    id: string,
    role: "primary_step" | "findings_step",
    enabled: boolean,
  ) => void;
}) {
  const [activeTab, setActiveTab] = useState<
    "prompt" | "inputs" | "execution" | "model"
  >("prompt");
  useEffect(() => setActiveTab("prompt"), [step.id]);

  const otherSteps = config.steps
    .filter((candidate) => candidate.enabled && candidate.id !== step.id)
    .map(({ id, label }) => ({ id, label }));
  const defaultReportStepIds = config.steps
    .slice(
      0,
      config.steps.findIndex((candidate) => candidate.id === step.id),
    )
    .filter((candidate) => candidate.enabled)
    .map((candidate) => candidate.id);
  const summary = describeStep(step, config);
  const tabs: Array<["prompt" | "inputs" | "execution" | "model", string]> =
    advanced
      ? [
          ["prompt", "Prompt"],
          ["inputs", "Inputs & dependencies"],
          ["execution", "When it runs"],
          ["model", "Model & agents"],
        ]
      : [
          ["prompt", "Prompt"],
          ["inputs", "Inputs"],
          ["execution", "When it runs"],
        ];

  useEffect(() => {
    if (!advanced && activeTab === "model") {
      setActiveTab("prompt");
    }
  }, [activeTab, advanced]);

  const handleTabKeyDown = (
    event: KeyboardEvent<HTMLButtonElement>,
    current: (typeof tabs)[number][0],
  ) => {
    const index = tabs.findIndex(([id]) => id === current);
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
    else if (event.key === "ArrowLeft")
      next = (index - 1 + tabs.length) % tabs.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = tabs.length - 1;
    else return;
    event.preventDefault();
    const id = tabs[next][0];
    setActiveTab(id);
    document.getElementById(`step-tab-${id}`)?.focus();
  };

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="px-5 pt-4 pb-3 border-b border-gray-200 dark:border-gray-700 space-y-2.5">
        <div className="flex items-end gap-3">
          <div className="min-w-0 flex-1">
            <label className="block text-xs font-medium text-gray-500 mb-1">
              Step name
            </label>
            <input
              type="text"
              aria-label="Step label"
              value={step.label}
              onChange={(event) =>
                onUpdate(step.id, { label: event.target.value })
              }
              className="w-full py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm font-medium text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 transition-colors"
            />
          </div>
          <span
            className={`mb-0.5 rounded-full px-2.5 py-1 text-[11px] font-medium ${
              step.enabled
                ? step.phase === "parallel"
                  ? "bg-blue-50 text-blue-700 dark:bg-blue-950/50 dark:text-blue-300"
                  : "bg-orange-50 text-orange-700 dark:bg-orange-950/50 dark:text-orange-300"
                : "bg-gray-100 text-gray-500 dark:bg-gray-800 dark:text-gray-400"
            }`}
          >
            {step.enabled
              ? step.phase === "parallel"
                ? "Parallel"
                : "Sequential"
              : "Disabled"}
          </span>
        </div>
        <p
          className="text-xs leading-relaxed text-gray-600 dark:text-gray-400"
          data-testid="step-summary"
        >
          {summary.sentence}
        </p>
      </div>

      <div
        role="tablist"
        aria-label="Step settings"
        className="flex gap-1 overflow-x-auto border-b border-gray-200 px-4 pt-2 dark:border-gray-700"
      >
        {tabs.map(([id, label]) => (
          <button
            key={id}
            type="button"
            role="tab"
            id={`step-tab-${id}`}
            aria-selected={activeTab === id}
            aria-controls={`step-panel-${id}`}
            tabIndex={activeTab === id ? 0 : -1}
            onClick={() => setActiveTab(id)}
            onKeyDown={(event) => handleTabKeyDown(event, id)}
            className={`shrink-0 rounded-t-lg border-b-2 px-3 py-2 text-xs font-medium transition-colors ${
              activeTab === id
                ? "border-gray-900 text-gray-900 dark:border-gray-100 dark:text-gray-100"
                : "border-transparent text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-200"
            }`}
          >
            {label}
          </button>
        ))}
      </div>

      <section
        id="step-panel-prompt"
        role="tabpanel"
        aria-labelledby="step-tab-prompt"
        hidden={activeTab !== "prompt"}
        className="flex-1 min-h-0 p-4"
      >
        <PromptEditor
          value={step.prompt}
          onChange={(prompt) => onUpdate(step.id, { prompt })}
          ariaLabel={`Prompt for ${step.label}`}
          context={
            step.phase === "sequential"
              ? {
                  kind: "sequential",
                  otherStepIds: config.steps
                    .filter((candidate) => candidate.id !== step.id)
                    .map((candidate) => candidate.id),
                }
              : { kind: "parallel" }
          }
        />
      </section>

      <section
        id="step-panel-inputs"
        role="tabpanel"
        aria-labelledby="step-tab-inputs"
        hidden={activeTab !== "inputs"}
        className="flex-1 min-h-0 overflow-y-auto p-5"
      >
        <div className="mb-4">
          <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200">
            Inputs &amp; dependencies
          </h3>
          <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
            Choose exactly what this step can read. Selecting an earlier report
            also makes this step wait for it.
          </p>
        </div>
        <AdvancedStepOptions
          key={`${step.id}-inputs`}
          section="inputs"
          step={step}
          otherSteps={otherSteps}
          defaultReportStepIds={defaultReportStepIds}
          namedInputs={config.extraction?.extra_inputs ?? []}
          inputMode={config.extraction?.input_mode || "document"}
          surveyEnabled={true}
          conditionStepIds={conditionStepIds}
          onChange={(patch) => onUpdate(step.id, patch)}
        />
      </section>

      <section
        id="step-panel-execution"
        role="tabpanel"
        aria-labelledby="step-tab-execution"
        hidden={activeTab !== "execution"}
        className="flex-1 min-h-0 overflow-y-auto p-5"
      >
        <div className="mb-5">
          <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200">
            When it runs
          </h3>
          <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
            Timing, run conditions, published results, and file-by-file fan-out.
            Output contracts live in the Schemas tab.
          </p>
        </div>
        <div className="mb-5">
          <label className="block text-xs font-medium text-gray-600 dark:text-gray-300 mb-1.5">
            When it runs
          </label>
          <div className="flex gap-2">
            {(["parallel", "sequential"] as const).map((phase) => (
              <button
                key={phase}
                type="button"
                aria-pressed={step.phase === phase}
                onClick={() => onPhaseChange(step.id, phase)}
                className={`px-3 py-1.5 text-xs rounded-full border transition-colors ${
                  step.phase === phase
                    ? phase === "parallel"
                      ? "bg-blue-700 text-white border-blue-700"
                      : "bg-orange-700 text-white border-orange-700"
                    : "bg-white dark:bg-gray-800 text-gray-500 border-gray-300 dark:border-gray-600 hover:border-gray-400"
                }`}
              >
                {phase === "parallel"
                  ? "Independently (parallel)"
                  : "After dependencies (sequential)"}
              </button>
            ))}
          </div>
        </div>
        <div className="mb-5 space-y-2">
          <div>
            <h4 className="text-xs font-medium text-gray-600 dark:text-gray-300">
              Published results
            </h4>
            <p className="mt-1 text-[11px] text-gray-500 dark:text-gray-400">
              Tell Pipeline which step is the report people read and which
              structured findings Projects tracks.
            </p>
          </div>
          <label className="flex items-start gap-2 text-xs text-gray-700 dark:text-gray-300">
            <input
              type="checkbox"
              className="mt-0.5"
              disabled={!step.enabled || step.phase !== "sequential"}
              checked={config.outputs?.primary_step === step.id}
              onChange={(event) =>
                onOutputRoleChange(
                  step.id,
                  "primary_step",
                  event.target.checked,
                )
              }
            />
            <span>
              Use as the primary report
              <span className="block text-[10px] text-gray-500 dark:text-gray-400">
                This becomes the run's main human-readable result.
              </span>
            </span>
          </label>
          <label className="flex items-start gap-2 text-xs text-gray-700 dark:text-gray-300">
            <input
              type="checkbox"
              className="mt-0.5"
              disabled={!step.enabled || step.phase !== "sequential"}
              checked={config.outputs?.findings_step === step.id}
              onChange={(event) =>
                onOutputRoleChange(
                  step.id,
                  "findings_step",
                  event.target.checked,
                )
              }
            />
            <span>
              Track findings in the project ledger
              <span className="block text-[10px] text-gray-500 dark:text-gray-400">
                This step's structured findings feed the issue ledger that
                follows your document across runs. Pipeline adds an issues
                contract automatically; review it in the Schemas tab.
              </span>
            </span>
          </label>
          {step.phase !== "sequential" && (
            <p className="text-[10px] text-amber-700 dark:text-amber-300">
              Published results must come from a sequential step.
            </p>
          )}
        </div>
        <AdvancedStepOptions
          key={`${step.id}-execution`}
          section="execution"
          step={step}
          otherSteps={otherSteps}
          defaultReportStepIds={defaultReportStepIds}
          namedInputs={config.extraction?.extra_inputs ?? []}
          inputMode={config.extraction?.input_mode || "document"}
          surveyEnabled={true}
          conditionStepIds={conditionStepIds}
          onChange={(patch) => onUpdate(step.id, patch)}
        />
      </section>

      <section
        id="step-panel-model"
        role="tabpanel"
        aria-labelledby="step-tab-model"
        hidden={activeTab !== "model"}
        className="flex-1 min-h-0 overflow-y-auto p-5 space-y-5"
      >
        <div>
          <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200">
            Model &amp; agents
          </h3>
          <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
            Choose providers and tools, or inherit the profile-wide defaults.
          </p>
        </div>
        <div>
          <label className="block text-xs font-medium text-gray-500 mb-1.5">
            Tools
          </label>
          {["WebSearch"].map((tool) => {
            const active = step.tools?.includes(tool) ?? false;
            return (
              <label
                key={tool}
                className="flex items-center gap-1.5 text-xs text-gray-700 dark:text-gray-300 cursor-pointer"
              >
                <input
                  type="checkbox"
                  checked={active}
                  onChange={() =>
                    onUpdate(step.id, {
                      tools: active
                        ? (step.tools ?? []).filter(
                            (candidate) => candidate !== tool,
                          )
                        : [...(step.tools ?? []), tool],
                    })
                  }
                  className="rounded"
                />
                Web search
              </label>
            );
          })}
        </div>
        <AgentChips
          agents={step.agents ?? []}
          multi={step.phase === "parallel"}
          onChange={(agents) => onUpdate(step.id, { agents })}
        />
        <ModelOverrides
          step={step}
          settings={settings}
          catalogs={catalogs}
          onChange={(patch) => onUpdate(step.id, patch)}
        />
      </section>
    </div>
  );
});

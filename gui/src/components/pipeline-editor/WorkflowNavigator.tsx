import { useState } from "react";

import { lintCrossStepReferences } from "../../lib/pipelineHelpers";
import WaveDiagram from "../WaveDiagram";

import SidebarPanel from "../SidebarPanel";
import usePersistentPanelWidth from "../../hooks/usePersistentPanelWidth";
import { confirmDialog } from "../DialogService";

import { MemoizedStepRow as StepRow } from "./StepListItems";
import { AdaptiveAgentsRow } from "./AdaptiveReviewEditors";
import {
  adaptiveAgentCount as getAdaptiveAgentCount,
  adaptiveAgentRange as getAdaptiveAgentRange,
  isAutoReview,
} from "../../lib/autoReview";

import { describeStep } from "./stepSummary";

import type { WorkflowEditor } from "./useWorkflowEditor";
import type { ProfileOperations } from "./useProfileOperations";
import type { StepActions } from "./useStepActions";
import type { WorkflowView } from "./useWorkflowView";
export function WorkflowNavigator({
  editor,
  operations,
  actions,
  view,
  onClose,
  onOpenGallery,
  showBack,
}: {
  editor: WorkflowEditor;
  operations: ProfileOperations;
  actions: StepActions;
  view: WorkflowView;
  onClose: () => void;
  onOpenGallery?: () => void;
  showBack: boolean;
}) {
  const {
    config,
    activeProfile,
    dirty,
    schemaDraftValid,
    editing,
    editingStep,
    setEditing,
  } = editor;
  const { undo, redo, canUndo, canRedo } = editor.history;
  const {
    profiles,
    saving,
    saved,
    profileMutationPending,
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
  } = operations;
  const { updateStepEnabled, removeStep, moveSequentialStep } = actions;
  const {
    navigatorView,
    setNavigatorView,
    editorMode,
    setEditorMode,
    setAddStepOpen,
  } = view;
  const [exportMenuOpen, setExportMenuOpen] = useState(false);
  const [panelWidth, setPanelWidth] = usePersistentPanelWidth(
    "pipeline.ui.workflowPanelWidth",
    420,
    280,
    640,
  );
  if (!config) return null;
  const activeProfileName =
    profiles.find((p) => p.id === activeProfile)?.name ?? activeProfile;

  // Group steps for display: find boundaries between parallel and sequential
  const parallelSteps = config.steps.filter((s) => s.phase === "parallel");
  const sequentialSteps = config.steps.filter((s) => s.phase === "sequential");
  const hasMultiAgent = parallelSteps.some((s) => s.agents?.length > 1);
  const autoReview = isAutoReview(config);
  const adaptiveAgentCount = getAdaptiveAgentCount(config);
  const adaptiveAgentRange = getAdaptiveAgentRange(config);
  const crossStepWarnings = lintCrossStepReferences(config);

  return (
    <SidebarPanel
      aria-label="Workflow editor navigation"
      width={panelWidth}
      defaultWidth={420}
      min={280}
      max={640}
      onResize={setPanelWidth}
      resizeLabel="Resize workflow panel"
    >
      {/* Header */}
      <div className="px-4 pt-4 pb-2 flex items-center justify-between">
        <div>
          <h2 className="text-lg font-bold text-gray-900 dark:text-gray-100">
            Review designer
          </h2>
          <div
            className="mt-2 inline-flex rounded-lg bg-gray-100 p-0.5 dark:bg-gray-800"
            aria-label="Editor detail level"
          >
            {(["basic", "advanced"] as const).map((mode) => (
              <button
                key={mode}
                type="button"
                aria-pressed={editorMode === mode}
                onClick={() => {
                  setEditorMode(mode);
                  if (mode === "basic" && navigatorView === "schemas")
                    setNavigatorView("steps");
                }}
                className={`rounded-md px-2.5 py-1 text-[11px] font-medium ${editorMode === mode ? "bg-white shadow-sm dark:bg-gray-700" : "text-gray-500 dark:text-gray-400"}`}
              >
                {mode === "basic" ? "Basic" : "Advanced"}
              </button>
            ))}
          </div>
        </div>
        {showBack && (
          <button
            onClick={() =>
              void (async () => {
                if (
                  dirty &&
                  !(await confirmDialog(
                    "You have unsaved changes. Leave and discard them?",
                    { destructive: true },
                  ))
                )
                  return;
                onClose();
              })()
            }
            className="text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
          >
            Back
          </button>
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
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
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
          <button
            onClick={handleDuplicateProfile}
            disabled={profileMutationPending}
            className="flex-1 py-1 text-[11px] text-gray-500 hover:text-gray-700 hover:bg-gray-50 rounded transition-colors disabled:opacity-40"
          >
            Duplicate
          </button>
          <button
            onClick={handleRenameProfile}
            disabled={profileMutationPending}
            className="flex-1 py-1 text-[11px] text-gray-500 hover:text-gray-700 hover:bg-gray-50 rounded transition-colors disabled:opacity-40"
          >
            Rename
          </button>
          <button
            onClick={handleDeleteProfile}
            disabled={
              profileMutationPending ||
              profiles.find((profile) => profile.id === activeProfile)?.builtin
            }
            className="flex-1 py-1 text-[11px] text-gray-500 hover:text-red-600 hover:bg-red-50
                         rounded transition-colors disabled:opacity-30 disabled:hover:text-gray-500
                         disabled:hover:bg-transparent"
          >
            Delete
          </button>
        </div>
      </div>

      {crossStepWarnings.length > 0 && (
        <div className="mx-4 mb-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 dark:border-amber-900 dark:bg-amber-950/30">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-amber-800 dark:text-amber-200">
            Cross-step references
          </p>
          <ul className="mt-1 space-y-0.5">
            {crossStepWarnings.slice(0, 6).map((warning, index) => (
              <li
                key={`${warning.stepId}-${index}`}
                className="text-[11px] leading-snug text-amber-800 dark:text-amber-200"
              >
                <button
                  type="button"
                  className="font-medium underline underline-offset-2"
                  onClick={() => setEditing(warning.stepId)}
                >
                  {warning.stepId}
                </button>
                : {warning.message}
              </li>
            ))}
            {crossStepWarnings.length > 6 && (
              <li className="text-[11px] text-amber-700 dark:text-amber-300">
                +{crossStepWarnings.length - 6} more
              </li>
            )}
          </ul>
        </div>
      )}
      <div className="px-4 pb-2">
        <div
          role="tablist"
          aria-label="Workflow navigator view"
          className="grid grid-cols-3 rounded-lg bg-gray-100 p-0.5 dark:bg-gray-800"
        >
          {(editorMode === "advanced"
            ? (["steps", "overview", "schemas"] as const)
            : (["steps", "overview"] as const)
          ).map((view) => (
            <button
              key={view}
              type="button"
              role="tab"
              aria-selected={navigatorView === view}
              onClick={() => {
                setNavigatorView(view);
                if (
                  view === "schemas" &&
                  editing !== "orientation" &&
                  !editingStep
                ) {
                  setEditing("orientation");
                }
              }}
              className={`rounded-md px-2 py-1.5 text-xs font-medium transition-colors ${
                navigatorView === view
                  ? "bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-gray-100"
                  : "text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-200"
              }`}
            >
              {view === "steps"
                ? "Steps"
                : view === "overview"
                  ? "Overview"
                  : "Schemas"}
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
        ) : navigatorView === "schemas" ? (
          <div>
            <div className="sticky top-0 z-[1] border-b border-gray-100 bg-white px-4 py-3 dark:border-gray-800 dark:bg-gray-900">
              <div className="flex items-center justify-between gap-3">
                <span className="text-[10px] font-semibold uppercase tracking-wider text-violet-700 dark:text-violet-300">
                  Artifact contracts
                </span>
                <span className="rounded-full bg-violet-50 px-2 py-0.5 text-[10px] font-medium text-violet-700 dark:bg-violet-950/40 dark:text-violet-300">
                  {config.steps.filter((step) => !!step.output_schema).length +
                    (config.orientation_schema ? 1 : 0)}{" "}
                  configured
                </span>
              </div>
              <p className="mt-1 text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">
                Select a stage to inspect its effective provider schema.
              </p>
            </div>
            <button
              type="button"
              aria-label="Orientation schema"
              onClick={() => setEditing("orientation")}
              className={`w-full border-b border-gray-100 px-4 py-3 text-left transition-colors dark:border-gray-800 ${
                !editingStep
                  ? "bg-violet-50/70 dark:bg-violet-950/25"
                  : "hover:bg-gray-50 dark:hover:bg-gray-800/50"
              }`}
            >
              <span className="flex items-center justify-between gap-3">
                <span className="min-w-0">
                  <span className="block truncate text-sm font-medium text-gray-800 dark:text-gray-200">
                    Orientation map
                  </span>
                  <span className="mt-0.5 block text-[11px] text-gray-500 dark:text-gray-400">
                    Preprocessing JSON survey
                  </span>
                </span>
                <span
                  className={`shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium ${
                    config.orientation_schema
                      ? "bg-violet-100 text-violet-700 dark:bg-violet-950/60 dark:text-violet-300"
                      : "bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300"
                  }`}
                >
                  {config.orientation_schema
                    ? autoReview
                      ? "Catalog-backed"
                      : "Workflow contract"
                    : "Object only"}
                </span>
              </span>
            </button>
            <div className="bg-gray-50 px-4 py-1.5 text-[10px] font-semibold uppercase tracking-wider text-gray-500 dark:bg-gray-800/50 dark:text-gray-400">
              Step outputs
            </div>
            {config.steps.map((step) => (
              <button
                key={step.id}
                type="button"
                aria-label={`${step.label} output schema`}
                onClick={() => setEditing(step.id)}
                className={`w-full border-b border-gray-100 px-4 py-3 text-left transition-colors dark:border-gray-800 ${
                  editingStep?.id === step.id
                    ? "bg-violet-50/70 dark:bg-violet-950/25"
                    : "hover:bg-gray-50 dark:hover:bg-gray-800/50"
                }`}
              >
                <span className="flex items-center justify-between gap-3">
                  <span className="min-w-0">
                    <span className="block truncate text-sm font-medium text-gray-800 dark:text-gray-200">
                      {step.label}
                    </span>
                    <span className="mt-0.5 block text-[11px] text-gray-500 dark:text-gray-400">
                      {step.phase === "parallel" ? "Parallel" : "Sequential"}
                      {!step.enabled ? " · Disabled" : ""}
                    </span>
                  </span>
                  <span
                    className={`shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium ${
                      step.output_schema
                        ? "bg-violet-100 text-violet-700 dark:bg-violet-950/60 dark:text-violet-300"
                        : "bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300"
                    }`}
                  >
                    {step.output_schema ? "Custom JSON" : "Markdown"}
                  </span>
                </span>
              </button>
            ))}
          </div>
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
              onClick={() =>
                setEditing(editing === "extraction" ? null : "extraction")
              }
              className={`w-full border-b border-gray-100 px-4 py-2.5 text-left dark:border-gray-800 ${
                editing === "extraction"
                  ? "bg-gray-100 dark:bg-gray-800"
                  : "hover:bg-gray-50 dark:hover:bg-gray-800/50"
              }`}
            >
              <span className="block text-sm font-medium text-gray-800 dark:text-gray-200">
                Input &amp; extraction
              </span>
              <span className="mt-0.5 block text-[11px] text-gray-500 dark:text-gray-400">
                Choose the input type and PDF extraction method
              </span>
            </button>
            <button
              type="button"
              aria-label={
                autoReview ? "Orientation & classification" : "Orientation map"
              }
              onClick={() =>
                setEditing(editing === "orientation" ? null : "orientation")
              }
              className={`w-full border-b border-gray-100 px-4 py-2.5 text-left dark:border-gray-800 ${
                editing === "orientation"
                  ? "bg-gray-100 dark:bg-gray-800"
                  : "hover:bg-gray-50 dark:hover:bg-gray-800/50"
              }`}
            >
              <span className="block text-sm font-medium text-gray-800 dark:text-gray-200">
                {autoReview
                  ? "Orientation & classification"
                  : "Orientation map"}
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
                  onSelect={() =>
                    setEditing(editing === step.id ? null : step.id)
                  }
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
                  onSelect={() =>
                    setEditing(
                      editing === "auto_adaptive_agents"
                        ? null
                        : "auto_adaptive_agents",
                    )
                  }
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
                  onSelect={() =>
                    setEditing(editing === step.id ? null : step.id)
                  }
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
              onClick={() =>
                setEditing(
                  editing === "pipeline_settings" ? null : "pipeline_settings",
                )
              }
              className={`w-full px-4 py-2 text-left border-t border-gray-200 dark:border-gray-700 ${
                editing === "pipeline_settings"
                  ? "bg-gray-100 dark:bg-gray-800"
                  : "hover:bg-gray-50 dark:hover:bg-gray-800/50"
              }`}
            >
              <div className="flex items-center gap-2">
                <svg
                  className="w-3.5 h-3.5 text-gray-400"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={1.5}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M10.5 6h9.75M10.5 6a1.5 1.5 0 1 1-3 0m3 0a1.5 1.5 0 1 0-3 0M3.75 6H7.5m3 12h9.75m-9.75 0a1.5 1.5 0 0 1-3 0m3 0a1.5 1.5 0 0 0-3 0m-3.75 0H7.5m9-6h3.75m-3.75 0a1.5 1.5 0 0 1-3 0m3 0a1.5 1.5 0 0 0-3 0m-9.75 0h9.75"
                  />
                </svg>
                <span className="text-xs font-medium text-gray-600 dark:text-gray-400">
                  Pipeline Settings
                </span>
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
            disabled={
              saving || profileMutationPending || !dirty || !schemaDraftValid
            }
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
        <div className="flex items-center justify-between text-[11px] text-gray-500 dark:text-gray-400">
          <span>
            {!schemaDraftValid
              ? "Cannot save: fix the invalid schema draft."
              : dirty
                ? "Unsaved draft is recoverable."
                : "All changes saved."}
          </span>
          <span className="flex gap-2">
            <button
              type="button"
              onClick={undo}
              disabled={!canUndo}
              className="underline disabled:opacity-30"
            >
              Undo
            </button>
            <button
              type="button"
              onClick={redo}
              disabled={!canRedo}
              className="underline disabled:opacity-30"
            >
              Redo
            </button>
          </span>
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
              <div
                className="absolute bottom-full left-0 right-0 mb-1 bg-white border border-gray-200
                                rounded-lg shadow-lg overflow-hidden z-10"
              >
                <button
                  onClick={() => {
                    handleExportItem();
                    setExportMenuOpen(false);
                  }}
                  disabled={!editingStep}
                  className="w-full py-2 px-3 text-xs text-left text-gray-700 hover:bg-gray-50
                               disabled:text-gray-300 disabled:hover:bg-white transition-colors"
                >
                  Export selected step
                </button>
                <button
                  onClick={() => {
                    handleExportProfile();
                    setExportMenuOpen(false);
                  }}
                  className="w-full py-2 px-3 text-xs text-left text-gray-700 hover:bg-gray-50 transition-colors"
                >
                  Export profile &ldquo;{activeProfileName}&rdquo;
                </button>
                <button
                  onClick={() => {
                    handleExportBundle();
                    setExportMenuOpen(false);
                  }}
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
        {saved && (
          <p className="text-xs text-green-700 dark:text-green-400 text-center">
            Saved.
          </p>
        )}
      </div>
    </SidebarPanel>
  );
}

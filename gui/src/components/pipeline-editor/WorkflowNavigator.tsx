import { useState } from "react";
import { IconButton, MenuItem, MenuSeparator } from "../../ui";
import { Icon } from "../../ui/icons";
import useTabList from "../../hooks/useTabList";
import DesignerActionMenu from "./DesignerActionMenu";
import "./WorkflowNavigator.css";

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

import { forgetWorkflowDraft } from "../../lib/workflowDraftRecovery";
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
  const tabs =
    editorMode === "advanced"
      ? (["steps", "overview", "schemas"] as const)
      : (["steps", "overview"] as const);
  const tabList = useTabList(tabs, navigatorView, (next) => {
    setNavigatorView(next);
    if (next === "schemas" && editing !== "orientation" && !editingStep) {
      setEditing("orientation");
    }
  });
  if (!config) return null;
  const activeProfileName =
    profiles.find((p) => p.id === activeProfile)?.name ?? activeProfile;
  const activeInvalid = profiles.find((p) => p.id === activeProfile)?.invalid;

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
      className="designer-nav"
      aria-label="Review designer navigation"
      width={panelWidth}
      defaultWidth={420}
      min={280}
      max={640}
      onResize={setPanelWidth}
      resizeLabel="Resize review panel"
    >
      {/* Header */}
      <div className="designer-header">
        <h2 className="text-lg font-semibold tracking-tight text-ink">
          Review designer
        </h2>
        <div className="designer-mode" aria-label="Editor detail level">
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
              className="rounded-ui-sm px-2 py-1 text-ui-meta font-medium text-ink-muted aria-pressed:bg-raised aria-pressed:text-ink aria-pressed:shadow-ui-1"
            >
              {mode === "basic" ? "Basic" : "Advanced"}
            </button>
          ))}
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
                forgetWorkflowDraft(activeProfile);
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
            aria-label="Active review"
            aria-busy={profileMutationPending}
            value={activeProfile}
            onChange={(e) => handleSwitchProfile(e.target.value)}
            disabled={profileMutationPending}
            className="min-w-0 flex-1 rounded-ui-sm border border-line bg-surface px-2 py-1.5 text-ui-body font-medium text-ink disabled:opacity-60"
          >
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.invalid ? `${p.name} (needs repair)` : p.name}
              </option>
            ))}
          </select>
          <IconButton
            label="New review"
            size="sm"
            disabled={profileMutationPending}
            onClick={handleNewProfile}
          >
            <Icon name="plus" className="h-4 w-4" />
          </IconButton>
          <DesignerActionMenu
            label="Review actions"
            disabled={profileMutationPending}
          >
            <MenuItem onSelect={handleDuplicateProfile}>Duplicate</MenuItem>
            <MenuItem onSelect={handleRenameProfile}>Rename</MenuItem>
            <MenuSeparator />
            <MenuItem
              danger
              disabled={
                profiles.find((profile) => profile.id === activeProfile)
                  ?.builtin
              }
              onSelect={handleDeleteProfile}
            >
              Delete
            </MenuItem>
          </DesignerActionMenu>
        </div>
        {activeInvalid && (
          <p
            role="alert"
            className="rounded-lg bg-amber-50 px-2 py-1.5 text-[11px] text-amber-800 dark:bg-amber-950/40 dark:text-amber-200"
          >
            This saved review design can't run until it is repaired. Fix it here
            and save, or export it to keep a copy. Reason: {activeInvalid}
          </p>
        )}
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
      <div className="flex flex-wrap items-baseline justify-between gap-x-2 border-b border-line px-4">
        <div
          role="tablist"
          aria-label="Review navigator view"
          className="designer-tabs"
        >
          {tabs.map((view) => (
            <button key={view} {...tabList.tabProps(view)}>
              {view === "steps"
                ? "Steps"
                : view === "overview"
                  ? "Overview"
                  : "Schemas"}
            </button>
          ))}
        </div>
        <button
          type="button"
          onClick={() => setAddStepOpen(true)}
          className="designer-add"
        >
          + Add step
        </button>
      </div>

      {/* Primary workflow navigator. The overview replaces rather than duplicates the list. */}
      {tabs
        .filter((tab) => tab !== navigatorView)
        .map((tab) => (
          <div key={tab} {...tabList.panelProps(tab)} hidden />
        ))}
      <div
        {...tabList.panelProps(navigatorView)}
        className="min-h-0 flex-1 overflow-y-auto"
      >
        {navigatorView === "overview" ? (
          <WaveDiagram
            compact
            steps={config.steps}
            merge={config.merge}
            inputMode={config.extraction?.input_mode}
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
                <span className="text-[10px] font-semibold uppercase tracking-wider text-ink-muted">
                  Output formats
                </span>
                <span className="rounded-full bg-sunken px-2 py-0.5 text-[10px] font-medium text-ink">
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
                  ? "bg-sunken"
                  : "hover:bg-gray-50 dark:hover:bg-gray-800/50"
              }`}
            >
              <span className="flex items-center justify-between gap-3">
                <span className="min-w-0">
                  <span className="block truncate text-sm font-medium text-gray-800 dark:text-gray-200">
                    Orientation map
                  </span>
                  <span className="designer-detail">
                    Preprocessing JSON survey
                  </span>
                </span>
                <span
                  className={`shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium ${
                    config.orientation_schema
                      ? "bg-sunken text-ink"
                      : "bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300"
                  }`}
                >
                  {config.orientation_schema
                    ? autoReview
                      ? "Catalog-backed"
                      : "Review contract"
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
                    ? "bg-sunken"
                    : "hover:bg-gray-50 dark:hover:bg-gray-800/50"
                }`}
              >
                <span className="flex items-center justify-between gap-3">
                  <span className="min-w-0">
                    <span className="block truncate text-sm font-medium text-gray-800 dark:text-gray-200">
                      {step.label}
                    </span>
                    <span className="designer-detail">
                      {step.phase === "parallel" ? "Parallel" : "Sequential"}
                      {!step.enabled ? " · Disabled" : ""}
                    </span>
                  </span>
                  <span
                    className={`shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium ${
                      step.output_schema
                        ? "bg-sunken text-ink"
                        : "bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300"
                    }`}
                  >
                    {step.kind === "analyzer"
                      ? "Pipeline writes this result"
                      : step.output_schema
                        ? "Custom JSON"
                        : "Markdown"}
                  </span>
                </span>
              </button>
            ))}
          </div>
        ) : (
          <>
            {/* Each section holds its header, which lets go at the section's end. */}
            <div>
              <h3 className="designer-phase sticky top-0 z-[1] bg-surface">
                <span>Review setup</span>
              </h3>
              <button
                type="button"
                aria-label="Input & extraction"
                onClick={() =>
                  setEditing(editing === "extraction" ? null : "extraction")
                }
                aria-pressed={editing === "extraction"}
                data-selected={editing === "extraction" || undefined}
                className="designer-setup designer-row"
              >
                <span className="block text-sm font-medium text-gray-800 dark:text-gray-200">
                  Input &amp; extraction
                </span>
                <span className="designer-detail">
                  Input type and PDF extraction
                </span>
              </button>
              <button
                type="button"
                aria-label={
                  autoReview
                    ? "Orientation & classification"
                    : "Orientation map"
                }
                onClick={() =>
                  setEditing(editing === "orientation" ? null : "orientation")
                }
                aria-pressed={editing === "orientation"}
                data-selected={editing === "orientation" || undefined}
                className="designer-setup designer-row"
              >
                <span className="block text-sm font-medium text-gray-800 dark:text-gray-200">
                  {autoReview
                    ? "Orientation & classification"
                    : "Orientation map"}
                </span>
                <span className="designer-detail">
                  {autoReview
                    ? "Maps the paper and picks reviewers"
                    : "Maps the input before review"}
                </span>
              </button>
            </div>

            {/* Parallel section */}
            <div>
              <h3 className="designer-phase sticky top-0 z-[1] bg-surface">
                <span>Parallel</span>
              </h3>

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
              )}

              {/* Merge indicator */}
              {hasMultiAgent && config.merge?.enabled && (
                <button
                  onClick={() => setEditing("merge")}
                  className="designer-setup designer-row"
                  data-selected={editing === "merge" || undefined}
                  aria-pressed={editing === "merge"}
                >
                  <span className="text-ui-label font-medium">
                    Merge (auto)
                  </span>
                </button>
              )}
            </div>

            {/* Sequential section */}
            <div>
              <h3 className="designer-phase sticky top-0 z-[1] bg-surface">
                <span>Sequential</span>
              </h3>

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
            </div>

            {/* Review settings */}
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
                  Review settings
                </span>
              </div>
            </button>
          </>
        )}
      </div>

      {/* Bottom actions */}
      <div className="p-3 border-t border-gray-200 dark:border-gray-700 space-y-2">
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
            className="py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-600 dark:text-gray-300
                         hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors disabled:cursor-wait disabled:opacity-50"
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
              className="w-full py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-xs text-gray-500 dark:text-gray-400
                           hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
            >
              Export {exportMenuOpen ? "\u25B2" : "\u25BC"}
            </button>
            {exportMenuOpen && (
              <div
                className="absolute bottom-full left-0 right-0 mb-1 bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700
                                rounded-lg shadow-lg overflow-hidden z-10"
              >
                <button
                  onClick={() => {
                    handleExportItem();
                    setExportMenuOpen(false);
                  }}
                  disabled={!editingStep}
                  className="w-full py-2 px-3 text-xs text-left text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800
                               disabled:text-gray-300 dark:disabled:text-gray-600 disabled:hover:bg-white dark:disabled:hover:bg-gray-900 transition-colors"
                >
                  Export selected step
                </button>
                <button
                  onClick={() => {
                    handleExportProfile();
                    setExportMenuOpen(false);
                  }}
                  className="w-full py-2 px-3 text-xs text-left text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
                >
                  Export review &ldquo;{activeProfileName}&rdquo;
                </button>
                <button
                  onClick={() => {
                    handleExportBundle();
                    setExportMenuOpen(false);
                  }}
                  className="w-full py-2 px-3 text-xs text-left text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
                >
                  Export all (reviews + settings)
                </button>
              </div>
            )}
          </div>
          <button
            onClick={handleImport}
            className="flex-1 py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-xs text-gray-500 dark:text-gray-400
                         hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
          >
            Import
          </button>
          {onOpenGallery && (
            <button
              onClick={onOpenGallery}
              title="Browse curated review templates"
              className="flex-1 py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-xs text-gray-500 dark:text-gray-400
                           hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
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

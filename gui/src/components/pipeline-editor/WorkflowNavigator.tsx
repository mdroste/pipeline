import "./WorkflowNavigator.css";
import { useId, useRef, useState } from "react";
import useTabList from "../../hooks/useTabList";
import Button from "../../ui/Button";
import { Menu, MenuItem, MenuSeparator } from "../../ui/Menu";
import { Icon } from "../../ui/icons";

import { lintCrossStepReferences } from "../../lib/pipelineHelpers";
import WaveDiagram from "../WaveDiagram";
import WorkflowWarnings from "./WorkflowWarnings";

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
  leaveGuardedByShell,
}: {
  editor: WorkflowEditor;
  operations: ProfileOperations;
  actions: StepActions;
  view: WorkflowView;
  onClose: () => void;
  onOpenGallery?: () => void;
  showBack: boolean;
  leaveGuardedByShell: boolean;
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
  const [profileMenuOpen, setProfileMenuOpen] = useState(false);
  const profileTrigger = useRef<HTMLButtonElement>(null);
  const [exportMenuOpen, setExportMenuOpen] = useState(false);
  const exportTrigger = useRef<HTMLButtonElement>(null);
  const exportId = useId();
  const navigatorIds =
    editorMode === "advanced"
      ? (["steps", "overview", "schemas"] as const)
      : (["steps", "overview"] as const);
  const navigatorTabs = useTabList(
    navigatorIds,
    navigatorView,
    (next) => {
      setNavigatorView(next);
      if (next === "schemas" && editing !== "orientation" && !editingStep)
        setEditing("orientation");
    },
    "manual",
  );
  const [panelWidth, setPanelWidth] = usePersistentPanelWidth(
    "pipeline.ui.workflowPanelWidth",
    420,
    280,
    640,
  );
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
      aria-label="Review designer navigation"
      width={panelWidth}
      defaultWidth={420}
      min={280}
      max={640}
      onResize={setPanelWidth}
      resizeLabel="Resize review panel"
    >
      {/* Header */}
      <div className="px-4 pt-4 pb-2 flex items-center justify-between">
        <div>
          <h2 className="text-lg font-bold text-gray-900 dark:text-gray-100">
            Review designer
          </h2>
          <div
            className="ui-segmented mt-2"
            role="group"
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
                if (leaveGuardedByShell) {
                  onClose();
                  return;
                }
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
            Back to review
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
            className="flex-1 py-1.5 px-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                         focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors
                         disabled:cursor-wait disabled:opacity-60"
          >
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.invalid ? `${p.name} (needs repair)` : p.name}
              </option>
            ))}
          </select>
          <button
            onClick={handleNewProfile}
            aria-label="New review"
            disabled={profileMutationPending}
            className="p-1.5 text-gray-500 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors disabled:opacity-40"
            title="New review"
          >
            <svg className="w-4 h-4" viewBox="0 0 20 20" fill="currentColor">
              <path d="M10.75 4.75a.75.75 0 00-1.5 0v4.5h-4.5a.75.75 0 000 1.5h4.5v4.5a.75.75 0 001.5 0v-4.5h4.5a.75.75 0 000-1.5h-4.5v-4.5z" />
            </svg>
          </button>
        </div>
        {activeInvalid && (
          <p role="alert" className="ui-notice ui-notice--warning">
            This saved review design can't run until it is repaired. Fix it here
            and save, or export it to keep a copy. Reason: {activeInvalid}
          </p>
        )}
      </div>

      <WorkflowWarnings
        warnings={crossStepWarnings}
        steps={config.steps}
        onSelect={(warning) => {
          setEditing(warning.stepId);
          setNavigatorView("steps");
          view.setWarningTarget((last) => ({
            ...warning,
            request: (last?.request ?? 0) + 1,
          }));
        }}
      />
      <div className="px-4 pb-2">
        <div
          role="tablist"
          aria-label="Review navigator view"
          className="ui-tabs"
        >
          {navigatorIds.map((view) => (
            <button key={view} {...navigatorTabs.tabProps(view)}>
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
      {navigatorIds
        .filter((id) => id !== navigatorView)
        .map((id) => (
          <div key={id} {...navigatorTabs.panelProps(id)} hidden />
        ))}
      <div
        {...navigatorTabs.panelProps(navigatorView)}
        className="flex-1 overflow-y-auto"
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
                <span className="text-[0.625rem] font-semibold uppercase tracking-wider text-violet-700 dark:text-violet-300">
                  Output formats
                </span>
                <span className="rounded-full bg-violet-50 px-2 py-0.5 text-[0.625rem] font-medium text-violet-700 dark:bg-violet-950/40 dark:text-violet-300">
                  {config.steps.filter((step) => !!step.output_schema).length +
                    (config.orientation_schema ? 1 : 0)}{" "}
                  configured
                </span>
              </div>
              <p className="mt-1 text-[0.6875rem] leading-relaxed text-gray-500 dark:text-gray-400">
                Select a stage to inspect its effective provider schema.
              </p>
            </div>
            <button
              type="button"
              aria-label="Document overview schema"
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
                    Document overview
                  </span>
                  <span className="mt-0.5 block text-[0.6875rem] text-gray-500 dark:text-gray-400">
                    Preprocessing JSON survey
                  </span>
                </span>
                <span
                  className={`shrink-0 rounded-full px-2 py-0.5 text-[0.625rem] font-medium ${
                    config.orientation_schema
                      ? "bg-violet-100 text-violet-700 dark:bg-violet-950/60 dark:text-violet-300"
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
            <div className="bg-gray-50 px-4 py-1.5 text-[0.625rem] font-semibold uppercase tracking-wider text-gray-500 dark:bg-gray-800/50 dark:text-gray-400">
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
                    <span className="mt-0.5 block text-[0.6875rem] text-gray-500 dark:text-gray-400">
                      {step.phase === "parallel" ? "Parallel" : "Sequential"}
                      {!step.enabled ? " · Disabled" : ""}
                    </span>
                  </span>
                  <span
                    className={`shrink-0 rounded-full px-2 py-0.5 text-[0.625rem] font-medium ${
                      step.output_schema
                        ? "bg-violet-100 text-violet-700 dark:bg-violet-950/60 dark:text-violet-300"
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
              <div className="px-4 py-1.5 bg-gray-50 dark:bg-gray-800 sticky top-0 z-[1]">
                <span className="text-[0.625rem] uppercase tracking-wider text-gray-500 dark:text-gray-400 font-medium">
                  Review setup
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
                <span className="mt-0.5 block text-[0.6875rem] text-gray-500 dark:text-gray-400">
                  Input type and PDF extraction
                </span>
              </button>
              <button
                type="button"
                aria-label="Document bundle preparation"
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
                  Document bundle preparation
                </span>
                <span className="mt-0.5 block text-[0.6875rem] text-gray-500 dark:text-gray-400">
                  {autoReview
                    ? "Prepares the document and chooses reviewers"
                    : "Maps the input before review"}
                </span>
              </button>
            </div>

            {/* Parallel section */}
            <div>
              <div className="px-4 py-1.5 bg-surface sticky top-0 z-[1]">
                <span className="text-[0.625rem] uppercase tracking-wider text-ink-muted font-medium">
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
                <div className="border-t border-line">
                  <div className="bg-sunken px-4 py-1.5 text-[0.625rem] font-semibold uppercase tracking-wider text-blue-700 dark:bg-sunken dark:text-blue-300">
                    Selected for this document
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
                  <span className="text-[0.625rem] uppercase tracking-wider text-amber-700 dark:text-amber-300 font-medium">
                    Merge (auto)
                  </span>
                </button>
              )}
            </div>

            {/* Sequential section */}
            <div>
              <div className="px-4 py-1.5 bg-surface sticky top-0 z-[1]">
                <span className="text-[0.625rem] uppercase tracking-wider text-ink-muted font-medium">
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
            className="py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-600 dark:text-gray-300
                         hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors disabled:cursor-wait disabled:opacity-50"
          >
            Reset
          </button>
        </div>
        <div className="flex items-center justify-between text-[0.6875rem] text-gray-500 dark:text-gray-400">
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
          <div className="flex-1">
            <Button
              ref={exportTrigger}
              size="sm"
              className="w-full"
              aria-haspopup="menu"
              aria-expanded={exportMenuOpen}
              aria-controls={exportId}
              onClick={() => setExportMenuOpen(!exportMenuOpen)}
            >
              Export <Icon name="chevron-down" className="h-3.5 w-3.5" />
            </Button>
            <Menu
              id={exportId}
              open={exportMenuOpen}
              onClose={() => setExportMenuOpen(false)}
              anchorRef={exportTrigger}
              label="Export review design"
              side="top"
              align="start"
              width={288}
            >
              <MenuItem onSelect={handleExportItem} disabled={!editingStep}>
                Export selected step
              </MenuItem>
              <MenuItem onSelect={handleExportProfile}>
                Export review &ldquo;{activeProfileName}&rdquo;
              </MenuItem>
              <MenuItem onSelect={handleExportBundle}>
                Export all (reviews + settings)
              </MenuItem>
            </Menu>
          </div>
          <Button
            ref={profileTrigger}
            size="sm"
            className="flex-1"
            aria-haspopup="menu"
            aria-expanded={profileMenuOpen}
            onClick={() => setProfileMenuOpen((open) => !open)}
          >
            Review actions <Icon name="chevron-down" className="h-3.5 w-3.5" />
          </Button>
          <Menu
            open={profileMenuOpen}
            onClose={() => setProfileMenuOpen(false)}
            anchorRef={profileTrigger}
            label="Review actions"
            side="top"
            width={256}
          >
            <MenuItem
              onSelect={handleDuplicateProfile}
              disabled={profileMutationPending}
            >
              Duplicate
            </MenuItem>
            <MenuItem
              onSelect={handleRenameProfile}
              disabled={profileMutationPending}
            >
              Rename
            </MenuItem>
            <MenuItem
              onSelect={handleDeleteProfile}
              disabled={
                profileMutationPending ||
                profiles.find((profile) => profile.id === activeProfile)
                  ?.builtin
              }
              danger
            >
              Delete
            </MenuItem>
            <MenuSeparator />
            <MenuItem onSelect={handleImport}>Import</MenuItem>
            {onOpenGallery && (
              <MenuItem onSelect={onOpenGallery}>Gallery</MenuItem>
            )}
          </Menu>
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

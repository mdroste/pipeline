import "./WorkflowNavigator.css";
import {
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { stageOptions } from "./stageOptions";
import Select from "../../ui/Select";
import Button from "../../ui/Button";
import { Menu, MenuItem, MenuSeparator } from "../../ui/Menu";
import { Icon } from "../../ui/icons";

import { lintCrossStepReferences } from "../../lib/pipelineHelpers";
import WorkflowWarnings from "./WorkflowWarnings";
import { SchemaBrowser } from "./SchemaBrowser";
import { confirmDialog } from "../DialogService";
import { forgetWorkflowDraft } from "../../lib/workflowDraftRecovery";
import type { WorkflowEditor } from "./useWorkflowEditor";
import type { ProfileOperations } from "./useProfileOperations";
import type { WorkflowView } from "./useWorkflowView";
export function WorkflowNavigator({
  editor,
  operations,
  children,
  view,
  onClose,
  onOpenGallery,
  showBack,
  leaveGuardedByShell,
}: {
  editor: WorkflowEditor;
  operations: ProfileOperations;
  children: ReactNode;
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
  const surface = useRef<HTMLDivElement>(null);
  const returnStage = useRef<string | null>(null);
  const schemas = navigatorView === "schemas" && editing !== null;
  const overview = editing === null;
  useLayoutEffect(() => {
    if (!overview || !returnStage.current) return;
    const stage = returnStage.current;
    returnStage.current = null;
    const node = Array.from(
      surface.current?.querySelectorAll<HTMLButtonElement>(
        "[data-review-stage]",
      ) ?? [],
    ).find((button) => button.dataset.reviewStage === stage);
    (
      node ??
      surface.current?.querySelector<HTMLButtonElement>(
        stage === "pipeline_settings"
          ? "[data-review-settings]"
          : ".designer-stage-picker",
      )
    )?.focus();
  }, [overview]);
  function openStage(id: string) {
    setNavigatorView("stage");
    setEditing(id);
  }
  function backToOverview() {
    returnStage.current = editing;
    setEditing(null);
    setNavigatorView("stage");
  }
  if (!config) return null;
  const activeProfileName =
    profiles.find((p) => p.id === activeProfile)?.name ?? activeProfile;
  const activeInvalid = profiles.find((p) => p.id === activeProfile)?.invalid;

  return (
    <div className="ui-layout-workspace designer-layout" ref={surface}>
      <header className="designer-page-head">
        <h1 className="sr-only">Review designer</h1>
        <div className="designer-title-row">
          <div
            className="designer-review-name"
            aria-busy={profileMutationPending}
          >
            <Select
              label="Active review design"
              value={activeProfile}
              onChange={handleSwitchProfile}
              disabled={profileMutationPending}
              className="designer-profile"
              options={profiles.map((p) => ({
                value: p.id,
                label: p.invalid ? p.name + " (needs repair)" : p.name,
              }))}
            />
            <p className="text-ui-label text-ink-muted">
              {config.steps.length}{" "}
              {config.steps.length === 1 ? "step" : "steps"} ·{" "}
              {config.steps.filter((step) => step.enabled).length} enabled
            </p>
          </div>
          <div className="designer-save">
            {showBack && (
              <Button
                size="sm"
                variant="link"
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
              >
                Back to review
              </Button>
            )}
            <span role="status" className="text-ui-meta text-ink-muted">
              {!schemaDraftValid
                ? "Cannot save: fix the invalid schema draft."
                : dirty
                  ? editor.recoveryWarning
                    ? "Unsaved changes."
                    : "Unsaved draft is recoverable."
                  : saved
                    ? "Saved."
                    : "All changes saved."}
            </span>
            <Button
              variant="primary"
              onClick={handleSave}
              disabled={
                saving || profileMutationPending || !dirty || !schemaDraftValid
              }
            >
              {saving ? "Saving..." : "Save"}
            </Button>
          </div>
        </div>
        {activeInvalid && (
          <p role="alert" className="ui-notice ui-notice--warning">
            This saved review design can't run until it is repaired. Fix it here
            and save, or export it to keep a copy. Reason: {activeInvalid}
          </p>
        )}

        <WorkflowWarnings
          warnings={lintCrossStepReferences(config)}
          steps={config.steps}
          onSelect={(warning) => {
            setEditing(warning.stepId);
            setNavigatorView("stage");
            view.setWarningTarget((last) => ({
              ...warning,
              request: (last?.request ?? 0) + 1,
            }));
          }}
        />
        <div className="designer-toolbar">
          <div className="designer-stage-nav">
            {!overview && (
              <Button size="sm" variant="link" onClick={backToOverview}>
                Back to overview
              </Button>
            )}
            {schemas ? (
              <Button size="sm" onClick={() => setNavigatorView("stage")}>
                Edit stage
              </Button>
            ) : (
              <Select
                label="Choose a stage"
                placeholder="Choose a stage"
                className="designer-stage-picker"
                value={editing === "pipeline_settings" ? "" : (editing ?? "")}
                options={stageOptions(config)}
                onChange={openStage}
              />
            )}
            <Button size="sm" onClick={() => setAddStepOpen(true)}>
              + Add step
            </Button>
            {overview && (
              <Button
                size="sm"
                data-review-settings
                onClick={() => openStage("pipeline_settings")}
              >
                Review settings
              </Button>
            )}
            {editorMode === "advanced" && !schemas && (
              <Button
                size="sm"
                onClick={() => {
                  if (!editingStep) setEditing("orientation");
                  setNavigatorView("schemas");
                }}
              >
                Schemas
              </Button>
            )}
          </div>
          <div className="designer-secondary">
            <div
              className="ui-segmented"
              role="group"
              aria-label="Editor detail level"
            >
              {(["basic", "advanced"] as const).map((mode) => (
                <button
                  key={mode}
                  aria-pressed={editorMode === mode}
                  onClick={() => {
                    setEditorMode(mode);
                    if (mode === "basic" && navigatorView === "schemas")
                      setNavigatorView("stage");
                  }}
                >
                  {mode === "basic" ? "Basic" : "Advanced"}
                </button>
              ))}
            </div>
            <Button size="sm" variant="link" onClick={undo} disabled={!canUndo}>
              Undo
            </Button>
            <Button size="sm" variant="link" onClick={redo} disabled={!canRedo}>
              Redo
            </Button>
            <div className="shrink-0">
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
                side="bottom"
                align="end"
                width={288}
              >
                <MenuItem onSelect={handleExportItem} disabled={!editingStep}>
                  Export selected step
                </MenuItem>
                <MenuItem onSelect={handleExportProfile}>
                  Export design &ldquo;{activeProfileName}&rdquo;
                </MenuItem>
                <MenuItem onSelect={handleExportBundle}>
                  Export all (designs + settings)
                </MenuItem>
              </Menu>
            </div>
            <Button
              ref={profileTrigger}
              size="sm"
              className="shrink-0"
              aria-haspopup="menu"
              aria-expanded={profileMenuOpen}
              onClick={() => setProfileMenuOpen((open) => !open)}
            >
              Design actions{" "}
              <Icon name="chevron-down" className="h-3.5 w-3.5" />
            </Button>
            <Menu
              open={profileMenuOpen}
              onClose={() => setProfileMenuOpen(false)}
              anchorRef={profileTrigger}
              label="Design actions"
              side="bottom"
              width={256}
            >
              {onOpenGallery && (
                <MenuItem onSelect={onOpenGallery}>Gallery</MenuItem>
              )}
              <MenuSeparator />
              <MenuItem
                onSelect={handleNewProfile}
                disabled={profileMutationPending}
              >
                New review design
              </MenuItem>
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
              <MenuSeparator />
              <MenuItem onSelect={handleImport}>Import</MenuItem>
              <MenuSeparator />
              <MenuItem
                onSelect={handleReset}
                disabled={profileMutationPending}
                danger
              >
                Reset to defaults
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
            </Menu>
          </div>
        </div>
      </header>
      {schemas ? (
        <SchemaBrowser config={config} selected={editing} onSelect={setEditing}>
          {children}
        </SchemaBrowser>
      ) : (
        <div className="designer-canvas">{children}</div>
      )}
    </div>
  );
}

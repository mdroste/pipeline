import ReviewPdfOptions from "./ReviewPdfOptions";
import AuthorNoteField from "./AuthorNoteField";
import { authorNoteLength, AUTHOR_NOTE_MAX_CHARS } from "../lib/authorNote";
import PaperSelector from "./PaperSelector";
import WorkflowPanel from "./WorkflowPanel";
import RunModelChoice from "./RunModelChoice";
import ReviewWorkflowSteps from "./review-setup/ReviewWorkflowSteps";
import PageHeader from "../ui/PageHeader";
import { Icon } from "../ui/icons";
import type { DepsReport } from "../lib/settingsTypes";
import type {
  PipelineConfig,
  PrimaryInputSelection,
} from "../lib/workflowTypes";
import type { RunModelOverrides } from "../lib/reviewModelTypes";
import "./RunSetupPanel.css";

interface Props {
  authorNote: string;
  onAuthorNoteChange: (note: string) => void;
  authorNoteFocusKey?: number;
  configVersion: number;
  configError: string | null;
  configLoading: boolean;
  dependenciesError: string | null;
  /** What readiness last saw, for labeling providers. */
  dependencies?: DepsReport | null;
  inputMode: string;
  /** The primary input App holds; the selector shows it across remounts. */
  inputSelection: PrimaryInputSelection | null;
  /** Why the last launch failed. Start review stays available to retry. */
  launchError?: string | null;
  listenersReady: boolean;
  localLlmActive: boolean;
  paperPath: string | null;
  preparingRun: boolean;
  /** This review's model choices; null follows Settings and the workflow. */
  parallelOverrides: RunModelOverrides | null;
  selectionKey: number;
  /** Reviews already queued or running, and how many may run at once. */
  runActivity?: { running: number; queued: number; limit: number };
  onConfigureWorkflow: (stageId?: string) => void;
  onConnectModel?: () => void;
  onGenerate: () => void;
  onPaperPathChange: (path: string | null) => void;
  onInputSelectionChange: (selection: PrimaryInputSelection | null) => void;
  onPrivacyDetails: () => void;
  onParallelOverridesChange: (overrides: RunModelOverrides | null) => void;
  onProfileChange: (config?: PipelineConfig) => void;
  onRetryConfig: () => void;
  onRetryDependencies: () => void;
}

export default function RunSetupPanel({
  authorNote,
  onAuthorNoteChange,
  authorNoteFocusKey,
  configVersion,
  configError,
  configLoading,
  dependenciesError,
  dependencies,
  inputMode,
  inputSelection,
  launchError,
  listenersReady,
  localLlmActive,
  paperPath,
  preparingRun,
  parallelOverrides,
  selectionKey,
  runActivity,
  onConfigureWorkflow,
  onConnectModel,
  onGenerate,
  onPaperPathChange,
  onInputSelectionChange,
  onPrivacyDetails,
  onParallelOverridesChange,
  onProfileChange,
  onRetryConfig,
  onRetryDependencies,
}: Props) {
  const noteTooLong = authorNoteLength(authorNote) > AUTHOR_NOTE_MAX_CHARS;
  const missingInput = !paperPath && inputMode !== "none";
  const reason = preparingRun
    ? "Preparing preview…"
    : configLoading
      ? "Loading review design…"
      : configError
        ? "Reload the review design to continue."
        : missingInput
          ? "Choose a document to preview this review."
          : noteTooLong
            ? "Shorten the author note to continue."
            : !listenersReady
              ? "Connecting to the review service…"
              : null;
  const target = configError
    ? "review-workflow"
    : missingInput
      ? "review-input"
      : noteTooLong
        ? "review-note"
        : undefined;
  const focusTarget = () => {
    const section = target && document.getElementById(target);
    if (section) {
      section.scrollIntoView({ block: "center", behavior: "smooth" });
      section
        .querySelector<HTMLElement>("button, input, select, textarea")
        ?.focus();
    }
  };
  return (
    <div
      className="ui-page ui-page--setup review-setup"
      data-testid="run-setup-panel"
    >
      <PageHeader variant="standard" title="New review" />
      <div className="review-setup-grid">
        <div className="review-setup-column">
          <section
            id="review-input"
            className="review-setup-card"
            aria-label="Review document"
          >
            <h2>Document</h2>
            {inputMode === "none" ? (
              <p>This review design runs without a source document.</p>
            ) : (
              <PaperSelector
                key={selectionKey}
                inputMode={inputMode}
                selection={inputSelection}
                onPathChange={onPaperPathChange}
                onSelectionChange={onInputSelectionChange}
                disabled={configLoading || preparingRun}
              />
            )}
            {paperPath?.toLowerCase().endsWith(".pdf") &&
              inputMode !== "none" && (
                <ReviewPdfOptions
                  path={paperPath}
                  value={parallelOverrides}
                  onChange={onParallelOverridesChange}
                  disabled={configLoading || preparingRun}
                  refreshKey={configVersion}
                />
              )}
          </section>
          <section
            id="review-workflow"
            className="review-setup-card"
            aria-label="Review design"
          >
            <WorkflowPanel
              disabled={configLoading || preparingRun}
              onConfigure={() => onConfigureWorkflow()}
              onProfileChange={onProfileChange}
              refreshKey={configVersion}
            />
          </section>
          <section
            id="review-note"
            className="review-setup-card review-setup-note"
            aria-label="Review focus"
          >
            <AuthorNoteField
              key={selectionKey}
              value={authorNote}
              onChange={onAuthorNoteChange}
              focusKey={authorNoteFocusKey}
              disabled={configLoading || preparingRun}
            />
          </section>
        </div>
        <div className="review-setup-column">
          <RunModelChoice
            value={parallelOverrides}
            disabled={configLoading || preparingRun}
            dependencies={dependencies}
            localLlmActive={localLlmActive}
            refreshKey={configVersion}
            onChange={onParallelOverridesChange}
            onConnectModel={onConnectModel}
            workflowSteps={
              <ReviewWorkflowSteps
                refreshKey={configVersion}
                inputMode={inputMode}
                hasInput={!!paperPath}
                disabled={configLoading || preparingRun}
                onConfigure={onConfigureWorkflow}
              />
            }
          />
        </div>
      </div>
      {(configError || launchError || dependenciesError) && (
        <div role="alert" className="review-setup-errors">
          {configError && (
            <div>
              <p>Couldn't load the review design. {configError}</p>
              <button onClick={onRetryConfig}>Retry review design</button>
            </div>
          )}
          {launchError && <p>Couldn't prepare the review. {launchError}</p>}
          {dependenciesError && (
            <div>
              <p>Couldn't check connections and tools. {dependenciesError}</p>
              <button onClick={onRetryDependencies}>Retry check</button>
            </div>
          )}
        </div>
      )}
      <footer className="review-setup-footer">
        <div className="min-w-0 flex-1 space-y-1">
          {reason ? (
            <p
              id="review-preview-reason"
              role="status"
              className="text-ui-meta text-ink-muted"
            >
              {target ? (
                <button
                  className="underline underline-offset-2"
                  onClick={focusTarget}
                >
                  {reason}
                </button>
              ) : (
                reason
              )}
            </p>
          ) : (
            <p className="text-ui-meta text-ink-muted" role="status">
              Ready to preview
            </p>
          )}
          {runActivity && runActivity.running + runActivity.queued > 0 && (
            <p className="text-xs text-ink-muted">
              {runActivity.running} running · {runActivity.queued} queued.{" "}
              {runActivity.running + runActivity.queued >= runActivity.limit
                ? "This review will join the queue after you confirm."
                : "You can start another review."}
            </p>
          )}
        </div>
        <div className="review-preview-actions">
          <button onClick={onPrivacyDetails} className="review-privacy-link">
            Data and privacy
          </button>
          <button
            type="button"
            onClick={onGenerate}
            data-review-tour="preview"
            disabled={!!reason}
            aria-describedby={reason ? "review-preview-reason" : undefined}
            className="ui-button ui-button--primary review-preview-button"
          >
            {preparingRun ? "Preparing preview…" : "Preview"}
            <Icon name="arrow" className="h-4 w-4" />
          </button>
        </div>
      </footer>
    </div>
  );
}

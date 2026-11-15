import SetupTab from "./research-panel/SetupTab";
import MemoryTab from "./research-panel/MemoryTab";
import ResultsTab from "./research-panel/ResultsTab";
import ReleaseTab from "./research-panel/ReleaseTab";
import { useResearchPanelController } from "./research-panel/useResearchPanelController";
import type { ResearchTab } from "./research-panel/types";
import WorkspaceRecipesPanel from "./WorkspaceRecipesPanel";
import SidebarPanel, { SidebarHeader } from "./SidebarPanel";
import WorkspaceIcon from "./WorkspaceIcon";
import type {
  ConversationSnapshot,
  SessionSnapshot,
  Workspace,
} from "../lib/workbench/sessionTypes";
import type { ReviewHandoff } from "../lib/workbench/releaseTypes";

export default function WorkspaceResearchPanel({
  snapshot,
  onSnapshot,
  onClose,
  onError,
  onReviewHandoff,
  onEditHarness,
  onMove,
  workspace: projectWorkspace,
  embedded = false,
  hint,
  initialTab = "setup",
  allowedTabs,
  title = "Research",
  settingsDisabled = false,
  resultsView = "all",
  releaseView = "all",
  active = true,
  onBusy,
  beforeChange,
  onChanged,
  onActionStart,
}: {
  snapshot: ConversationSnapshot | null;
  workspace?: Workspace | null;
  embedded?: boolean;
  /** One line under the project subnav, in place of the sidebar header. */
  hint?: string;
  initialTab?: ResearchTab;
  allowedTabs?: ResearchTab[];
  title?: string;
  settingsDisabled?: boolean;
  active?: boolean;
  onBusy?: (busy: boolean) => void;
  beforeChange?: () => Promise<void>;
  /** A write here succeeded, so the surrounding project view may need to reload. */
  onChanged?: () => void;
  /** An action started, so an earlier failure no longer applies. */
  onActionStart?: () => void;
  resultsView?: "all" | "runs" | "profiles";
  releaseView?: "all" | "share" | "review";
  onSnapshot: (snapshot: SessionSnapshot) => void;
  onClose: () => void;
  onError: (message: string) => void;
  onReviewHandoff?: (handoff: ReviewHandoff) => void;
  /** Opens the full harness editor; the Setup tab is a read-only summary. */
  onEditHarness?: () => void;
  /** Opens the move dialog so an unfiled conversation can be filed in a project. */
  onMove?: () => void;
}) {
  const {
    width,
    setWidth,
    workspace,
    workspaceId,
    hostPreview,
    setHostPreview,
    tab,
    setTab,
    catalog,
    effective,
    papers,
    setProjectPaperId,
    notes,
    profiles,
    executions,
    busy,
    noteBody,
    setNoteBody,
    noteKind,
    setNoteKind,
    profileName,
    setProfileName,
    adapter,
    setAdapter,
    argv,
    setArgv,
    inputs,
    setInputs,
    outputs,
    setOutputs,
    act,
    patchSession,
    selectedPaper,
    successor,
    tabs,
  } = useResearchPanelController({
    snapshot,
    onSnapshot,
    onError,
    workspace: projectWorkspace,
    initialTab,
    active,
    onBusy,
    beforeChange,
    onChanged,
    onActionStart,
  });

  return (
    <SidebarPanel
      fill={embedded}
      // A project page has nothing to close; Escape belongs to its fields.
      onKeyDown={
        embedded
          ? undefined
          : (event) => {
              if (event.key === "Escape" && !busy) {
                event.stopPropagation();
                onClose();
              }
            }
      }
      aria-label={embedded ? title : "Research workspace"}
      side="right"
      width={width}
      defaultWidth={432}
      min={320}
      max={560}
      onResize={setWidth}
      resizeLabel="Resize research workspace"
    >
      {embedded ? (
        hint && <p className="project-page-hint">{hint}</p>
      ) : (
        <SidebarHeader
          title={title}
          actions={
            <button
              type="button"
              onClick={onClose}
              disabled={busy}
              className="workspace-sidebar-icon-button"
              aria-label="Close research workspace"
            >
              <WorkspaceIcon name="close" />
            </button>
          }
        >
          <p className="mt-1 truncate text-ui-meta text-ink-muted">
            {tab === "setup"
              ? (effective?.preset.name ?? "Loading assistant settings…")
              : workspace?.name}
          </p>
        </SidebarHeader>
      )}
      {(!allowedTabs || allowedTabs.length > 1) && (
        <div className="workspace-research-tabs ui-segmented mx-4 mb-2 shrink-0 self-start">
          {tabs
            .filter(([id]) => !allowedTabs || allowedTabs.includes(id))
            .map(([id, label]) => (
              <button
                key={id}
                type="button"
                aria-pressed={tab === id}
                onClick={() => setTab(id)}
              >
                {label}
              </button>
            ))}
        </div>
      )}
      <div className="workspace-research-content min-h-0 min-w-0 flex-1 overflow-auto p-5 text-sm">
        {!workspaceId && tab !== "setup" && (
          <div className="rounded border border-amber-200 bg-amber-50 p-3 text-amber-800 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-200">
            <p>
              Move this conversation to a project to use notes and saved
              commands.
            </p>
            {onMove && (
              <button
                type="button"
                disabled={busy}
                onClick={onMove}
                className="mt-2 rounded border border-amber-400 px-2 py-1 text-xs"
              >
                Move to project…
              </button>
            )}
          </div>
        )}

        {/* Conversation-scoped quick settings with a full editor for inheritance. */}
        {tab === "setup" && snapshot && effective && catalog && (
          <SetupTab
            workspaceId={workspaceId}
            busy={busy}
            act={act}
            patchSession={patchSession}
            successor={successor}
            snapshot={snapshot}
            effective={effective}
            catalog={catalog}
            settingsDisabled={settingsDisabled}
            onEditHarness={onEditHarness}
          />
        )}

        {tab === "recipes" && snapshot && (
          <fieldset disabled={settingsDisabled}>
            <WorkspaceRecipesPanel
              snapshot={snapshot}
              onSnapshot={onSnapshot}
              onError={onError}
            />
          </fieldset>
        )}

        {tab === "memory" && workspaceId && (
          <MemoryTab
            busy={busy}
            act={act}
            noteKind={noteKind}
            setNoteKind={setNoteKind}
            noteBody={noteBody}
            setNoteBody={setNoteBody}
            notes={notes}
            workspaceId={workspaceId}
            snapshot={snapshot}
          />
        )}

        {tab === "results" && workspaceId && (
          <ResultsTab
            busy={busy}
            act={act}
            workspace={workspace}
            profileName={profileName}
            setProfileName={setProfileName}
            adapter={adapter}
            setAdapter={setAdapter}
            argv={argv}
            setArgv={setArgv}
            inputs={inputs}
            setInputs={setInputs}
            outputs={outputs}
            setOutputs={setOutputs}
            profiles={profiles}
            hostPreview={hostPreview}
            setHostPreview={setHostPreview}
            executions={executions}
            workspaceId={workspaceId}
            snapshot={snapshot}
            resultsView={resultsView}
            onError={onError}
          />
        )}

        {tab === "release" && (
          <ReleaseTab
            workspaceId={workspaceId}
            busy={busy}
            act={act}
            papers={papers}
            selectedPaper={selectedPaper}
            setProjectPaperId={setProjectPaperId}
            snapshot={snapshot}
            releaseView={releaseView}
            onError={onError}
            onReviewHandoff={onReviewHandoff}
          />
        )}
      </div>
    </SidebarPanel>
  );
}

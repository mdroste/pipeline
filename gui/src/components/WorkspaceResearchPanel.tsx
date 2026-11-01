import SetupTab from "./research-panel/SetupTab";
import DocumentsTab from "./research-panel/DocumentsTab";
import MemoryTab from "./research-panel/MemoryTab";
import EvidenceTab from "./research-panel/EvidenceTab";
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
  ReviewHandoff,
  Workspace,
} from "../lib/workbenchTypes";

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
    sources,
    notes,
    ledger,
    profiles,
    executions,
    busy,
    query,
    setQuery,
    hits,
    setHits,
    noteBody,
    setNoteBody,
    noteKind,
    setNoteKind,
    claim,
    setClaim,
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
          <p className="mt-2 truncate text-xs text-gray-500 dark:text-gray-400">
            {tab === "setup"
              ? (effective?.preset.name ?? "Loading assistant settings…")
              : workspace?.name}
          </p>
        </SidebarHeader>
      )}
      {(!allowedTabs || allowedTabs.length > 1) && (
        <div className="workspace-research-tabs flex shrink-0 flex-wrap gap-1 border-b border-gray-200 px-5 pb-4 dark:border-gray-800">
          {tabs
            .filter(([id]) => !allowedTabs || allowedTabs.includes(id))
            .map(([id, label]) => (
              <button
                key={id}
                type="button"
                aria-pressed={tab === id}
                onClick={() => setTab(id)}
                className={`rounded-md px-2 py-1.5 text-xs font-medium transition-colors ${tab === id ? "bg-gray-100 text-gray-950 dark:bg-gray-800 dark:text-gray-100" : "text-gray-500 hover:bg-gray-50 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800/60 dark:hover:text-gray-100"}`}
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
              Move this conversation to a project to use documents, notes,
              evidence, and results.
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

        {tab === "documents" && workspaceId && (
          <DocumentsTab
            busy={busy}
            act={act}
            papers={papers}
            sources={sources}
            selectedPaper={selectedPaper}
            setProjectPaperId={setProjectPaperId}
            patchSession={patchSession}
            query={query}
            setQuery={setQuery}
            hits={hits}
            setHits={setHits}
            workspaceId={workspaceId}
            projectWorkspace={projectWorkspace}
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

        {tab === "evidence" && workspaceId && ledger && (
          <EvidenceTab
            busy={busy}
            act={act}
            claim={claim}
            setClaim={setClaim}
            workspaceId={workspaceId}
            snapshot={snapshot}
            ledger={ledger}
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

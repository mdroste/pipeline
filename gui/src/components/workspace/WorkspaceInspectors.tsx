import { lazy, Suspense } from "react";
import type { ComponentProps } from "react";
import type { ConversationSnapshot } from "../../lib/workbench/sessionTypes";
import type { ConversationProvider } from "../../lib/workspaceProviders";
import { providerName } from "../../lib/workspaceProviders";
import { turnStateLabel, turnTimeLabel } from "../../lib/workspaceTurnLabels";
import RetainedWorkspaceView from "../RetainedWorkspaceView";
import WorkspaceContextInspector from "../WorkspaceContextInspector";
import WorkspaceConversationOutline, {
  type ConversationEntry,
} from "../WorkspaceConversationOutline";

const WorkspaceFollowups = lazy(() => import("../research-programs/Followups"));
const WorkspaceResearchPanel = lazy(() => import("../WorkspaceResearchPanel"));

interface Props {
  inspector: "settings" | "activity" | "outline" | "context" | null;
  sessionId: string | null;
  snapshot: ConversationSnapshot | null;
  running: boolean;
  contextBusy: boolean;
  runtime: ConversationProvider;
  model: string;
  effort: string;
  outlineEntries: ConversationEntry[];
  selectedMessage: string | null;
  onJump: (id: string) => void;
  closeInspector: (restoreFocus?: boolean) => void;
  acceptSnapshot: ComponentProps<typeof WorkspaceResearchPanel>["onSnapshot"];
  saveCurrentDraft: NonNullable<
    ComponentProps<typeof WorkspaceResearchPanel>["beforeChange"]
  >;
  setError: (message: string | null) => void;
  onOpenSettings: () => void;
  onContextBusy: (value: boolean) => void;
  onEditHarness: () => void;
  onMove?: () => void;
  onDispatch: (value: boolean) => void;
  onRefresh: () => Promise<void>;
  onBranch: (id: string) => Promise<void>;
}

export default function WorkspaceInspectors({
  inspector,
  sessionId,
  snapshot,
  running,
  contextBusy,
  runtime,
  model,
  effort,
  outlineEntries,
  selectedMessage,
  onJump,
  closeInspector,
  acceptSnapshot,
  saveCurrentDraft,
  setError,
  onOpenSettings,
  onContextBusy,
  onEditHarness,
  onMove,
  onDispatch,
  onRefresh,
  onBranch,
}: Props) {
  return (
    <>
      <RetainedWorkspaceView
        key={`settings-${sessionId}`}
        active={inspector === "settings"}
      >
        {snapshot && (
          <Suspense
            fallback={
              <section className="workspace-panel-loading">
                Opening assistant settings…
              </section>
            }
          >
            <WorkspaceResearchPanel
              key={snapshot.session.id}
              embedded
              active={inspector === "settings"}
              title="Assistant settings"
              snapshot={snapshot}
              onSnapshot={acceptSnapshot}
              beforeChange={saveCurrentDraft}
              initialTab="setup"
              allowedTabs={["setup", "recipes"]}
              onBusy={onContextBusy}
              settingsDisabled={running || contextBusy}
              onClose={() => closeInspector()}
              onError={setError}
              onEditHarness={onEditHarness}
              onMove={onMove}
            />
          </Suspense>
        )}
      </RetainedWorkspaceView>
      <RetainedWorkspaceView
        key={`activity-${sessionId}`}
        active={inspector === "activity"}
      >
        <section
          className="workspace-simple-inspector"
          aria-label="Assistant activity"
        >
          <div className="workspace-inspector-heading">
            <h2>Activity</h2>
            <button
              type="button"
              onClick={() => closeInspector()}
              aria-label="Close inspector"
            >
              ×
            </button>
          </div>
          <p role="status">
            {running
              ? `${providerName(runtime)} is working…`
              : "No response running."}
          </p>
          {sessionId && (
            <Suspense fallback={null}>
              <WorkspaceFollowups
                key={sessionId}
                sessionId={sessionId}
                active={running || contextBusy}
                model={model}
                effort={effort}
                onError={setError}
                onDispatch={onDispatch}
                onRefresh={onRefresh}
                onBranch={onBranch}
              />
            </Suspense>
          )}
          <details className="mt-5">
            <summary className="cursor-pointer text-xs">
              Recent responses
            </summary>
            {snapshot?.turns
              .slice(-10)
              .reverse()
              .map((turn) => (
                <div key={turn.id} className="workspace-activity-row">
                  {turnStateLabel(turn)}
                  <span>{turnTimeLabel(turn.createdAt)}</span>
                </div>
              ))}
          </details>
        </section>
      </RetainedWorkspaceView>
      {inspector === "outline" ? (
        <WorkspaceConversationOutline
          embedded
          key={sessionId}
          entries={outlineEntries}
          selectedId={selectedMessage}
          onJump={(id) => {
            closeInspector(false);
            onJump(id);
          }}
          onClose={() => closeInspector()}
        />
      ) : inspector === "context" ? (
        <WorkspaceContextInspector
          sessionId={sessionId}
          workspaceName={snapshot?.workspace?.name ?? null}
          onClose={() => closeInspector()}
          onOpenSettings={onOpenSettings}
        />
      ) : null}
    </>
  );
}

import { providerName } from "../lib/workspaceProviders";
import type { AppRoute } from "../lib/router";
import { useWorkspacePageController } from "../hooks/useWorkspacePageController";
import WorkspaceMenu from "./WorkspaceMenu";
import WorkspaceToolPicker from "./WorkspaceToolPicker";
import WorkspaceComposerControls, {
  loadAccountSummary,
  loadClaudeUsage,
} from "./WorkspaceComposerControls";
import RetainedWorkspaceView from "./RetainedWorkspaceView";
import WorkspaceRailNavigation from "./WorkspaceRailNavigation";
import { useNavRailSlot } from "./nav-rail/navRailSlot";
import {
  workspaceDestinations,
  type WorkspaceDestination,
} from "../lib/workspaceNavigation";
import WorkspaceDesk from "./WorkspaceDesk";
import WorkspaceConversationView from "./WorkspaceConversationView";
import WorkspaceContextInspector from "./WorkspaceContextInspector";
import WorkspaceContextTray from "./WorkspaceContextTray";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { conversationClient } from "../lib/workbench/conversationClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import { turnStateLabel, turnTimeLabel } from "../lib/workspaceTurnLabels";
import "./WorkspaceNavigation.css";
import "./WorkspaceInspector.css";
import RequestCard from "./WorkspaceRequestCard";
import WorkspaceBackgroundTurnNotice from "./WorkspaceBackgroundTurnNotice";
import useComposerFileDrop from "../hooks/useComposerFileDrop";
import IconButton from "../ui/IconButton";
import { MenuItem, MenuLabel, MenuSeparator } from "../ui/Menu";
import { Icon } from "../ui/icons";
import WorkspaceIcon from "./WorkspaceIcon";
import WorkspaceComposerMenu from "./WorkspaceComposerMenu";
import WorkspaceConversationOutline from "./WorkspaceConversationOutline";
import { MoveConversationDialog } from "./WorkspaceConversationActions";
import type { ReviewHandoff } from "../lib/workbenchTypes";

const WorkspaceProjectSurface = lazy(() => import("./WorkspaceProjectSurface"));
const WorkspaceProjectDialog = lazy(() => import("./WorkspaceProjectDialog"));
const WorkspaceFollowups = lazy(() => import("./research-programs/Followups"));
const WorkspaceTaskCards = lazy(() => import("./WorkspaceTaskCards"));
const WorkspaceResearchPanel = lazy(() => import("./WorkspaceResearchPanel"));
const WorkspaceHarnessEditor = lazy(() => import("./WorkspaceHarnessEditor"));
/** Placeholder title until the first exchange is auto-titled or the user renames. */
const DEFAULT_TITLE = "New conversation";

export default function WorkspacePage({
  onAllProjects,
  onOpenConversation,
  onSaveHandlerChange,
  entryTarget,
  entryRequest = 0,
  entrySurface,
  newProjectRequest = 0,
  onNewProjectRequestHandled,
  onReviewHandoff,
  onTasks,
}: {
  onAllProjects?: () => void;
  /** Opens any conversation, including one in another project. */
  onOpenConversation?: (sessionId: string) => void;
  onSaveHandlerChange?: (save: (() => Promise<boolean>) | null) => void;
  entryTarget?: Extract<AppRoute, { page: "workspace" }>;
  entryRequest?: number;
  entrySurface?: "chat" | "project";
  newProjectRequest?: number;
  onNewProjectRequestHandled?: () => void;
  onTasks?: (sessionId: string, taskId?: string) => void;
  onReviewHandoff?: (handoff: ReviewHandoff) => void;
}) {
  const {
    layout: { toolPickerOpen, setToolPickerOpen, conversationMenuRef },
    inspector: {
      inspector,
      harnessEditor,
      setHarnessEditor,
      openInspector,
      closeInspector,
      setInspector,
    },
    navigation: {
      workspaces,
      workspaceId,
      sessions,
      hasOlderSessions,
      loadOlderSessions,
      sessionId,
      tasksEnabled,
      projectRequest,
      resetRequest,
      destination,
      assistantRequest,
      surface,
      projectDialog,
      setProjectDialog,
      loadSessions,
      selectProject,
      selectSession,
      createSession,
      projectCreated,
      openResearchConversation,
      updateProjectWorkspace,
      recordDestination,
      navigateProject,
      currentWorkspace,
      setProjectRequest,
    },
    conversation: {
      snapshot,
      models,
      model,
      setModel,
      runtime,
      setRuntime,
      effort,
      setEffort,
      streamStore,
      pendingUser,
      active,
      submitting,
      submittingRef,
      hydrate,
      exportCurrent,
      stopCurrent,
      stopping,
      exporting,
      actionError,
      actionNotice,
      send,
      resolve,
      visibleRequests,
      requestItem,
      backgroundTurn,
      notice,
      acceptSnapshot,
      setSubmitting,
    },
    persistence: { draft, editDraft, saveCurrentDraft, saveSelection },
    context: { contextBusy, setContextBusy, contextBusyRef },
    mutations: {
      moveTarget,
      setMoveTarget,
      moving,
      moveError,
      setMoveError,
      archiveSession,
      renameSession,
      generateTitle,
      deleteSession,
      moveSession,
    },
    feedback: { error, setError },
    transcript: {
      selectedMessage,
      messageRef,
      transcriptRef,
      renderedItems,
      outlineEntries,
      jumpToMessage,
      latest,
      allRenderedItems,
      transcriptStart,
      transcriptEnd,
      setTranscriptPage,
      loadEarlier,
      loadingEarlier,
      followLatestRef,
    },
  } = useWorkspacePageController({
    entryTarget,
    entryRequest,
    entrySurface,
    newProjectRequest,
    onNewProjectRequestHandled,
  });
  const [conversationFirst, setConversationFirst] = useState(0);
  const assistantPane = useRef<HTMLElement>(null);
  const setComposerBusy = (value: boolean) => {
    contextBusyRef.current = value;
    setContextBusy(value);
    if (value) setInspector(null);
  };
  const dropState = useComposerFileDrop({
    targetRef: assistantPane,
    workspaceId:
      snapshot?.session.id === sessionId ? snapshot?.session.workspaceId : null,
    sessionId,
    blocked: submitting || Boolean(active) || contextBusy || harnessEditor,
    onBusy: setComposerBusy,
    onError: setError,
  });
  const [projectBrief, setProjectBrief] = useState<string | null>(null);

  useEffect(() => {
    onSaveHandlerChange?.(async () => {
      if (contextBusy || harnessEditor) {
        setError("Finish the open editor before leaving this project.");
        return false;
      }
      try {
        await saveCurrentDraft();
        return true;
      } catch (cause) {
        setError(workbenchErrorMessage(cause));
        return false;
      }
    });
    return () => onSaveHandlerChange?.(null);
  }, [
    onSaveHandlerChange,
    saveCurrentDraft,
    contextBusy,
    harnessEditor,
    setError,
  ]);

  // The sidebar's actions run the latest closures without re-reporting.
  const railActions = useRef({
    newConversation: () => undefined as void,
    openView: (_destination: WorkspaceDestination) => undefined as void,
  });
  railActions.current = {
    newConversation: () =>
      void createSession().catch((cause) =>
        setError(workbenchErrorMessage(cause)),
      ),
    openView: navigateProject,
  };
  const rail = useNavRailSlot();
  const setRailWorkspace = rail?.setWorkspace;
  const projectName = currentWorkspace?.name ?? null;
  useLayoutEffect(() => {
    setRailWorkspace?.({
      projectId: workspaceId,
      projectName,
      newConversation: () => railActions.current.newConversation(),
      openView: (view) => railActions.current.openView(view),
    });
    return () => setRailWorkspace?.(null);
  }, [setRailWorkspace, workspaceId, projectName]);
  const dismissRail = () => rail?.dismissRef.current?.();
  // The project pane is memoized, so typing here leaves it alone. Its callbacks
  // stay stable and run the latest closures.
  const paneActions = useRef({ createSession, onAllProjects, onReviewHandoff });
  paneActions.current = { createSession, onAllProjects, onReviewHandoff };
  const startConversation = useCallback(
    () => paneActions.current.createSession(),
    [],
  );
  const showAllProjects = useCallback(
    () => paneActions.current.onAllProjects?.(),
    [],
  );
  const handOffReview = useCallback(
    (handoff: ReviewHandoff) => paneActions.current.onReviewHandoff?.(handoff),
    [],
  );
  const openConversationFirst = useCallback(
    () => setConversationFirst((request) => request + 1),
    [],
  );
  const currentSession = snapshot?.session;
  const paneSessions = useMemo(
    () =>
      sessions.map((session) =>
        currentSession?.id === session.id ? currentSession : session,
      ),
    [sessions, currentSession],
  );
  const reportError = (cause: unknown) =>
    setError(workbenchErrorMessage(cause));
  return (
    <div className="workspace-chat-shell flex h-full min-h-0 min-w-0 bg-white dark:bg-neutral-950">
      {rail?.slot &&
        createPortal(
          <WorkspaceRailNavigation
            key={workspaceId ?? "unfiled"}
            project={currentWorkspace}
            projects={workspaces}
            destination={destination}
            sessions={sessions}
            hasOlderSessions={hasOlderSessions}
            onLoadOlderSessions={() => void loadOlderSessions()}
            sessionId={sessionId}
            chatOpen={surface === "chat"}
            background={backgroundTurn}
            disabled={contextBusy || harnessEditor}
            onNavigate={(view) => {
              dismissRail();
              navigateProject(view);
            }}
            onSelectSession={(id) => {
              dismissRail();
              void selectSession(id).catch(reportError);
            }}
            onCreateSession={() => {
              dismissRail();
              void createSession().catch(reportError);
            }}
            onSelectProject={(id) => {
              dismissRail();
              void selectProject(id).catch(reportError);
            }}
            onNewProject={() => {
              dismissRail();
              setProjectDialog(true);
            }}
            onAllProjects={() => {
              dismissRail();
              onAllProjects?.();
            }}
            conversationActions={(session) => ({
              onRename: () => void renameSession(session).catch(reportError),
              onGenerateTitle: () =>
                void generateTitle(session).catch((cause) =>
                  setError(
                    `Title could not be generated: ${workbenchErrorMessage(cause)}`,
                  ),
                ),
              onMove: () => {
                setMoveError(null);
                setMoveTarget(session);
              },
              onRevealFiles: session.workspaceId
                ? undefined
                : () =>
                    void conversationClient
                      .revealConversationFolder(session.id)
                      .catch(reportError),
              onArchive: () => void archiveSession(session).catch(reportError),
              onDelete: () => void deleteSession(session).catch(reportError),
            })}
          />,
          rail.slot,
        )}
      {toolPickerOpen && workspaceId && (
        <WorkspaceToolPicker
          workspaceId={workspaceId}
          current={destination}
          onChoose={navigateProject}
          onClose={() => setToolPickerOpen(false)}
        />
      )}

      {projectDialog && (
        <Suspense fallback={null}>
          <WorkspaceProjectDialog
            onClose={() => setProjectDialog(false)}
            onCreated={projectCreated}
          />
        </Suspense>
      )}
      {moveTarget && (
        <MoveConversationDialog
          key={moveTarget.id}
          session={moveTarget}
          workspaces={workspaces}
          busy={moving}
          error={moveError}
          onMove={(target) => void moveSession(moveTarget, target)}
          onClose={() => {
            if (!moving) setMoveTarget(null);
          }}
        />
      )}
      <WorkspaceDesk
        key={workspaceId ?? "unfiled"}
        workspaceId={workspaceId}
        projectName={currentWorkspace?.name}
        toolLabel={workspaceDestinations[destination].label}
        onTools={workspaceId ? () => setToolPickerOpen(true) : undefined}
        toolsDisabled={contextBusy || harnessEditor}
        attentionCount={visibleRequests.length + (backgroundTurn?.waiting ?? 0)}
        active={Boolean(active) || submitting}
        onStop={active ? () => void stopCurrent() : undefined}
        stopping={stopping}
        feedback={
          actionError ? (
            <p
              role="alert"
              className="p-3 text-sm text-red-700 dark:text-red-300"
            >
              {actionError}
            </p>
          ) : actionNotice ? (
            <p role="status" className="p-3 text-sm">
              {actionNotice}
            </p>
          ) : null
        }
        assistantRequest={assistantRequest}
        projectRequest={projectRequest}
        resetRequest={resetRequest}
        conversationFirstRequest={conversationFirst}
        project={
          workspaceId || (harnessEditor && snapshot) ? (
            <>
              {workspaceId && (
                <div hidden={harnessEditor} className="workspace-retained-view">
                  <Suspense
                    fallback={
                      <section className="workspace-panel-loading">
                        Opening project…
                      </section>
                    }
                  >
                    <WorkspaceProjectSurface
                      key={workspaceId}
                      workspaceId={workspaceId}
                      sessionId={sessionId}
                      sessions={paneSessions}
                      onResumeSession={selectSession}
                      onStartConversation={startConversation}
                      onAllProjects={
                        onAllProjects ? showAllProjects : undefined
                      }
                      destination={destination}
                      onDestination={recordDestination}
                      navigationInSidebar
                      snapshot={snapshot}
                      onSnapshot={acceptSnapshot}
                      onConversation={openResearchConversation}
                      onConversationFirst={openConversationFirst}
                      onProjectBrief={setProjectBrief}
                      onWorkspaceChanged={updateProjectWorkspace}
                      onReviewHandoff={
                        onReviewHandoff ? handOffReview : undefined
                      }
                    />
                  </Suspense>
                </div>
              )}
              {snapshot && harnessEditor && (
                <Suspense
                  fallback={
                    <section className="workspace-panel-loading">
                      Opening agent profiles…
                    </section>
                  }
                >
                  <WorkspaceHarnessEditor
                    key={snapshot.session.id}
                    snapshot={snapshot}
                    onSnapshot={acceptSnapshot}
                    beforeChange={saveCurrentDraft}
                    disabled={Boolean(active) || submitting}
                    onBusy={(value) => {
                      contextBusyRef.current = value;
                      setContextBusy(value);
                    }}
                    onClose={() => {
                      setHarnessEditor(false);
                      setInspector("settings");
                    }}
                  />
                </Suspense>
              )}
            </>
          ) : null
        }
      >
        <section
          ref={assistantPane}
          className="workspace-assistant relative flex min-h-0 min-w-0 flex-1 flex-col"
          onKeyDown={(event) => {
            if (
              event.key === "Escape" &&
              inspector &&
              !contextBusyRef.current
            ) {
              event.stopPropagation();
              closeInspector();
            }
          }}
        >
          <header className="workspace-chat-header workspace-chat-header-minimal">
            <div className="workspace-chat-heading">
              <h1 className="truncate text-sm font-semibold">
                {snapshot?.session.title ?? DEFAULT_TITLE}
              </h1>
              {(active || submitting) && (
                <p role="status">{providerName(runtime)} is working…</p>
              )}
              {/* The brief heads the conversation only when the chat stands
                  alone; beside the overview it would repeat what is on screen. */}
              {workspaceId && projectBrief && (
                <p
                  className="hidden max-w-[720px] truncate [[data-layout=assistant]_&]:block"
                  title={projectBrief}
                >
                  {projectBrief}
                </p>
              )}
            </div>
            {workspaceId && (
              <IconButton
                label="New conversation"
                tooltipSide="bottom"
                disabled={contextBusy || harnessEditor}
                onClick={() =>
                  void createSession().catch((cause) =>
                    setError(workbenchErrorMessage(cause)),
                  )
                }
              >
                <Icon name="plus" />
              </IconButton>
            )}
            <WorkspaceMenu
              label="Conversation menu"
              triggerRef={conversationMenuRef}
              disabled={!snapshot || contextBusy || harnessEditor}
            >
              {sessions.length > 1 && (
                <>
                  <MenuLabel>Conversations</MenuLabel>
                  {sessions.map((session) => (
                    <MenuItem
                      key={session.id}
                      checked={session.id === sessionId}
                      description={session.archivedAt ? "Archived" : undefined}
                      onSelect={() => {
                        if (session.id !== sessionId)
                          void selectSession(session.id).catch((cause) =>
                            setError(workbenchErrorMessage(cause)),
                          );
                      }}
                    >
                      {session.title}
                    </MenuItem>
                  ))}
                  <MenuSeparator />
                </>
              )}
              <MenuItem
                onSelect={() =>
                  openInspector(
                    "outline",
                    conversationMenuRef.current ?? undefined,
                  )
                }
              >
                Outline
              </MenuItem>
              <MenuItem
                onSelect={() =>
                  openInspector(
                    "context",
                    conversationMenuRef.current ?? undefined,
                  )
                }
              >
                Context
              </MenuItem>
              <MenuItem
                onSelect={() =>
                  openInspector(
                    "activity",
                    conversationMenuRef.current ?? undefined,
                  )
                }
              >
                Activity & follow-ups
              </MenuItem>
              <MenuSeparator />
              <MenuItem
                disabled={exporting}
                onSelect={() => void exportCurrent()}
              >
                {exporting ? "Exporting…" : "Export conversation"}
              </MenuItem>
              {snapshot && (
                <MenuItem
                  onSelect={() =>
                    void archiveSession(snapshot.session).catch((cause) =>
                      setError(workbenchErrorMessage(cause)),
                    )
                  }
                >
                  {snapshot.session.archivedAt
                    ? "Restore conversation"
                    : "Archive conversation"}
                </MenuItem>
              )}
            </WorkspaceMenu>
          </header>
          <WorkspaceConversationView
            assistantName={providerName(runtime)}
            header={null}
            dropState={dropState}
            inspectorOpen={Boolean(inspector)}
            inspector={
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
                        onBusy={(value) => {
                          contextBusyRef.current = value;
                          setContextBusy(value);
                        }}
                        settingsDisabled={
                          Boolean(active) || submitting || contextBusy
                        }
                        onClose={() => closeInspector()}
                        onError={setError}
                        onEditHarness={() => {
                          setInspector(null);
                          setHarnessEditor(true);
                          setProjectRequest((value) => value + 1);
                        }}
                        onMove={() => {
                          setMoveError(null);
                          setMoveTarget(snapshot.session);
                        }}
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
                      {active || submitting
                        ? `${providerName(runtime)} is working…`
                        : "No response running."}
                    </p>
                    {sessionId && (
                      <Suspense fallback={null}>
                        <WorkspaceFollowups
                          key={sessionId}
                          sessionId={sessionId}
                          active={Boolean(active) || contextBusy || submitting}
                          model={model}
                          effort={effort}
                          onError={setError}
                          onDispatch={(value) => {
                            submittingRef.current = value;
                            setSubmitting(value);
                          }}
                          onRefresh={() => hydrate()}
                          onBranch={async (id) => {
                            await loadSessions(workspaceId);
                            await selectSession(id);
                          }}
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
                      jumpToMessage(id);
                    }}
                    onClose={() => closeInspector()}
                  />
                ) : inspector === "context" ? (
                  <WorkspaceContextInspector
                    sessionId={sessionId}
                    workspaceName={snapshot?.workspace?.name ?? null}
                    onClose={() => closeInspector()}
                    onOpenSettings={() => setInspector("settings")}
                  />
                ) : null}
              </>
            }
            composerControls={
              <WorkspaceComposerControls
                models={models}
                model={model}
                effort={effort}
                disabled={
                  !snapshot || contextBusy || Boolean(active) || submitting
                }
                runtime={runtime}
                loadAccount={
                  runtime === "codex" ? loadAccountSummary : undefined
                }
                loadClaudeUsage={
                  runtime === "claude" ? loadClaudeUsage : undefined
                }
                onRuntimeChange={(next) => {
                  setRuntime(next);
                  setModel("");
                  setEffort("");
                  saveSelection("", "", next);
                }}
                onChange={(nextModel, nextEffort) => {
                  setModel(nextModel);
                  setEffort(nextEffort);
                  saveSelection(nextModel, nextEffort);
                }}
              />
            }
            composerSetup={
              snapshot && (
                <IconButton
                  label="Assistant settings"
                  tooltip={
                    typeof snapshot.session.overrides.mode === "string"
                      ? `Assistant settings · ${snapshot.session.overrides.mode === "inspect" ? "Read only" : "Allow edits"}`
                      : "Assistant settings"
                  }
                  disabled={contextBusy || harnessEditor}
                  onClick={(event) =>
                    openInspector("settings", event.currentTarget)
                  }
                >
                  <WorkspaceIcon name="settings" size={18} />
                </IconButton>
              )
            }
            snapshot={snapshot}
            renderedItems={renderedItems}
            totalItems={allRenderedItems.length}
            hasEarlier={Boolean(snapshot?.olderCursor)}
            loadingEarlier={loadingEarlier}
            pendingUser={pendingUser}
            streamStore={streamStore}
            selectedMessage={selectedMessage}
            transcriptStart={transcriptStart}
            transcriptEnd={transcriptEnd}
            transcriptRef={transcriptRef}
            messageRef={messageRef}
            error={error}
            notice={notice}
            draft={draft}
            disabled={
              !sessionId ||
              snapshot?.session.id !== sessionId ||
              submitting ||
              Boolean(active) ||
              contextBusy
            }
            active={Boolean(active)}
            stopping={stopping}
            submitting={submitting}
            contextBusy={contextBusy}
            onDraft={editDraft}
            onSend={() => void send()}
            onStop={() => void stopCurrent()}
            onNewConversation={
              sessionId
                ? undefined
                : () => void createSession().catch(reportError)
            }
            onLatest={latest}
            onEarlier={() => void loadEarlier()}
            onNewer={() => setTranscriptPage((v) => Math.max(0, v - 1))}
            followLatestRef={followLatestRef}
            requests={
              <>
                {backgroundTurn && (
                  <WorkspaceBackgroundTurnNotice
                    title={backgroundTurn.title}
                    waiting={backgroundTurn.waiting}
                    onOpen={() =>
                      onOpenConversation
                        ? onOpenConversation(backgroundTurn.sessionId)
                        : void selectSession(backgroundTurn.sessionId).catch(
                            (cause) => setError(workbenchErrorMessage(cause)),
                          )
                    }
                    onOpenAutomations={
                      backgroundTurn.automated && onTasks
                        ? () => onTasks(backgroundTurn.sessionId)
                        : undefined
                    }
                  />
                )}
                {visibleRequests.map((request) => (
                  <RequestCard
                    key={`${request.epoch}:${String(request.requestId)}`}
                    event={request}
                    item={requestItem(request)}
                    onResolve={resolve}
                  />
                ))}
              </>
            }
            contextTray={
              <>
                {snapshot?.workspace && (
                  <WorkspaceContextTray
                    key={snapshot.session.id}
                    workspaceId={snapshot.workspace.id}
                    sessionId={snapshot.session.id}
                    disabled={contextBusy || Boolean(active) || submitting}
                    onError={setError}
                  />
                )}{" "}
              </>
            }
            taskCards={
              tasksEnabled && sessionId && onTasks ? (
                <Suspense fallback={null}>
                  <WorkspaceTaskCards
                    key={sessionId}
                    sessionId={sessionId}
                    onOpen={(id) => onTasks(sessionId, id)}
                  />
                </Suspense>
              ) : null
            }
            composerMenu={
              <WorkspaceComposerMenu
                key={sessionId ?? "empty"}
                snapshot={snapshot}
                disabled={submitting || Boolean(active) || contextBusy}
                onSnapshot={acceptSnapshot}
                onBusy={setComposerBusy}
                onBrowseProjects={() => onAllProjects?.()}
                onTasks={
                  sessionId && onTasks ? () => onTasks(sessionId) : undefined
                }
              />
            }
          />
        </section>
      </WorkspaceDesk>
    </div>
  );
}

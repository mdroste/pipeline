import { providerName } from "../lib/workspaceProviders";
import type { AppRoute } from "../lib/router";
import { useWorkspacePageController } from "../hooks/useWorkspacePageController";
import WorkspaceMenu from "./WorkspaceMenu";
import WorkspaceComposerToolbar from "./workspace/WorkspaceComposerToolbar";
import WorkspaceRailNavigation from "./WorkspaceRailNavigation";
import { useWorkspaceRail } from "./workspace/useWorkspaceRail";
import { workspaceDestinations } from "../lib/workspaceNavigation";
import WorkspaceDesk from "./WorkspaceDesk";
import WorkspaceConversationView from "./WorkspaceConversationView";
import WorkspaceContextTray from "./WorkspaceContextTray";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { conversationClient } from "../lib/workbench/conversationClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
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
import WorkspaceInspectors from "./workspace/WorkspaceInspectors";
import WorkspaceDialogs from "./workspace/WorkspaceDialogs";
import type { ReviewHandoff } from "../lib/workbench/releaseTypes";
import ReportIssueLink from "./ReportIssueLink";
import type { FileLocation } from "../lib/fileLinks";
import type { WorkbenchSession } from "../lib/workbench/sessionTypes";
import useConversationTabs, {
  neighbourTab,
} from "../hooks/workspace/useConversationTabs";
import ConversationTabs from "./workspace/ConversationTabs";

const WorkspaceProjectSurface = lazy(() => import("./WorkspaceProjectSurface"));
const ConversationArtifacts = lazy(() => import("./ConversationArtifacts"));
const WorkspaceTaskCards = lazy(() => import("./WorkspaceTaskCards"));
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
      closeSession,
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
      warnings,
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
      deleteSession: deleteSessionRecord,
      moveSession: moveSessionRecord,
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
  const artifactsId = useId();
  const artifactScope =
    snapshot && snapshot.session.id === sessionId
      ? JSON.stringify([
          sessionId,
          snapshot.workspace?.id,
          snapshot.workspace?.root,
          snapshot.session.overrides?.projectCheckpointId,
        ])
      : null;
  const [artifactSelection, setArtifactSelection] = useState<{
    scope: string;
    location: FileLocation | null;
  } | null>(null);
  const artifactsOpen = artifactSelection?.scope === artifactScope;
  if (artifactSelection && !artifactsOpen) setArtifactSelection(null);
  const collapseArtifacts = useCallback(() => setArtifactSelection(null), []);
  const openArtifacts = useCallback(
    (location: FileLocation | null) => {
      if (artifactScope)
        setArtifactSelection({ scope: artifactScope, location });
    },
    [artifactScope],
  );
  const assistantPane = useRef<HTMLElement>(null);
  const setComposerBusy = (value: boolean) => {
    contextBusyRef.current = value;
    setContextBusy(value);
    if (value) setInspector(null);
  };
  const [pendingAttachment, setPendingAttachment] = useState<{
    sessionId: string;
    paths: string[];
    view: "import" | "documents";
  } | null>(null);
  const dropState = useComposerFileDrop({
    targetRef: assistantPane,
    workspaceId:
      snapshot?.session.id === sessionId ? snapshot?.session.workspaceId : null,
    projectId: workspaceId,
    sessionId,
    blocked: submitting || Boolean(active) || contextBusy || harnessEditor,
    onBusy: setComposerBusy,
    onError: setError,
    onNeedsProject: (paths) => {
      if (sessionId) setPendingAttachment({ sessionId, paths, view: "import" });
    },
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

  const { rail, dismissRail } = useWorkspaceRail(
    workspaceId,
    currentWorkspace?.name ?? null,
    createSession,
    navigateProject,
    setError,
  );
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
  const conversationTabs = useConversationTabs(
    workspaceId,
    sessionId,
    paneSessions,
  );
  // A conversation deleted or moved to another project leaves this project's
  // tabs; one moved here from elsewhere gets its tab when it is selected.
  const deleteSession = async (session: WorkbenchSession) => {
    const deleted = await deleteSessionRecord(session);
    if (deleted) conversationTabs.forget(session.id, session.workspaceId);
  };
  const moveSession = async (
    session: WorkbenchSession,
    target: string | null,
  ) => {
    const moved = await moveSessionRecord(session, target);
    if (moved && target !== session.workspaceId)
      conversationTabs.forget(session.id, session.workspaceId);
    return moved;
  };
  const closeTab = (id: string) => {
    const next =
      id === sessionId ? neighbourTab(conversationTabs.tabs, id) : undefined;
    conversationTabs.close(id);
    if (next) void selectSession(next).catch(reportError);
    else if (next === null) void closeSession().catch(reportError);
  };
  return (
    <div className="workspace-chat-shell flex h-full min-h-0 min-w-0 bg-surface">
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
      <WorkspaceDialogs
        workspaceId={workspaceId}
        destination={destination}
        toolPickerOpen={toolPickerOpen}
        navigateProject={navigateProject}
        setToolPickerOpen={setToolPickerOpen}
        projectDialog={projectDialog}
        setProjectDialog={setProjectDialog}
        projectCreated={projectCreated}
        moveTarget={moveTarget}
        workspaces={workspaces}
        moving={moving}
        moveError={moveError}
        moveSession={moveSession}
        setMoveTarget={setMoveTarget}
      />
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
            <p role="alert" className="p-3 text-sm text-danger">
              {actionError}
              <ReportIssueLink />
            </p>
          ) : actionNotice ? (
            <p role="status" className="p-3 text-sm">
              {actionNotice}
            </p>
          ) : null
        }
        assistantFeedback={
          <>
            {warnings.map((warning) => (
              <p key={warning} role="status" className="p-3 text-sm">
                {warning}
              </p>
            ))}
            {error && (
              <p role="alert" className="p-3 text-sm text-danger">
                {error}{" "}
                <button
                  type="button"
                  className="underline"
                  onClick={() => setError(null)}
                >
                  Dismiss
                </button>
                <ReportIssueLink />
              </p>
            )}
          </>
        }
        assistantRequest={assistantRequest}
        projectRequest={projectRequest}
        resetRequest={resetRequest}
        conversationFirstRequest={conversationFirst}
        artifacts={
          artifactsOpen && snapshot
            ? {
                id: artifactsId,
                onCollapse: collapseArtifacts,
                content: (
                  <Suspense
                    fallback={
                      <p
                        className="px-4 py-2 text-ui-meta text-ink-muted"
                        role="status"
                      >
                        Loading artifacts…
                      </p>
                    }
                  >
                    <ConversationArtifacts
                      snapshot={snapshot}
                      active={Boolean(active)}
                      initial={artifactSelection.location}
                      onClose={collapseArtifacts}
                    />
                  </Suspense>
                ),
              }
            : undefined
        }
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
          <header className="workspace-chat-header workspace-chat-header-minimal workspace-chat-header-tabs">
            {/* The tabs name the conversation on screen. */}
            <div className="sr-only">
              <h1>{snapshot?.session.title ?? DEFAULT_TITLE}</h1>
              {(active || submitting) && (
                <p role="status">{providerName(runtime)} is working…</p>
              )}
            </div>
            {conversationTabs.tabs.length > 0 ? (
              <ConversationTabs
                tabs={conversationTabs.tabs}
                active={sessionId}
                working={Boolean(active) || submitting}
                disabled={contextBusy || harnessEditor}
                onSelect={(id) => {
                  if (id !== sessionId)
                    void selectSession(id).catch(reportError);
                }}
                onClose={closeTab}
                onMove={conversationTabs.move}
              />
            ) : (
              <span className="workspace-conversation-tabs-empty">
                {snapshot?.session.title ?? DEFAULT_TITLE}
              </span>
            )}
            {workspaceId && (
              <IconButton
                label="New conversation"
                size="sm"
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
            <IconButton
              label="Artifacts"
              size="sm"
              tooltipSide="bottom"
              aria-controls={artifactsOpen ? artifactsId : undefined}
              aria-expanded={artifactsOpen}
              disabled={!artifactScope}
              onClick={() =>
                artifactsOpen ? collapseArtifacts() : openArtifacts(null)
              }
            >
              <Icon name="folder" />
            </IconButton>
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
          {/* The brief heads the conversation only when the chat stands
              alone; beside the overview it would repeat what is on screen. */}
          {workspaceId && projectBrief && (
            <p
              className="workspace-chat-brief hidden [[data-layout=assistant]_&]:block"
              title={projectBrief}
            >
              {projectBrief}
            </p>
          )}
          <WorkspaceConversationView
            assistantName={providerName(runtime)}
            header={null}
            onOpenFile={openArtifacts}
            dropState={dropState}
            inspectorOpen={Boolean(inspector)}
            inspector={
              <WorkspaceInspectors
                inspector={inspector}
                sessionId={sessionId}
                snapshot={snapshot}
                running={Boolean(active) || submitting}
                contextBusy={contextBusy}
                runtime={runtime}
                model={model}
                effort={effort}
                outlineEntries={outlineEntries}
                selectedMessage={selectedMessage}
                onJump={jumpToMessage}
                closeInspector={closeInspector}
                acceptSnapshot={acceptSnapshot}
                saveCurrentDraft={saveCurrentDraft}
                setError={setError}
                onOpenSettings={() => setInspector("settings")}
                onContextBusy={(value) => {
                  contextBusyRef.current = value;
                  setContextBusy(value);
                }}
                onEditHarness={() => {
                  setInspector(null);
                  setHarnessEditor(true);
                  setProjectRequest((value) => value + 1);
                }}
                onMove={
                  snapshot
                    ? () => {
                        setMoveError(null);
                        setMoveTarget(snapshot.session);
                      }
                    : undefined
                }
                onDispatch={(value) => {
                  submittingRef.current = value;
                  setSubmitting(value);
                }}
                onRefresh={() => hydrate()}
                onBranch={async (id) => {
                  await loadSessions(workspaceId, undefined, true);
                  await selectSession(id);
                }}
              />
            }
            composerControls={
              <WorkspaceComposerToolbar
                models={models}
                model={model}
                effort={effort}
                snapshot={snapshot}
                disabled={
                  !snapshot || contextBusy || Boolean(active) || submitting
                }
                runtime={runtime}
                setRuntime={setRuntime}
                setModel={setModel}
                setEffort={setEffort}
                saveSelection={saveSelection}
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
            warnings={warnings}
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
              sessionId && onTasks ? (
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
                onMoveToProject={(id, view) => {
                  if (!snapshot) return Promise.resolve(false);
                  setPendingAttachment((current) => ({
                    sessionId: snapshot.session.id,
                    paths:
                      current?.sessionId === snapshot.session.id
                        ? current.paths
                        : [],
                    view,
                  }));
                  return moveSession(snapshot.session, id);
                }}
                onProjectCreated={updateProjectWorkspace}
                moveError={moveError}
                resumeView={
                  pendingAttachment?.sessionId === sessionId
                    ? pendingAttachment.view
                    : undefined
                }
                pendingFiles={
                  pendingAttachment?.sessionId === sessionId
                    ? pendingAttachment.paths
                    : undefined
                }
                onPendingFilesConsumed={() => setPendingAttachment(null)}
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

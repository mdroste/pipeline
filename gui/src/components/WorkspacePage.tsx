import type { AppRoute } from "../lib/router";
import { useWorkspacePageController } from "../hooks/useWorkspacePageController";
import WorkspaceMenu from "./WorkspaceMenu";
import WorkspaceToolPicker from "./WorkspaceToolPicker";
import WorkspaceComposerControls, {
  loadAccountSummary,
} from "./WorkspaceComposerControls";
import RetainedWorkspaceView from "./RetainedWorkspaceView";
import WorkspaceProjectNavigation from "./WorkspaceProjectNavigation";
import { workspaceDestinations } from "../lib/workspaceNavigation";
import WorkspaceDesk from "./WorkspaceDesk";
import WorkspaceConversationView from "./WorkspaceConversationView";
import WorkspaceContextInspector from "./WorkspaceContextInspector";
import WorkspaceContextTray from "./WorkspaceContextTray";
import { lazy, Suspense, useEffect, useId, useRef, useState } from "react";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import "./WorkspaceNavigation.css";
import "./WorkspaceInspector.css";
import RequestCard from "./WorkspaceRequestCard";
import useComposerFileDrop from "../hooks/useComposerFileDrop";
import IconButton from "../ui/IconButton";
import Select from "../ui/Select";
import { MenuItem, MenuLabel, MenuSeparator } from "../ui/Menu";
import { Icon } from "../ui/icons";
import WorkspaceIcon from "./WorkspaceIcon";
import SidebarPanel, { SidebarHeader } from "./SidebarPanel";
import WorkspaceComposerMenu from "./WorkspaceComposerMenu";
import WorkspaceConversationOutline from "./WorkspaceConversationOutline";
import {
  ConversationMenu,
  MoveConversationDialog,
} from "./WorkspaceConversationActions";
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
  onSaveHandlerChange,
  entryTarget,
  entryRequest = 0,
  entrySurface,
  newProjectRequest = 0,
  onNewProjectRequestHandled,
  onOpenSettings,
  onReviewHandoff,
  onTasks,
}: {
  onAllProjects?: () => void;
  onSaveHandlerChange?: (save: (() => Promise<boolean>) | null) => void;
  entryTarget?: Extract<AppRoute, { page: "workspace" }>;
  entryRequest?: number;
  entrySurface?: "chat" | "project";
  newProjectRequest?: number;
  onNewProjectRequestHandled?: () => void;
  onTasks?: (sessionId: string, taskId?: string) => void;
  onOpenSettings: () => void;
  onReviewHandoff?: (handoff: ReviewHandoff) => void;
}) {
  const {
    sidebarWidth,
    setSidebarWidth,
    workspaces,
    workspaceId,
    sessions,
    sessionId,
    snapshot,
    models,
    model,
    setModel,
    effort,
    setEffort,
    draft,
    stream,
    pendingUser,
    active,
    submitting,
    showArchived,
    setShowArchived,
    tasksEnabled,
    inspector,
    projectRequest,
    resetRequest,
    destination,
    shellRef,
    shellWidth,
    navigationOpen,
    setNavigationOpen,
    navigationButtonRef,
    toolPickerOpen,
    setToolPickerOpen,
    conversationMenuRef,
    assistantRequest,
    harnessEditor,
    setHarnessEditor,
    selectedMessage,
    contextBusy,
    setContextBusy,
    contextBusyRef,
    messageRef,
    transcriptRef,
    surface,
    projectDialog,
    setProjectDialog,
    moveTarget,
    setMoveTarget,
    moving,
    moveError,
    setMoveError,
    error,
    setError,
    submittingRef,
    editDraft,
    loadSessions,
    hydrate,
    saveCurrentDraft,
    selectProject,
    selectSession,
    createSession,
    projectCreated,
    openResearchConversation,
    updateProjectWorkspace,
    archiveSession,
    renameSession,
    generateTitle,
    deleteSession,
    moveSession,
    exportCurrent,
    saveSelection,
    send,
    resolve,
    renderedItems,
    outlineEntries,
    jumpToMessage,
    latest,
    visibleRequests,
    recordDestination,
    navigateProject,
    acceptSnapshot,
    openInspector,
    closeInspector,
    setInspector,
    setSubmitting,
    allRenderedItems,
    transcriptStart,
    transcriptEnd,
    setTranscriptPage,
    loadEarlier,
    loadingEarlier,
    followLatestRef,
    sessionRef,
    setSnapshot,
    navigationFits,
    navigationCollapsed,
    pinNavigation,
    navigationDialog,
    currentWorkspace,
    projectFolder,
    setProjectRequest,
  } = useWorkspacePageController({
    entryTarget,
    entryRequest,
    entrySurface,
    newProjectRequest,
    onNewProjectRequestHandled,
  });
  const [conversationFirst, setConversationFirst] = useState(0);
  const projectLabel = useId();
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

  const navigation = (
    <SidebarPanel
      fill={navigationCollapsed}
      aria-label="Project navigation"
      className={navigationCollapsed ? "workspace-navigation-drawer" : ""}
      width={navigationCollapsed ? (shellWidth ?? sidebarWidth) : sidebarWidth}
      defaultWidth={224}
      min={200}
      max={440}
      onResize={setSidebarWidth}
      resizeLabel="Resize project sidebar"
    >
      <SidebarHeader
        title="Projects"
        actions={
          <>
            <button
              type="button"
              title="Connection settings"
              aria-label="Assistant connection settings"
              onClick={onOpenSettings}
              className="workspace-sidebar-icon-button"
            >
              <WorkspaceIcon name="settings" size={18} />
            </button>
            <button
              type="button"
              className="workspace-sidebar-icon-button"
              aria-label="Close navigation"
              title="Close navigation"
              onClick={() => {
                if (navigationCollapsed) setNavigationOpen(false);
                else pinNavigation(false);
              }}
            >
              <WorkspaceIcon name="close" />
            </button>
          </>
        }
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="space-y-5 px-5 pb-5 pt-1">
          <div>
            <div className="mb-2 flex items-center justify-between gap-2">
              <span
                id={projectLabel}
                className="text-ui-meta font-medium text-ink-muted"
              >
                Project
              </span>
              <button
                type="button"
                title="New project"
                aria-label="New project"
                disabled={contextBusy || harnessEditor}
                onClick={() => {
                  setNavigationOpen(false);
                  setProjectDialog(true);
                }}
                className="workspace-sidebar-text-button"
              >
                <WorkspaceIcon name="plus" size={14} />
                New project
              </button>
            </div>
            <Select
              className="w-full"
              labelledBy={projectLabel}
              disabled={contextBusy || harnessEditor}
              value={workspaceId ?? ""}
              onChange={(value) => {
                void selectProject(value || null).catch((cause) =>
                  setError(workbenchErrorMessage(cause)),
                );
              }}
              options={[
                { value: "", label: "Unfiled conversations" },
                ...workspaces
                  .filter((item) => !item.archivedAt)
                  .map((item) => ({ value: item.id, label: item.name })),
              ]}
            />
            {currentWorkspace && (
              <p
                className="mt-2 truncate text-xs text-gray-500"
                title={currentWorkspace.root ?? undefined}
              >
                {currentWorkspace.missingRootAt
                  ? "Folder not found"
                  : (projectFolder ?? "No folder attached")}
              </p>
            )}
          </div>
        </div>
        {workspaceId && (
          <WorkspaceProjectNavigation
            key={workspaceId}
            destination={destination}
            disabled={contextBusy || harnessEditor}
            onNavigate={navigateProject}
          />
        )}
        <details
          className="workspace-conversations"
          open={!workspaceId || undefined}
        >
          <summary>Conversations · {sessions.length}</summary>
          <div className="px-3 py-2">
            <button
              type="button"
              disabled={contextBusy || harnessEditor}
              onClick={() =>
                void createSession().catch((cause) =>
                  setError(workbenchErrorMessage(cause)),
                )
              }
              className="workspace-sidebar-primary"
            >
              <WorkspaceIcon name="plus" />
              New conversation
            </button>
          </div>

          <nav aria-label="Conversations" className="px-3 pb-4">
            {!sessions.length && (
              <p className="px-2 py-3 text-xs text-gray-500 dark:text-gray-400">
                No conversations yet.
              </p>
            )}
            {sessions.map((session) => (
              <div
                key={session.id}
                className={`mb-1 flex items-center rounded-lg pr-1 transition-colors ${session.id === sessionId && surface === "chat" ? "bg-gray-100 text-gray-950 dark:bg-gray-800 dark:text-gray-50" : "text-gray-600 hover:bg-gray-50 dark:text-gray-400 dark:hover:bg-gray-800/60"}`}
              >
                <button
                  type="button"
                  disabled={contextBusy || harnessEditor}
                  aria-current={
                    session.id === sessionId && surface === "chat"
                      ? "page"
                      : undefined
                  }
                  title={session.title}
                  onClick={() => {
                    void selectSession(session.id).catch((cause) =>
                      setError(workbenchErrorMessage(cause)),
                    );
                  }}
                  className="min-w-0 flex-1 rounded-lg px-2 py-2.5 text-left text-sm disabled:opacity-40"
                >
                  <span className="block truncate">{session.title}</span>
                  {session.archivedAt && (
                    <span className="text-xs text-gray-500 dark:text-gray-400">
                      Archived
                    </span>
                  )}
                </button>
                <ConversationMenu
                  session={session}
                  disabled={contextBusy || harnessEditor}
                  onRename={() =>
                    void renameSession(session).catch((cause) =>
                      setError(workbenchErrorMessage(cause)),
                    )
                  }
                  onGenerateTitle={() =>
                    void generateTitle(session).catch((cause) =>
                      setError(
                        `Title could not be generated: ${workbenchErrorMessage(cause)}`,
                      ),
                    )
                  }
                  onMove={() => {
                    setNavigationOpen(false);
                    setMoveError(null);
                    setMoveTarget(session);
                  }}
                  onArchive={() =>
                    void archiveSession(session).catch((cause) =>
                      setError(workbenchErrorMessage(cause)),
                    )
                  }
                  onDelete={() =>
                    void deleteSession(session).catch((cause) =>
                      setError(workbenchErrorMessage(cause)),
                    )
                  }
                />
              </div>
            ))}
          </nav>
          <label className="flex shrink-0 cursor-pointer items-center gap-2 border-t border-gray-200/80 px-5 py-4 text-xs text-gray-500 dark:border-gray-800 dark:text-gray-400">
            <input
              type="checkbox"
              disabled={contextBusy || harnessEditor}
              checked={showArchived}
              onChange={(event) => {
                setShowArchived(event.target.checked);
                void loadSessions(workspaceId, event.target.checked);
              }}
              className="rounded border-gray-300"
            />
            Show archived
          </label>
        </details>
      </div>

      {navigationCollapsed && (
        <button
          type="button"
          className="workspace-pin-navigation"
          disabled={!navigationFits}
          onClick={() => pinNavigation(true)}
        >
          Keep navigation open
        </button>
      )}
    </SidebarPanel>
  );

  return (
    <div
      ref={shellRef}
      className="workspace-chat-shell flex h-full min-h-0 min-w-0 bg-white dark:bg-neutral-950"
    >
      {navigationCollapsed ? (
        <div
          ref={navigationDialog}
          hidden={!navigationOpen}
          role="dialog"
          aria-modal={navigationOpen ? true : undefined}
          aria-label="Browse projects"
          className="workspace-navigation-overlay"
        >
          <div
            className="workspace-navigation-backdrop"
            onPointerDown={() => setNavigationOpen(false)}
          />
          {navigation}
        </div>
      ) : (
        navigation
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
        navigationHidden={navigationCollapsed}
        navigationButtonRef={navigationButtonRef}
        onNavigation={() => {
          if (!navigationCollapsed) pinNavigation(false);
          else setNavigationOpen((value) => !value);
        }}
        navigationOpen={navigationOpen}
        projectName={currentWorkspace?.name}
        toolLabel={workspaceDestinations[destination].label}
        onTools={workspaceId ? () => setToolPickerOpen(true) : undefined}
        toolsDisabled={contextBusy || harnessEditor}
        attentionCount={visibleRequests.length}
        active={Boolean(active) || submitting}
        onStop={
          active
            ? () => {
                void workbenchClient.interruptTurn(
                  active.threadId,
                  active.turnId,
                );
              }
            : undefined
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
                      sessions={sessions.map((session) =>
                        snapshot?.session.id === session.id
                          ? snapshot.session
                          : session,
                      )}
                      onResumeSession={selectSession}
                      onStartConversation={createSession}
                      onAllProjects={onAllProjects}
                      destination={destination}
                      onDestination={recordDestination}
                      navigationInSidebar
                      snapshot={snapshot}
                      onSnapshot={acceptSnapshot}
                      onConversation={openResearchConversation}
                      onConversationFirst={() =>
                        setConversationFirst((request) => request + 1)
                      }
                      onProjectBrief={setProjectBrief}
                      onWorkspaceChanged={updateProjectWorkspace}
                      onReviewHandoff={onReviewHandoff}
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
                <p role="status">ChatGPT is working…</p>
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
                label="New chat"
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
              <MenuItem onSelect={() => void exportCurrent()}>
                Export conversation
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
                        ? "ChatGPT is working…"
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
                            {turn.state ??
                              (turn.terminalAt ? "Completed" : "Working")}
                            <span>{turn.createdAt ?? ""}</span>
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
                loadAccount={loadAccountSummary}
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
            stream={stream}
            selectedMessage={selectedMessage}
            transcriptStart={transcriptStart}
            transcriptEnd={transcriptEnd}
            transcriptRef={transcriptRef}
            messageRef={messageRef}
            error={error}
            draft={draft}
            disabled={
              !sessionId ||
              snapshot?.session.id !== sessionId ||
              submitting ||
              Boolean(active) ||
              contextBusy
            }
            active={Boolean(active)}
            submitting={submitting}
            contextBusy={contextBusy}
            onDraft={editDraft}
            onSend={() => void send()}
            onStop={() => {
              if (active)
                void workbenchClient.interruptTurn(
                  active.threadId,
                  active.turnId,
                );
            }}
            onLatest={latest}
            onEarlier={() => void loadEarlier()}
            onNewer={() => setTranscriptPage((v) => Math.max(0, v - 1))}
            onFollow={(value) => {
              followLatestRef.current = value;
            }}
            requests={visibleRequests.map((request) => (
              <RequestCard
                key={String(request.requestId)}
                event={request}
                onResolve={(event, result) => void resolve(event, result)}
              />
            ))}
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
                onSnapshot={(next) => {
                  if (sessionRef.current === next.session.id) setSnapshot(next);
                }}
                onBusy={setComposerBusy}
                onBrowseProjects={() => {
                  if (navigationCollapsed) setNavigationOpen(true);
                }}
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

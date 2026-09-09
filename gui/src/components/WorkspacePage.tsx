import { useWorkspacePageController } from "../hooks/useWorkspacePageController";
import WorkspaceMenu from "./WorkspaceMenu";
import WorkspaceToolPicker from "./WorkspaceToolPicker";
import WorkspaceComposerControls from "./WorkspaceComposerControls";
import RetainedWorkspaceView from "./RetainedWorkspaceView";
import WorkspaceProjectNavigation from "./WorkspaceProjectNavigation";
import { workspaceDestinations } from "../lib/workspaceNavigation";
import WorkspaceDesk from "./WorkspaceDesk";
import WorkspaceConversationView from "./WorkspaceConversationView";
import WorkspaceContextTray from "./WorkspaceContextTray";
import { lazy, Suspense, useState } from "react";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import "./WorkspaceConversation.css";
import WorkspaceIcon from "./WorkspaceIcon";
import SidebarPanel, { SidebarHeader } from "./SidebarPanel";
import WorkspaceComposerMenu from "./WorkspaceComposerMenu";
import WorkspaceConversationOutline from "./WorkspaceConversationOutline";
import {
  ConversationMenu,
  MoveConversationDialog,
} from "./WorkspaceConversationActions";
import type { WorkbenchEvent, ReviewHandoff } from "../lib/workbenchTypes";

const WorkspaceProjectSurface = lazy(() => import("./WorkspaceProjectSurface"));
const WorkspaceProjectDialog = lazy(() => import("./WorkspaceProjectDialog"));
const WorkspaceFollowups = lazy(() => import("./research-programs/Followups"));
const WorkspaceTaskCards = lazy(() => import("./WorkspaceTaskCards"));
const WorkspaceResearchPanel = lazy(() => import("./WorkspaceResearchPanel"));
const WorkspaceHarnessEditor = lazy(() => import("./WorkspaceHarnessEditor"));
/** Placeholder title until the first exchange is auto-titled or the user renames. */
const DEFAULT_TITLE = "New conversation";

function RequestCard({
  event,
  onResolve,
}: {
  event: WorkbenchEvent;
  onResolve: (event: WorkbenchEvent, result?: Record<string, unknown>) => void;
}) {
  const params = event.params ?? {};
  const questions = Array.isArray(params.questions)
    ? (params.questions as Array<Record<string, unknown>>)
    : [];
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const isQuestion = event.method === "item/tool/requestUserInput";
  const isPermission = event.method === "item/permissions/requestApproval";
  return (
    <div className="rounded-xl border border-amber-300 bg-amber-50 p-4 text-sm dark:border-amber-800 dark:bg-amber-950/30">
      <div className="font-semibold text-amber-950 dark:text-amber-100">
        {isQuestion
          ? "ChatGPT has a question"
          : isPermission
            ? "Permission requested"
            : "Approval requested"}
      </div>
      {!isQuestion && (
        <p className="mt-2 whitespace-pre-wrap text-amber-900 dark:text-amber-200">
          {String(
            params.reason ??
              params.command ??
              "Review this request before continuing.",
          )}
        </p>
      )}
      {questions.map((question) => {
        const id = String(question.id ?? "question");
        const options = Array.isArray(question.options)
          ? (question.options as Array<Record<string, unknown>>)
          : [];
        return (
          <label key={id} className="mt-3 block">
            <span className="block font-medium">
              {String(question.question ?? "Response")}
            </span>
            {options.length ? (
              <select
                value={answers[id] ?? ""}
                onChange={(e) =>
                  setAnswers((old) => ({ ...old, [id]: e.target.value }))
                }
                className="mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-900"
              >
                <option value="">Choose…</option>
                {options.map((option) => (
                  <option
                    key={String(option.label)}
                    value={String(option.label)}
                  >
                    {String(option.label)}
                  </option>
                ))}
              </select>
            ) : (
              <input
                type={question.isSecret ? "password" : "text"}
                value={answers[id] ?? ""}
                onChange={(e) =>
                  setAnswers((old) => ({ ...old, [id]: e.target.value }))
                }
                className="mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-900"
              />
            )}
          </label>
        );
      })}
      <div className="mt-3 flex gap-2">
        <button
          type="button"
          onClick={() => {
            if (isQuestion) {
              onResolve(event, {
                answers: Object.fromEntries(
                  Object.entries(answers).map(([id, answer]) => [
                    id,
                    { answers: [answer] },
                  ]),
                ),
              });
            } else if (isPermission) {
              onResolve(event, {
                permissions: params.permissions as Record<string, unknown>,
                scope: "turn",
              });
            } else {
              onResolve(event, { decision: "accept" });
            }
          }}
          className="rounded bg-amber-900 px-3 py-1.5 font-medium text-white dark:bg-amber-100 dark:text-amber-950"
        >
          {isQuestion ? "Submit" : "Allow once"}
        </button>
        <button
          type="button"
          onClick={() => {
            if (isQuestion) onResolve(event);
            else if (isPermission)
              onResolve(event, { permissions: {}, scope: "turn" });
            else onResolve(event, { decision: "decline" });
          }}
          className="rounded border border-amber-400 px-3 py-1.5"
        >
          Decline
        </button>
      </div>
    </div>
  );
}

export default function WorkspacePage({
  entryRequest = 0,
  entrySurface,
  newProjectRequest = 0,
  onNewProjectRequestHandled,
  onOpenSettings,
  onReviewHandoff,
  onTasks,
}: {
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
    setTasksEnabled,
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
    openProject,
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
    entryRequest,
    entrySurface,
    newProjectRequest,
    onNewProjectRequestHandled,
  });

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
              <label
                htmlFor="workspace-project"
                className="text-xs font-medium text-gray-600 dark:text-gray-400"
              >
                Project
              </label>
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
            <select
              id="workspace-project"
              disabled={contextBusy || harnessEditor}
              aria-label="Project"
              value={workspaceId ?? ""}
              onChange={(event) => {
                void selectProject(event.target.value || null).catch((cause) =>
                  setError(workbenchErrorMessage(cause)),
                );
              }}
              className="w-full min-w-0 rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-900"
            >
              <option value="">Unfiled conversations</option>
              {workspaces
                .filter((item) => !item.archivedAt)
                .map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
            </select>
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
                      destination={destination}
                      onDestination={recordDestination}
                      navigationInSidebar
                      snapshot={snapshot}
                      onSnapshot={acceptSnapshot}
                      onConversation={openResearchConversation}
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
            </div>
            {workspaceId && (
              <button
                type="button"
                className="workspace-sidebar-icon-button"
                disabled={contextBusy || harnessEditor}
                aria-label="New chat"
                title="New chat"
                onClick={() =>
                  void createSession().catch((cause) =>
                    setError(workbenchErrorMessage(cause)),
                  )
                }
              >
                <WorkspaceIcon name="plus" />
              </button>
            )}
            <WorkspaceMenu
              label="Conversation menu"
              triggerRef={conversationMenuRef}
              disabled={!snapshot || contextBusy || harnessEditor}
            >
              {sessions.length > 1 && (
                <label>
                  Conversation
                  <select
                    aria-label="Assistant conversation"
                    value={sessionId ?? ""}
                    onChange={(event) =>
                      void selectSession(event.target.value).catch((cause) =>
                        setError(workbenchErrorMessage(cause)),
                      )
                    }
                  >
                    {sessions.map((session) => (
                      <option key={session.id} value={session.id}>
                        {session.title}
                        {session.archivedAt ? " · Archived" : ""}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              <button
                type="button"
                onClick={() =>
                  openInspector(
                    "outline",
                    conversationMenuRef.current ?? undefined,
                  )
                }
              >
                Outline
              </button>
              <button
                type="button"
                onClick={() =>
                  openInspector(
                    "context",
                    conversationMenuRef.current ?? undefined,
                  )
                }
              >
                Context
              </button>
              <button
                type="button"
                onClick={() =>
                  openInspector(
                    "activity",
                    conversationMenuRef.current ?? undefined,
                  )
                }
              >
                Activity & follow-ups
              </button>
              <button type="button" onClick={() => void exportCurrent()}>
                Export conversation
              </button>
              {snapshot && (
                <button
                  type="button"
                  onClick={() =>
                    void archiveSession(snapshot.session).catch((cause) =>
                      setError(workbenchErrorMessage(cause)),
                    )
                  }
                >
                  {snapshot.session.archivedAt
                    ? "Restore conversation"
                    : "Archive conversation"}
                </button>
              )}
            </WorkspaceMenu>
          </header>
          <WorkspaceConversationView
            header={null}
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
                  <section
                    className="workspace-simple-inspector"
                    aria-label="Conversation context"
                  >
                    <div className="workspace-inspector-heading">
                      <h2>Conversation context</h2>
                      <button
                        type="button"
                        onClick={() => closeInspector()}
                        aria-label="Close inspector"
                      >
                        ×
                      </button>
                    </div>
                    <p>{snapshot?.workspace?.name ?? "Unfiled conversation"}</p>
                    <p className="text-xs text-gray-500">
                      Expand Sources above your message to see selected material
                      and its roles. Opening a file does not add it to chat.
                    </p>
                    {workspaceId ? (
                      <button
                        type="button"
                        onClick={() => navigateProject("documents")}
                      >
                        Browse project documents
                      </button>
                    ) : (
                      <button
                        type="button"
                        onClick={() => {
                          setNavigationOpen(false);
                          setProjectDialog(true);
                        }}
                      >
                        New project
                      </button>
                    )}
                    <button
                      type="button"
                      onClick={() => setInspector("settings")}
                    >
                      Instructions and tools
                    </button>
                  </section>
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
                onChange={(nextModel, nextEffort) => {
                  setModel(nextModel);
                  setEffort(nextEffort);
                  saveSelection(nextModel, nextEffort);
                }}
              />
            }
            composerSetup={
              snapshot && (
                <button
                  type="button"
                  className="workspace-assistant-setup"
                  aria-label="Assistant settings"
                  title={
                    typeof snapshot.session.overrides.mode === "string"
                      ? `Assistant settings · ${snapshot.session.overrides.mode === "inspect" ? "Read only" : "Allow edits"}`
                      : "Assistant settings"
                  }
                  disabled={contextBusy || harnessEditor}
                  onClick={(event) =>
                    openInspector("settings", event.currentTarget)
                  }
                >
                  <WorkspaceIcon name="settings" />
                </button>
              )
            }
            snapshot={snapshot}
            renderedItems={renderedItems}
            totalItems={allRenderedItems.length}
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
            onEarlier={() => setTranscriptPage((v) => v + 1)}
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
                workspaces={workspaces}
                disabled={submitting || Boolean(active) || contextBusy}
                onProject={selectProject}
                onCreateProject={() => setProjectDialog(true)}
                onOpenProject={workspaceId ? openProject : undefined}
                onSnapshot={(next) => {
                  if (sessionRef.current === next.session.id) setSnapshot(next);
                }}
                onBusy={(value) => {
                  contextBusyRef.current = value;
                  setContextBusy(value);
                  if (value) setInspector(null);
                }}
                onResearch={() => {
                  setInspector("settings");
                }}
                onTasks={
                  sessionId && onTasks
                    ? () => {
                        localStorage.setItem("pipeline.tasks.enabled", "true");
                        setTasksEnabled(true);
                        onTasks(sessionId);
                      }
                    : undefined
                }
                onDictation={() => messageRef.current?.focus()}
              />
            }
          />
        </section>
      </WorkspaceDesk>
    </div>
  );
}

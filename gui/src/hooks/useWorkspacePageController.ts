import {
  useWorkspaceConversationRuntime,
  useWorkspaceConversationEvents,
  createWorkspaceSubmission,
} from "./workspace/useWorkspaceConversationRuntime";
import {
  useWorkspaceSessionPersistence,
  useWorkspaceDraftAutosave,
} from "./workspace/useWorkspaceSessionPersistence";
import { useWorkspaceSessionMutations } from "./workspace/useWorkspaceSessionMutations";
import {
  useWorkspaceChrome,
  useWorkspaceChromeSession,
  useWorkspaceSurfacePreference,
  useWorkspaceChromeEffects,
} from "./workspace/useWorkspaceChrome";
import useWorkspaceModelCatalog from "./useWorkspaceModelCatalog";
import useWorkspaceSessionList from "./useWorkspaceSessionList";
import useWorkspaceRequestResolution from "./useWorkspaceRequestResolution";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useConversationActions } from "./useConversationActions";
import useWorkspaceEntry from "./useWorkspaceEntry";
import { newOperation } from "../lib/workspaceSessionState";
import useWorkspaceTranscript from "./useWorkspaceTranscript";
import { isWorkspaceDestination } from "../lib/workspaceNavigation";
import { payloadText, roleFor } from "../lib/workspaceTranscript";
import { deskClient, type ContextItem } from "../lib/deskClient";
import { appEvents } from "../lib/appEvents";
import { asContextItems } from "../lib/workspaceAttachments";
import {
  useWorkspaceRouteEntry,
  type WorkspaceEntryTarget,
} from "./useWorkspaceRouteEntry";
import { workspaceClient } from "../lib/workbench/workspaceClient";
import { conversationClient } from "../lib/workbench/conversationClient";
import { runtimeClient } from "../lib/workbench/runtimeClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { WorkbenchSession, Workspace } from "../lib/workbenchTypes";

export function useWorkspacePageController({
  entryTarget,
  entryRequest = 0,
  entrySurface,
  newProjectRequest = 0,
  onNewProjectRequestHandled,
}: {
  entryTarget?: WorkspaceEntryTarget;
  entryRequest?: number;
  entrySurface?: "chat" | "project";
  newProjectRequest?: number;
  onNewProjectRequestHandled?: () => void;
}) {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspaceId, setWorkspaceId] = useState<string | null>(
    () =>
      entryTarget?.projectId ??
      (entryTarget?.sessionId
        ? null
        : localStorage.getItem("pipeline.workspace.workspaceId")),
  );
  const [sessions, setSessions] = useState<WorkbenchSession[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(() =>
    entryTarget?.sessionId || entryTarget?.projectId
      ? null
      : localStorage.getItem("pipeline.workspace.sessionId"),
  );
  const [contextBusy, setContextBusy] = useState(false);
  const contextBusyRef = useRef(false);
  const chrome = useWorkspaceChrome(workspaceId, entrySurface, contextBusyRef);
  const {
    sidebarWidth,
    setSidebarWidth,
    inspector,
    setInspector,
    projectRequest,
    setProjectRequest,
    resetRequest,
    destination,
    setDestinationState,
    shellRef,
    shellWidth,
    navigationOpen,
    setNavigationOpen,
    navigationButtonRef,
    navigationPinned,
    toolPickerOpen,
    setToolPickerOpen,
    conversationMenuRef,
    assistantRequest,
    setAssistantRequest,
    harnessEditor,
    setHarnessEditor,
    inspectorTriggerRef,
    surface,
    setSurface,
    projectDialog,
    setProjectDialog,
    recordDestination,
    navigateProject,
    openInspector,
    closeInspector,
    navigationFits,
    navigationCollapsed,
    pinNavigation,
  } = chrome;
  const conversation = useWorkspaceConversationRuntime(sessionId, setSessions);
  const {
    snapshot,
    setSnapshot,
    models,
    setModels,
    model,
    setModel,
    runtime,
    setRuntime,
    effort,
    setEffort,
    draft,
    setDraft,
    stream,
    pendingUser,
    active,
    submitting,
    setSubmitting,
    requests,
    setRequests,
    error,
    setError,
    sessionRef,
    snapshotRef,
    activeRef,
    submittingRef,
    hydrationGeneration,
    hydrate,
    refreshSessionRecord,
    acceptSnapshot,
  } = conversation;
  const [showArchived, setShowArchived] = useState(false);
  // Automations are a first-class capability; no discovery gate hides them.
  const tasksEnabled = true;
  const [selectedMessage, setSelectedMessage] = useState<string | null>(null);
  const [jumpTarget, setJumpTarget] = useState<string | null>(null);
  const messageRef = useRef<HTMLTextAreaElement>(null);
  const transcriptRef = useRef<HTMLDivElement>(null);
  const followLatestRef = useRef(true);
  useWorkspaceEntry({
    workspaceId,
    entryRequest,
    entrySurface,
    newProjectRequest,
    onNewProjectRequestHandled,
    setSurface,
    setProjectRequest,
    setAssistantRequest,
    setProjectDialog,
  });
  const navigationGeneration = useRef(0);
  const workspaceRef = useRef(workspaceId);
  workspaceRef.current = workspaceId;
  const sessionListSequence = useRef(0);
  const persistence = useWorkspaceSessionPersistence(conversation);
  const { persistDraft, editDraft, saveCurrentDraft, saveSelection } =
    persistence;
  const loadSessions = useWorkspaceSessionList(
    workspaceRef,
    sessionListSequence,
    setSessions,
    setSessionId,
    showArchived,
  );

  useEffect(() => {
    let disposed = false;
    // App-owned conversations remain available even when an installed Codex
    // update changes or temporarily breaks the optional live runtime.
    void workspaceClient
      .listWorkspaces(true)
      .then((listed) => {
        if (!disposed) setWorkspaces(listed.workspaces);
      })
      .catch((cause) => {
        if (!disposed) setError(workbenchErrorMessage(cause));
      });
    void runtimeClient
      .pendingRequests()
      .then((pending) => {
        if (!disposed) setRequests(pending);
      })
      .catch((cause) => {
        if (!disposed) setError(workbenchErrorMessage(cause));
      });
    return () => {
      disposed = true;
    };
  }, [surface, sessionId]);

  useWorkspaceModelCatalog(
    surface,
    sessionId,
    snapshot?.session.id,
    runtime,
    setModels,
    setError,
  );

  useEffect(() => {
    void loadSessions(workspaceId);
  }, [loadSessions, workspaceId]);

  useEffect(() => {
    if (workspaceId)
      localStorage.setItem("pipeline.workspace.workspaceId", workspaceId);
    else localStorage.removeItem("pipeline.workspace.workspaceId");
  }, [workspaceId]);

  useEffect(() => {
    if (sessionId)
      localStorage.setItem("pipeline.workspace.sessionId", sessionId);
    else localStorage.removeItem("pipeline.workspace.sessionId");
  }, [sessionId]);

  useEffect(() => {
    if (
      sessionId &&
      snapshot?.session.id === sessionId &&
      snapshot.session.workspaceId === workspaceId
    ) {
      try {
        localStorage.setItem(
          `pipeline.workspace.lastSession.${workspaceId ?? "unfiled"}`,
          sessionId,
        );
      } catch {
        /* Optional preference. */
      }
    }
  }, [sessionId, workspaceId, snapshot?.session.id]);

  useWorkspaceChromeSession(chrome, sessionId);

  useEffect(() => {
    if (!sessionId) {
      setSnapshot(null);
      return;
    }
    setTranscriptPage(0);
    setSelectedMessage(null);
    setJumpTarget(null);
    followLatestRef.current = true;
    void runtimeClient
      .reconcileSession(sessionId)
      .catch((cause) =>
        setError(
          `Could not refresh the conversation. Showing the saved transcript: ${workbenchErrorMessage(cause)}`,
        ),
      )
      .finally(() => void hydrate(sessionId));
  }, [hydrate, sessionId]);

  useWorkspaceDraftAutosave(conversation, persistence, contextBusy);

  const { finishSubmission } = useWorkspaceConversationEvents(
    conversation,
    persistence,
  );

  const selectProject = useCallback(
    async (id: string | null) => {
      if (contextBusyRef.current || harnessEditor) return;
      const generation = ++navigationGeneration.current;
      await saveCurrentDraft();
      if (generation !== navigationGeneration.current) return;
      if (id !== workspaceId) {
        setSnapshot(null);
        setSessionId(null);
        setDraft("");
        setSessions([]);
        setWorkspaceId(id);
      }
      setSurface(id ? "project" : "chat");
      setNavigationOpen(false);
      setInspector(null);
    },
    [harnessEditor, saveCurrentDraft, workspaceId],
  );

  const selectSession = useCallback(
    async (id: string) => {
      if (contextBusyRef.current || harnessEditor) return;
      const generation = ++navigationGeneration.current;
      await saveCurrentDraft();
      if (generation !== navigationGeneration.current) return;
      if (id !== sessionId) {
        setSnapshot(null);
        setDraft("");
      }
      setSessionId(id);
      if (!workspaceId) setSurface("chat");
      setAssistantRequest((value) => value + 1);
    },
    [harnessEditor, saveCurrentDraft, sessionId, workspaceId],
  );

  const createSession = async () => {
    if (contextBusyRef.current || harnessEditor) return;
    const generation = ++navigationGeneration.current;
    await saveCurrentDraft();
    if (generation !== navigationGeneration.current) return;
    const created = await conversationClient.createSession({
      workspaceId,
      title: "New conversation",
      operationId: newOperation("create-session"),
    });
    if (
      generation !== navigationGeneration.current ||
      workspaceId !== workspaceRef.current
    )
      return;
    await loadSessions(workspaceId);
    if (
      generation !== navigationGeneration.current ||
      workspaceId !== workspaceRef.current
    )
      return;
    setSnapshot(null);
    setDraft("");
    setSessionId(created.record.id);
    if (!workspaceId) setSurface("chat");
    setAssistantRequest((value) => value + 1);
  };

  useWorkspaceSurfacePreference(surface);
  const openProject = () => {
    void saveCurrentDraft()
      .then(() => setSurface("project"))
      .catch((cause) => setError(workbenchErrorMessage(cause)));
  };

  const projectCreated = async (created: Workspace) => {
    ++navigationGeneration.current;
    await saveCurrentDraft().catch(() => undefined);
    setWorkspaces((old) => [
      created,
      ...old.filter((item) => item.id !== created.id),
    ]);
    setWorkspaceId(created.id);
    setSessionId(null);
    setSnapshot(null);
    setDraft("");
    setSurface("project");
    setProjectDialog(false);
    await loadSessions(created.id);
  };

  const openResearchConversation = useCallback(
    async (text: string, checkpoint?: string, contextItems?: ContextItem[]) => {
      if (!workspaceId) return;
      const generation = ++navigationGeneration.current;
      await saveCurrentDraft();
      if (generation !== navigationGeneration.current) return;
      let session: WorkbenchSession;
      if (checkpoint) {
        const { projectClient } = await import("../lib/projectClient");
        session = await projectClient.taskSession(workspaceId, checkpoint);
      } else {
        session = (
          await conversationClient.createSession({
            workspaceId,
            title: "Selected passage",
            operationId: newOperation("selection-chat"),
          })
        ).record;
        session = (
          await conversationClient.updateSession({
            sessionId: session.id,
            expectedRevision: session.revision,
            operationId: newOperation("selection-context"),
            draft: text,
            presetId: "research_assistant",
          })
        ).record;
      }
      if (
        generation !== navigationGeneration.current ||
        workspaceId !== workspaceRef.current
      )
        return;
      await loadSessions(workspaceId);
      if (contextItems?.length)
        await deskClient.saveContext(
          session.id,
          { revision: 0, items: [] },
          contextItems,
        );
      if (
        generation !== navigationGeneration.current ||
        workspaceId !== workspaceRef.current
      )
        return;
      setSessionId(session.id);
      setSurface("project");
      setAssistantRequest((value) => value + 1);
    },
    [loadSessions, workspaceId, sessionId, draft, active, persistDraft],
  );
  useEffect(() => {
    return appEvents.on("context-add", (detail) => {
      if (
        sessionId ||
        !workspaceId ||
        detail.workspaceId !== workspaceId ||
        !detail.objects.length ||
        contextBusyRef.current
      )
        return;
      contextBusyRef.current = true;
      setContextBusy(true);
      void openResearchConversation(
        "Work with this exact research source.",
        undefined,
        asContextItems(detail.objects),
      )
        .catch((cause) => setError(workbenchErrorMessage(cause)))
        .finally(() => {
          contextBusyRef.current = false;
          setContextBusy(false);
        });
    });
  }, [sessionId, workspaceId, openResearchConversation]);
  const updateProjectWorkspace = useCallback(
    (next: Workspace) =>
      setWorkspaces((old) => old.map((w) => (w.id === next.id ? next : w))),
    [],
  );

  const sessionMutations = useWorkspaceSessionMutations({
    workspaceId,
    showArchived,
    sessionRef,
    saveCurrentDraft,
    loadSessions,
    refreshSessionRecord,
    hydrate,
    setSnapshot,
    setSessionId,
    setDraft,
    setSessions,
    setWorkspaceId,
    setError,
  });
  const {
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
  } = sessionMutations;

  const conversationActions = useConversationActions({
    sessionId,
    sessionRef,
    snapshotRef,
    activeRef,
    hydrate,
  });

  const resolve = useWorkspaceRequestResolution(setRequests, setError);

  const {
    allRenderedItems,
    renderedItems,
    transcriptStart,
    transcriptEnd,
    transcriptPage,
    setTranscriptPage,
    loadEarlier,
    loadingEarlier,
  } = useWorkspaceTranscript(snapshot, setSnapshot, setError);
  const send = createWorkspaceSubmission(
    conversation,
    persistence,
    finishSubmission,
    {
      contextBusyRef,
      followLatestRef,
      setSelectedMessage,
      setJumpTarget,
      setTranscriptPage,
    },
  );
  const outlineEntries = useMemo(
    () =>
      allRenderedItems
        .map((item) => ({
          id: item.id,
          text: payloadText(item),
          role: roleFor(item),
        }))
        .filter((item) => item.text),
    [allRenderedItems],
  );
  const jumpToMessage = (id: string) => {
    const index = allRenderedItems.findIndex((item) => item.id === id);
    if (index < 0) return;
    followLatestRef.current = false;
    setTranscriptPage(Math.floor((allRenderedItems.length - 1 - index) / 200));
    setSelectedMessage(id);
    setJumpTarget(id);
  };
  useEffect(() => {
    if (!jumpTarget) return;
    const target = document.getElementById(`workspace-message-${jumpTarget}`);
    if (target) {
      target.scrollIntoView({ block: "start" });
      target.focus({ preventScroll: true });
      setJumpTarget(null);
    }
  }, [jumpTarget, renderedItems]);
  useEffect(() => {
    if (transcriptPage !== 0 || !followLatestRef.current || jumpTarget) return;
    const container = transcriptRef.current;
    if (container) container.scrollTop = container.scrollHeight;
  }, [stream, pendingUser, renderedItems, transcriptPage, jumpTarget]);
  const latest = () => {
    setTranscriptPage(0);
    setSelectedMessage(null);
    setJumpTarget(null);
    followLatestRef.current = true;
    const container = transcriptRef.current;
    if (container) container.scrollTop = container.scrollHeight;
  };
  const visibleRequests = requests.filter(
    (request) =>
      request.params?.threadId === snapshot?.activeBinding?.providerThreadId ||
      request.params?.threadId === active?.threadId,
  );
  useWorkspaceRouteEntry(
    entryRequest,
    entryTarget,
    async (target, current) => {
      const generation = ++navigationGeneration.current;
      await saveCurrentDraft();
      const selected = target.sessionId
        ? await conversationClient.sessionSnapshot(target.sessionId)
        : null;
      if (!current() || generation !== navigationGeneration.current) return;
      const project = selected
        ? selected.session.workspaceId
        : target.projectId;
      if (
        project !== undefined &&
        (project !== workspaceId ||
          (target.sessionId !== undefined &&
            target.sessionId !== sessionRef.current))
      ) {
        hydrationGeneration.current++;
        sessionListSequence.current++;
        setWorkspaceId(project);
        setSnapshot(null);
        snapshotRef.current = null;
        setDraft("");
        sessionRef.current = target.sessionId ?? null;
        setSessionId(target.sessionId ?? null);
        if (project !== workspaceId) setSessions([]);
        if (target.sessionId) void hydrate(target.sessionId);
      }
      if (target.destination && isWorkspaceDestination(target.destination))
        setDestinationState({
          workspaceId: project ?? workspaceId,
          tab: target.destination,
        });
      setSurface(target.surface ?? (target.sessionId ? "chat" : "project"));
      setNavigationOpen(false);
      setInspector(null);
    },
    (cause) => setError(workbenchErrorMessage(cause)),
  );
  const navigationDialog = useWorkspaceChromeEffects(
    chrome,
    workspaceId,
    sessionId,
    contextBusy,
  );

  const currentWorkspace =
    workspaces.find((item) => item.id === workspaceId) ?? null;
  const projectFolder = currentWorkspace?.root
    ? currentWorkspace.root
        .replace(/[\\/]+$/, "")
        .split(/[\\/]/)
        .pop() || currentWorkspace.root
    : null;

  return {
    layout: {
      sidebarWidth,
      setSidebarWidth,
      shellRef,
      shellWidth,
      navigationOpen,
      setNavigationOpen,
      navigationButtonRef,
      navigationPinned,
      toolPickerOpen,
      setToolPickerOpen,
      conversationMenuRef,
      navigationFits,
      navigationCollapsed,
      pinNavigation,
      navigationDialog,
    },
    inspector: {
      inspector,
      openInspector,
      closeInspector,
      setInspector,
      inspectorTriggerRef,
      harnessEditor,
      setHarnessEditor,
    },
    navigation: {
      workspaces,
      workspaceId,
      sessions,
      sessionId,
      showArchived,
      setShowArchived,
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
      openProject,
      projectCreated,
      openResearchConversation,
      updateProjectWorkspace,
      recordDestination,
      navigateProject,
      currentWorkspace,
      projectFolder,
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
      stream,
      pendingUser,
      active,
      submitting,
      requests,
      submittingRef,
      hydrate,
      send,
      resolve,
      visibleRequests,
      acceptSnapshot,
      setSubmitting,
      sessionRef,
      setSnapshot,
      ...conversationActions,
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
      jumpTarget,
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
  };
}

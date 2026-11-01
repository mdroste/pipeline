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
import useWorkspaceSessionList, {
  useOlderSessions,
} from "./useWorkspaceSessionList";
import useWorkspaceRequestResolution from "./useWorkspaceRequestResolution";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useConversationActions } from "./useConversationActions";
import useWorkspaceEntry from "./useWorkspaceEntry";
import { newOperation } from "../lib/workspaceSessionState";
import useWorkspaceTranscript from "./useWorkspaceTranscript";
import { isWorkspaceDestination } from "../lib/workspaceNavigation";
import { router } from "../lib/router";
import {
  payloadText,
  roleFor,
  TRANSCRIPT_WINDOW,
} from "../lib/workspaceTranscript";
import { deskClient, type ContextItem } from "../lib/deskClient";
import { appEvents } from "../lib/appEvents";
import { clearDeskObjects } from "../lib/deskLayout";
import { asContextItems } from "../lib/workspaceAttachments";
import {
  useWorkspaceRouteEntry,
  type WorkspaceEntryTarget,
} from "./useWorkspaceRouteEntry";
import { workspaceClient } from "../lib/workbench/workspaceClient";
import { conversationClient } from "../lib/workbench/conversationClient";
import { runtimeClient } from "../lib/workbench/runtimeClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  WorkbenchEvent,
  WorkbenchSession,
  Workspace,
} from "../lib/workbenchTypes";

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
    inspector,
    setInspector,
    projectRequest,
    setProjectRequest,
    resetRequest,
    destination,
    setDestinationState,
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
    streamStore,
    clearStream,
    pendingUser,
    setPendingUser,
    active,
    setActive,
    submitting,
    setSubmitting,
    requests,
    setRequests,
    error,
    setError,
    notice,
    setNotice,
    submissionOwner,
    activeTurn,
    fileChangesRef,
    sessionRef,
    snapshotRef,
    activeRef,
    submittingRef,
    hydrationGeneration,
    hydrate,
    refreshSessionRecord,
    acceptSnapshot,
  } = conversation;
  // Automations are a first-class capability; no discovery gate hides them.
  const tasksEnabled = true;
  const [selectedMessage, setSelectedMessage] = useState<string | null>(null);
  const [jumpTarget, setJumpTarget] = useState<string | null>(null);
  const messageRef = useRef<HTMLTextAreaElement>(null);
  const transcriptRef = useRef<HTMLDivElement>(null);
  const followLatestRef = useRef(true);
  // The conversation being opened, which takes keyboard focus once it loads.
  const composerFocusFor = useRef<string | null>(null);
  // The route entry the page has finished applying. Its pane is revealed in the
  // next commit, so the desk of a project being left keeps its layout and the
  // desk of the next project is already there to see the request.
  const [entryApplied, setEntryApplied] = useState(0);
  useWorkspaceEntry({
    workspaceId,
    entryRequest,
    entryApplied: entryTarget ? entryApplied : entryRequest,
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
  // A route entry being applied still names where the researcher is going; the
  // state on the way there must not replace it as the place to reopen.
  const enteringRoute = useRef(Boolean(entryTarget));
  // A conversation a route entry opened in another project. It may be archived
  // or on a later page, so that project's list must not choose a different one.
  const routedSession = useRef<string | null>(null);
  const persistence = useWorkspaceSessionPersistence(conversation);
  const { editDraft, saveCurrentDraft, saveSelection } = persistence;
  const olderSessions = useOlderSessions(
    workspaceRef,
    sessionListSequence,
    setSessions,
    false,
    setError,
  );
  const { position: sessionPosition, setHasOlder } = olderSessions;
  const sessionPaging = useMemo(
    () => ({ position: sessionPosition, setHasOlder }),
    [sessionPosition, setHasOlder],
  );
  const loadSessions = useWorkspaceSessionList(
    workspaceRef,
    sessionListSequence,
    setSessions,
    setSessionId,
    false,
    setError,
    sessionPaging,
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
    const routed = routedSession.current;
    routedSession.current = null;
    void loadSessions(
      workspaceId,
      false,
      routed !== null && routed === sessionRef.current,
    );
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

  useEffect(() => {
    if (enteringRoute.current) return;
    router.remember({
      page: "workspace",
      surface,
      ...(workspaceId ? { projectId: workspaceId, destination } : {}),
      ...(sessionId ? { sessionId } : {}),
    });
  }, [workspaceId, sessionId, surface, destination]);

  useWorkspaceChromeSession(chrome, sessionId);

  useEffect(() => {
    // Errors speak for the conversation that was on screen.
    setError(null);
    if (!sessionId) {
      // Hydration resets these for a conversation; with none selected, a
      // response running elsewhere must not stay on this page. Its submission
      // stays, so the background notice can still point to it.
      activeRef.current = null;
      setActive(null);
      setPendingUser(null);
      setSubmitting(false);
      clearStream();
      setSnapshot(null);
      return;
    }
    setTranscriptPage(0);
    setSelectedMessage(null);
    setJumpTarget(null);
    setNotice(null);
    followLatestRef.current = true;
    // A slow reconcile for a conversation the user has already left must
    // neither report its error here nor start a load.
    let cancelled = false;
    const current = () => !cancelled && sessionRef.current === sessionId;
    void runtimeClient
      .reconcileSession(sessionId)
      .catch((cause) => {
        if (current())
          setError(
            `Could not refresh the conversation. Showing the saved transcript: ${workbenchErrorMessage(cause)}`,
          );
      })
      .finally(() => {
        if (current()) void hydrate(sessionId);
      });
    return () => {
      cancelled = true;
    };
  }, [hydrate, sessionId]);

  useEffect(() => {
    if (!sessionId || snapshot?.session.id !== sessionId) return;
    if (composerFocusFor.current !== sessionId) return;
    composerFocusFor.current = null;
    messageRef.current?.focus({ preventScroll: true });
  }, [sessionId, snapshot?.session.id]);

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
        composerFocusFor.current = id;
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
    appEvents.emit("conversations-changed", {});
    composerFocusFor.current = created.record.id;
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
      appEvents.emit("conversations-changed", {});
      setSessionId(session.id);
      setSurface("project");
      setAssistantRequest((value) => value + 1);
    },
    [loadSessions, saveCurrentDraft, workspaceId],
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
    showArchived: false,
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
    setTranscriptPage(
      Math.floor((allRenderedItems.length - 1 - index) / TRANSCRIPT_WINDOW),
    );
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
  // WorkspaceConversationView keeps the latest message pinned while following.
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
      request.params?.threadId === active?.threadId ||
      (!!sessionId && request.sessionId === sessionId),
  );
  /** The pending file change an approval refers to: live events, then the saved transcript. */
  const requestItem = (request: WorkbenchEvent) => {
    const itemId = request.params?.itemId;
    if (typeof itemId !== "string") return null;
    return (
      fileChangesRef.current.get(itemId) ??
      snapshot?.items.find((item) => item.providerItemId === itemId)?.payload ??
      null
    );
  };
  // Work this conversation can't show: a response in one the researcher has
  // left, an approval waiting there, or an Automation here that hasn't started.
  const elsewhere =
    [
      activeTurn?.sessionId !== sessionId || (activeTurn?.automated && !active)
        ? activeTurn?.sessionId
        : null,
      submissionOwner?.sessionId !== sessionId
        ? submissionOwner?.sessionId
        : null,
      requests.find(
        (request) => request.sessionId && request.sessionId !== sessionId,
      )?.sessionId,
    ].find(Boolean) ?? null;
  const elsewhereTurn = activeTurn?.sessionId === elsewhere ? activeTurn : null;
  const elsewhereThread =
    elsewhereTurn?.threadId ??
    (submissionOwner?.sessionId === elsewhere
      ? submissionOwner?.threadId
      : null);
  const backgroundTurn = elsewhere
    ? {
        sessionId: elsewhere,
        title:
          sessions.find((session) => session.id === elsewhere)?.title ??
          elsewhereTurn?.title ??
          null,
        waiting:
          elsewhere === sessionId
            ? 0
            : requests.filter(
                (request) =>
                  request.sessionId === elsewhere ||
                  (!!elsewhereThread &&
                    request.params?.threadId === elsewhereThread),
              ).length,
        automated: Boolean(elsewhereTurn?.automated),
      }
    : null;
  useWorkspaceRouteEntry(
    entryRequest,
    entryTarget,
    async (target, current) => {
      enteringRoute.current = true;
      try {
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
          routedSession.current =
            project !== workspaceRef.current
              ? (target.sessionId ?? null)
              : null;
          composerFocusFor.current = target.sessionId ?? null;
          setWorkspaceId(project);
          setSnapshot(null);
          snapshotRef.current = null;
          setDraft("");
          sessionRef.current = target.sessionId ?? null;
          setSessionId(target.sessionId ?? null);
          if (project !== workspaceId) setSessions([]);
          // The sequence above retired any list load in flight, and an
          // unchanged project starts no other. The target may be archived or
          // on a later page, so the list must not choose a different one.
          else void loadSessions(project, false, true);
          if (target.sessionId) void hydrate(target.sessionId);
        }
        const entered = project ?? workspaceId;
        const landing =
          target.surface ?? (target.sessionId ? "chat" : "project");
        if (target.destination && isWorkspaceDestination(target.destination)) {
          setDestinationState({
            workspaceId: entered,
            tab: target.destination,
          });
          // A link names where to land, so a research item left open must not
          // cover it. A relaunch (request 0) restores the view as it was left.
          if (entered && entryRequest > 0 && landing === "project") {
            clearDeskObjects(entered);
            appEvents.emit("workspace-destination", {
              workspaceId: entered,
              tab: target.destination,
            });
          }
        }
        setSurface(landing);
        setInspector(null);
        setEntryApplied(entryRequest);
      } finally {
        if (current()) enteringRoute.current = false;
      }
    },
    (cause) => setError(workbenchErrorMessage(cause)),
  );
  useWorkspaceChromeEffects(chrome, workspaceId, sessionId);

  const currentWorkspace =
    workspaces.find((item) => item.id === workspaceId) ?? null;

  return {
    layout: {
      toolPickerOpen,
      setToolPickerOpen,
      conversationMenuRef,
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
      hasOlderSessions: olderSessions.hasOlder,
      loadOlderSessions: olderSessions.loadOlder,
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
      openProject,
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
      requests,
      submittingRef,
      hydrate,
      send,
      resolve,
      visibleRequests,
      requestItem,
      backgroundTurn,
      notice,
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

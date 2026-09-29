import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import useModalDialog from "./useModalDialog";
import {
  useWorkbenchEvents,
  terminalKey,
  type PendingSubmission,
} from "./useWorkbenchEvents";
import useContainerWidth from "./useContainerWidth";
import useWorkspaceEntry from "./useWorkspaceEntry";
import {
  mergeSessionSnapshot,
  saveWorkspaceDraft,
} from "../lib/workspaceSessionState";
import useWorkspaceTranscript from "./useWorkspaceTranscript";
import usePersistentPanelWidth from "./usePersistentPanelWidth";
import { loadDeskLayout } from "../lib/deskLayout";
import {
  isWorkspaceDestination,
  type WorkspaceDestination,
} from "../lib/workspaceNavigation";
import { payloadText, roleFor } from "../components/WorkspaceConversationView";
import { deskClient, type ContextItem } from "../lib/deskClient";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  ConversationSnapshot,
  WorkbenchEvent,
  WorkbenchSession,
  Workspace,
  WorkspaceModel,
} from "../lib/workbenchTypes";

function newOperation(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

export function useWorkspacePageController({
  entryRequest = 0,
  entrySurface,
  newProjectRequest = 0,
  onNewProjectRequestHandled,
}: {
  entryRequest?: number;
  entrySurface?: "chat" | "project";
  newProjectRequest?: number;
  onNewProjectRequestHandled?: () => void;
}) {
  const [sidebarWidth, setSidebarWidth] = usePersistentPanelWidth(
    "pipeline.workspace.sidebarWidth",
    224,
    200,
    440,
  );
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspaceId, setWorkspaceId] = useState<string | null>(() =>
    localStorage.getItem("pipeline.workspace.workspaceId"),
  );
  const [sessions, setSessions] = useState<WorkbenchSession[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(() =>
    localStorage.getItem("pipeline.workspace.sessionId"),
  );
  const [snapshot, setSnapshot] = useState<ConversationSnapshot | null>(null);
  const [models, setModels] = useState<WorkspaceModel[]>([]);
  const [model, setModel] = useState("");
  const [effort, setEffort] = useState("");
  const [draft, setDraft] = useState("");
  const [stream, setStream] = useState("");
  const [pendingUser, setPendingUser] = useState<string | null>(null);
  const [active, setActive] = useState<{
    threadId: string;
    turnId: string;
  } | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [requests, setRequests] = useState<WorkbenchEvent[]>([]);
  const [showArchived, setShowArchived] = useState(false);
  const [tasksEnabled, setTasksEnabled] = useState(
    () => localStorage.getItem("pipeline.tasks.enabled") === "true",
  );
  const [inspector, setInspector] = useState<
    "settings" | "outline" | "context" | "activity" | null
  >(null);
  const [projectRequest, setProjectRequest] = useState(0);
  const resetRequest = 0;
  const [destinationState, setDestinationState] = useState<{
    workspaceId: string | null;
    tab: WorkspaceDestination;
  } | null>(null);
  const savedDestination = useMemo(
    () => (workspaceId ? loadDeskLayout(workspaceId).tab : "overview"),
    [workspaceId],
  );
  const destination =
    destinationState?.workspaceId === workspaceId
      ? destinationState.tab
      : isWorkspaceDestination(savedDestination)
        ? savedDestination
        : "overview";
  const [shellRef, shellWidth] = useContainerWidth<HTMLDivElement>();
  const [navigationOpen, setNavigationOpen] = useState(false);
  const navigationButtonRef = useRef<HTMLButtonElement>(null);
  const [navigationPinned, setNavigationPinned] = useState(() => {
    try {
      return (
        localStorage.getItem("pipeline.workspace.navigationPinned") === "true"
      );
    } catch {
      return false;
    }
  });
  const [toolPickerOpen, setToolPickerOpen] = useState(false);
  const conversationMenuRef = useRef<HTMLButtonElement>(null);
  const [assistantRequest, setAssistantRequest] = useState(0);

  const [harnessEditor, setHarnessEditor] = useState(false);
  const [selectedMessage, setSelectedMessage] = useState<string | null>(null);
  const [jumpTarget, setJumpTarget] = useState<string | null>(null);
  const [contextBusy, setContextBusy] = useState(false);
  const contextBusyRef = useRef(false);
  const inspectorTriggerRef = useRef<HTMLButtonElement | null>(null);
  const messageRef = useRef<HTMLTextAreaElement>(null);
  const transcriptRef = useRef<HTMLDivElement>(null);
  const followLatestRef = useRef(true);
  const [surface, setSurface] = useState<"chat" | "project">(
    () =>
      entrySurface ??
      (Boolean(localStorage.getItem("pipeline.workspace.workspaceId"))
        ? "project"
        : "chat"),
  );
  const [projectDialog, setProjectDialog] = useState(false);
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
  const [moveTarget, setMoveTarget] = useState<WorkbenchSession | null>(null);
  const [moving, setMoving] = useState(false);
  const [moveError, setMoveError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const sessionRef = useRef(sessionId);
  const snapshotRef = useRef(snapshot);
  const submissionRef = useRef<PendingSubmission | null>(null);
  const activeRef = useRef(active);
  const submittingRef = useRef(false);
  const completedTurnsRef = useRef(new Map<string, string>());
  const selectionSaveRef = useRef<Promise<void>>(Promise.resolve());
  const sessionWriteRef = useRef<Promise<unknown>>(Promise.resolve());
  const sessionListSequence = useRef(0);
  const draftSaveRef = useRef<Promise<unknown>>(Promise.resolve());
  const streamBufferRef = useRef("");
  const streamFlushRef = useRef<number | null>(null);
  sessionRef.current = sessionId;
  snapshotRef.current = snapshot;
  activeRef.current = active;

  const draftEditRevision = useRef(0);
  const hydrationGeneration = useRef(0);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const persistDraft = useCallback((selectedSession: string, text: string) => {
    // Navigation can flush a draft while its debounce save is in flight.
    const pending = sessionWriteRef.current
      .catch(() => undefined)
      .then(() => saveWorkspaceDraft(selectedSession, text))
      .then((next) => {
        if (sessionRef.current === selectedSession)
          setSnapshot((old) => mergeSessionSnapshot(old, next));
        return next;
      });
    draftSaveRef.current = pending;
    sessionWriteRef.current = pending;
    return pending;
  }, []);

  const editDraft = (text: string) => {
    draftEditRevision.current += 1;
    if (sessionId)
      try {
        localStorage.setItem(`pipeline.pendingDraft.${sessionId}`, text);
      } catch {
        /* Server debounce remains available. */
      }
    setDraft(text);
  };
  useEffect(
    () => () => {
      const id = sessionRef.current;
      if (
        id &&
        snapshotRef.current?.session.id === id &&
        !activeRef.current &&
        !submittingRef.current
      )
        void persistDraft(id, draftRef.current).catch(() => undefined);
    },
    [persistDraft],
  );
  const loadSessions = useCallback(
    async (selectedWorkspace: string | null, archived = showArchived) => {
      const sequence = ++sessionListSequence.current;
      const response = await workbenchClient.listSessions(
        selectedWorkspace,
        archived,
      );
      if (sequence !== sessionListSequence.current) return;
      let remembered: string | null = null;
      try {
        remembered = localStorage.getItem(
          `pipeline.workspace.lastSession.${selectedWorkspace ?? "unfiled"}`,
        );
      } catch {
        /* Optional navigation state. */
      }
      setSessions(response.sessions);
      setSessionId((current) =>
        response.sessions.some((session) => session.id === current)
          ? current
          : (response.sessions.find((session) => session.id === remembered)
              ?.id ??
            response.sessions[0]?.id ??
            null),
      );
    },
    [showArchived],
  );

  const hydrate = useCallback(async (selectedSession = sessionRef.current) => {
    if (!selectedSession) {
      if (!sessionRef.current) setSnapshot(null);
      return;
    }
    const generation = ++hydrationGeneration.current;
    const activeAtStart = activeRef.current;
    const ownerAtStart = submissionRef.current;
    const editRevision = draftEditRevision.current;
    const initialize = snapshotRef.current?.session.id !== selectedSession;
    let next: ConversationSnapshot;
    try {
      next = await workbenchClient.conversationSnapshot(selectedSession);
    } catch (cause) {
      if (
        sessionRef.current === selectedSession &&
        generation === hydrationGeneration.current
      )
        setError(workbenchErrorMessage(cause));
      return;
    }
    if (
      sessionRef.current !== selectedSession ||
      generation !== hydrationGeneration.current ||
      activeRef.current !== activeAtStart ||
      submissionRef.current !== ownerAtStart
    )
      return;
    const current = snapshotRef.current;
    if (
      current?.session.id === selectedSession &&
      (current.sequence > next.sequence ||
        current.session.revision > next.session.revision)
    )
      return;
    setSnapshot(next);
    let recoveredDraft: string | null = null;
    try {
      recoveredDraft = localStorage.getItem(
        `pipeline.pendingDraft.${selectedSession}`,
      );
    } catch {
      /* Optional recovery cache. */
    }
    const owner = submissionRef.current;
    const pendingHere = owner?.sessionId === selectedSession;
    setPendingUser(pendingHere ? owner.text : null);
    setSubmitting(pendingHere && !owner.turnId);
    const inProgress = [...next.turns]
      .reverse()
      .find((turn) => !turn.terminalAt && turn.providerTurnId);
    if (!activeRef.current || (!pendingHere && !inProgress)) {
      setStream("");
      if (streamFlushRef.current !== null)
        window.clearTimeout(streamFlushRef.current);
      streamFlushRef.current = null;
      streamBufferRef.current = "";
    }
    if (initialize && draftEditRevision.current === editRevision)
      setDraft(pendingHere ? "" : (recoveredDraft ?? next.session.draft));
    const overrides = next.session.overrides;
    setModel(typeof overrides.model === "string" ? overrides.model : "");
    setEffort(typeof overrides.effort === "string" ? overrides.effort : "");
    if (pendingHere && owner.threadId && owner.turnId) {
      setActive({ threadId: owner.threadId, turnId: owner.turnId });
    } else if (inProgress && next.activeBinding) {
      setActive({
        threadId: next.activeBinding.providerThreadId,
        turnId: inProgress.providerTurnId!,
      });
    } else {
      setActive(null);
    }
  }, []);

  // Refresh records without replacing the composer draft.
  const refreshSessionRecord = useCallback(async (id: string) => {
    const latest = await workbenchClient.sessionSnapshot(id);
    setSessions((old) =>
      old.map((session) => (session.id === id ? latest.session : session)),
    );
    if (sessionRef.current === id)
      setSnapshot((old) => mergeSessionSnapshot(old, latest));
  }, []);

  useEffect(() => {
    let disposed = false;
    // App-owned conversations remain available even when an installed Codex
    // update changes or temporarily breaks the optional live runtime.
    void workbenchClient
      .listWorkspaces(true)
      .then((listed) => {
        if (!disposed) setWorkspaces(listed.workspaces);
      })
      .catch((cause) => {
        if (!disposed) setError(workbenchErrorMessage(cause));
      });
    void workbenchClient
      .pendingRequests()
      .then((pending) => {
        if (!disposed) setRequests(pending);
      })
      .catch((cause) => {
        if (!disposed) setError(workbenchErrorMessage(cause));
      });
    if (surface === "chat" || sessionId)
      void workbenchClient
        .connectCodex()
        .then(() => workbenchClient.accountState())
        .then(async (account) => {
          if (account.status !== "chatgpt") return;
          const catalog = await workbenchClient.modelCatalog();
          if (!disposed) setModels(catalog.models);
        })
        .catch((cause) => {
          if (!disposed)
            setError(
              `The assistant could not connect. Saved conversations remain available: ${workbenchErrorMessage(cause)}`,
            );
        });
    return () => {
      disposed = true;
    };
  }, [surface, sessionId]);

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

  // The harness editor is conversation-scoped; leaving the conversation closes it.
  useEffect(() => {
    setHarnessEditor(false);
    setInspector(null);
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId) {
      setSnapshot(null);
      return;
    }
    setTranscriptPage(0);
    setSelectedMessage(null);
    setJumpTarget(null);
    followLatestRef.current = true;
    void workbenchClient
      .reconcileSession(sessionId)
      .catch((cause) =>
        setError(
          `Could not refresh the conversation. Showing the saved transcript: ${workbenchErrorMessage(cause)}`,
        ),
      )
      .finally(() => void hydrate(sessionId));
  }, [hydrate, sessionId]);

  useEffect(() => {
    const current = snapshot;
    if (
      !current ||
      current.session.id !== sessionId ||
      pendingUser ||
      active ||
      submitting ||
      contextBusy ||
      current.session.draft === draft
    )
      return;
    const selectedSession = current.session.id;
    const text = draft;
    const timer = window.setTimeout(() => {
      void persistDraft(selectedSession, text)
        .then((next) => {
          if (sessionRef.current === selectedSession)
            setSnapshot((old) => mergeSessionSnapshot(old, next));
        })
        .catch((cause) =>
          setError(`Draft could not be saved: ${workbenchErrorMessage(cause)}`),
        );
    }, 350);
    return () => window.clearTimeout(timer);
  }, [
    active,
    contextBusy,
    draft,
    pendingUser,
    persistDraft,
    sessionId,
    snapshot,
    submitting,
  ]);

  const { finishSubmission } = useWorkbenchEvents({
    activeRef,
    completedTurnsRef,
    hydrate,
    submissionRef,
    persistDraft,
    refreshSessionRecord,
    sessionRef,
    setActive,
    setDraft,
    setError,
    setModels,
    setPendingUser,
    setRequests,
    setSnapshot,
    setStream,
    setSubmitting,
    setTasksEnabled,
    snapshotRef,
    streamBufferRef,
    streamFlushRef,
    submittingRef,
  });

  const saveCurrentDraft = useCallback(async () => {
    await selectionSaveRef.current;
    if (sessionId && !active && !submittingRef.current)
      await persistDraft(sessionId, draft);
  }, [active, draft, persistDraft, sessionId]);

  const selectProject = useCallback(
    async (id: string | null) => {
      if (contextBusyRef.current || harnessEditor) return;
      await saveCurrentDraft();
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
      await saveCurrentDraft();
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
    await saveCurrentDraft();
    const created = await workbenchClient.createSession({
      workspaceId,
      title: "New conversation",
      operationId: newOperation("create-session"),
    });
    await loadSessions(workspaceId);
    setSnapshot(null);
    setDraft("");
    setSessionId(created.record.id);
    if (!workspaceId) setSurface("chat");
    setAssistantRequest((value) => value + 1);
  };

  useEffect(() => {
    localStorage.setItem("pipeline.workspace.surface", surface);
  }, [surface]);
  const openProject = () => {
    void saveCurrentDraft()
      .then(() => setSurface("project"))
      .catch((cause) => setError(workbenchErrorMessage(cause)));
  };

  const projectCreated = async (created: Workspace) => {
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
      await saveCurrentDraft();
      let session: WorkbenchSession;
      if (checkpoint) {
        const { projectClient } = await import("../lib/projectClient");
        session = await projectClient.taskSession(workspaceId, checkpoint);
      } else {
        session = (
          await workbenchClient.createSession({
            workspaceId,
            title: "Selected passage",
            operationId: newOperation("selection-chat"),
          })
        ).record;
        session = (
          await workbenchClient.updateSession({
            sessionId: session.id,
            expectedRevision: session.revision,
            operationId: newOperation("selection-context"),
            draft: text,
            presetId: "research_assistant",
          })
        ).record;
      }
      await loadSessions(workspaceId);
      if (contextItems?.length)
        await deskClient.saveContext(
          session.id,
          { revision: 0, items: [] },
          contextItems,
        );
      setSessionId(session.id);
      setSurface("project");
      setAssistantRequest((value) => value + 1);
    },
    [loadSessions, workspaceId, sessionId, draft, active, persistDraft],
  );
  useEffect(() => {
    const add = (event: Event) => {
      const detail = (
        event as CustomEvent<{
          workspaceId: string;
          object: ContextItem["object"];
        }>
      ).detail;
      if (
        sessionId ||
        !workspaceId ||
        detail.workspaceId !== workspaceId ||
        contextBusyRef.current
      )
        return;
      contextBusyRef.current = true;
      setContextBusy(true);
      void openResearchConversation(
        "Work with this exact research source.",
        undefined,
        [
          {
            role: detail.object.kind === "dataset" ? "data_dictionary" : "main",
            object: detail.object,
          },
        ],
      )
        .catch((cause) => setError(workbenchErrorMessage(cause)))
        .finally(() => {
          contextBusyRef.current = false;
          setContextBusy(false);
        });
    };
    window.addEventListener("pipeline-context-add", add);
    return () => window.removeEventListener("pipeline-context-add", add);
  }, [sessionId, workspaceId, openResearchConversation]);
  const updateProjectWorkspace = useCallback(
    (next: Workspace) =>
      setWorkspaces((old) => old.map((w) => (w.id === next.id ? next : w))),
    [],
  );

  const archiveSession = async (session: WorkbenchSession) => {
    const latest = await workbenchClient.sessionSnapshot(session.id);
    await workbenchClient.updateSession({
      sessionId: session.id,
      expectedRevision: latest.session.revision,
      operationId: newOperation("archive-session"),
      archived: !latest.session.archivedAt,
    });
    await loadSessions(workspaceId, showArchived);
    if (sessionRef.current === session.id)
      await refreshSessionRecord(session.id);
  };

  const renameSession = async (session: WorkbenchSession) => {
    const answer = window.prompt("Conversation title", session.title);
    if (answer === null) return;
    const title = answer.trim();
    if (!title || title === session.title) return;
    const latest = await workbenchClient.sessionSnapshot(session.id);
    await workbenchClient.updateSession({
      sessionId: session.id,
      expectedRevision: latest.session.revision,
      operationId: newOperation("rename-session"),
      title,
    });
    await refreshSessionRecord(session.id);
  };

  const generateTitle = async (session: WorkbenchSession) => {
    setError(null);
    await workbenchClient.generateSessionTitle(session.id);
    await refreshSessionRecord(session.id);
  };

  const deleteSession = async (session: WorkbenchSession) => {
    if (
      !window.confirm(
        `Delete “${session.title}”? Its transcript and ChatGPT history are removed permanently. Research records it produced stay with their project.`,
      )
    )
      return;
    await workbenchClient.deleteSession({
      sessionId: session.id,
      operationId: newOperation("delete-session"),
    });
    if (sessionRef.current === session.id) {
      setSnapshot(null);
      setSessionId(null);
      setDraft("");
    }
    await loadSessions(workspaceId, showArchived);
  };

  const moveSession = async (
    session: WorkbenchSession,
    target: string | null,
  ) => {
    setMoving(true);
    setMoveError(null);
    try {
      await saveCurrentDraft();
      const latest = await workbenchClient.sessionSnapshot(session.id);
      await workbenchClient.moveSession({
        sessionId: session.id,
        expectedRevision: latest.session.revision,
        operationId: newOperation("move-session"),
        workspaceId: target,
      });
      setMoveTarget(null);
      if (sessionRef.current === session.id) {
        // Follow the conversation into its new project instead of losing it
        // behind the sidebar filter.
        if (target !== workspaceId) {
          setSessions([]);
          setWorkspaceId(target);
        }
        await hydrate(session.id);
      } else {
        await loadSessions(workspaceId, showArchived);
      }
    } catch (cause) {
      setMoveError(workbenchErrorMessage(cause));
    } finally {
      setMoving(false);
    }
  };

  const exportCurrent = async () => {
    const current = snapshotRef.current;
    if (!current) return;
    const path = await save({
      defaultPath: `${current.session.title.replace(/[^a-z0-9_-]+/gi, "-") || "conversation"}.md`,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (path)
      await workbenchClient.exportConversation(current.session.id, path);
  };

  const saveSelection = (nextModel: string, nextEffort: string) => {
    const selectedSession = sessionRef.current;
    if (!selectedSession) return;
    const pending = sessionWriteRef.current
      .catch(() => undefined)
      .then(async () => {
        const latest = await workbenchClient.sessionSnapshot(selectedSession);
        const updated = await workbenchClient.updateSession({
          sessionId: latest.session.id,
          expectedRevision: latest.session.revision,
          operationId: newOperation("model-selection"),
          overrides: {
            ...latest.session.overrides,
            model: nextModel || null,
            effort: nextEffort || null,
          },
        });
        if (sessionRef.current === selectedSession) {
          setSnapshot((old) =>
            mergeSessionSnapshot(old, {
              ...latest,
              session: updated.record,
              sequence: updated.sequence,
            }),
          );
        }
      });
    sessionWriteRef.current = pending;
    selectionSaveRef.current = pending;
    void pending.catch((cause) => {
      if (sessionRef.current === selectedSession)
        setError(
          `Model selection could not be saved: ${workbenchErrorMessage(cause)}. Select the model again to retry.`,
        );
    });
  };

  const send = async () => {
    hydrationGeneration.current += 1;
    if (
      !sessionId ||
      snapshotRef.current?.session.id !== sessionId ||
      !draft.trim() ||
      active ||
      submittingRef.current ||
      submissionRef.current ||
      contextBusyRef.current
    )
      return;
    submittingRef.current = true;
    setSubmitting(true);
    const text = draft.trim();
    const submission: PendingSubmission = { sessionId, text };
    submissionRef.current = submission;
    if (streamFlushRef.current !== null)
      window.clearTimeout(streamFlushRef.current);
    streamFlushRef.current = null;
    streamBufferRef.current = "";
    followLatestRef.current = true;
    setSelectedMessage(null);
    setJumpTarget(null);
    setTranscriptPage(0);
    setError(null);
    setPendingUser(text);
    setStream("");
    setDraft("");
    try {
      localStorage.removeItem(`pipeline.pendingDraft.${sessionId}`);
    } catch {
      /* The host retains submission recovery. */
    }
    let dispatched = false;
    try {
      await selectionSaveRef.current;
      await draftSaveRef.current;
      dispatched = true;
      const result = await workbenchClient.sendTurn({
        sessionId,
        text,
        clientSubmissionId: newOperation("submission"),
        model: model || null,
        effort: effort || null,
      });
      Object.assign(submission, {
        threadId: result.threadId,
        turnId: result.turnId,
        epoch: result.epoch,
      });
      const key = terminalKey(result.epoch, result.threadId, result.turnId);
      const terminal =
        completedTurnsRef.current.get(key) ??
        completedTurnsRef.current.get(terminalKey(result.epoch, "", ""));
      if (terminal) {
        completedTurnsRef.current.delete(key);
        finishSubmission(terminal);
      } else if (sessionRef.current === sessionId) {
        const turn = { threadId: result.threadId, turnId: result.turnId };
        activeRef.current = turn;
        setActive(turn);
        setSubmitting(false);
      }
      submittingRef.current = false;
      void workbenchClient
        .pendingRequests()
        .then((pending) => setRequests(pending))
        .catch(() => undefined);
    } catch (cause) {
      submittingRef.current = false;
      if (sessionRef.current === sessionId) setSubmitting(false);
      if (!dispatched) {
        submissionRef.current = null;
        if (sessionRef.current === sessionId) {
          setPendingUser(null);
          setDraft(text);
        }
        try {
          localStorage.setItem(`pipeline.pendingDraft.${sessionId}`, text);
        } catch {
          /* Keep the visible draft. */
        }
        if (sessionRef.current === sessionId)
          setError(
            `Message not sent because its settings or draft could not be saved: ${workbenchErrorMessage(cause)}`,
          );
        return;
      }
      if (sessionRef.current === sessionId)
        setError(
          `The submission may not have been acknowledged. It was preserved as a draft and will not be retried automatically. ${workbenchErrorMessage(cause)}`,
        );
      submissionRef.current = null;
      if (sessionRef.current === sessionId) setPendingUser(null);
      await hydrate(sessionId);
    }
  };

  const resolve = async (
    event: WorkbenchEvent,
    result?: Record<string, unknown>,
  ) => {
    if (event.requestId === undefined || !event.method) return;
    setRequests((old) =>
      old.filter(
        (item) =>
          item.requestId !== event.requestId || item.epoch !== event.epoch,
      ),
    );
    await workbenchClient
      .resolveServerRequest({
        epoch: event.epoch,
        requestId: event.requestId,
        method: event.method,
        result: result ?? null,
        declineMessage: result ? null : "The user declined this request",
      })
      .catch((cause) => setError(workbenchErrorMessage(cause)));
  };

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
  const outlineEntries = useMemo(
    () =>
      allRenderedItems
        .map((item) => ({
          id: item.id,
          text: payloadText(item),
          role: roleFor(item) as "You" | "ChatGPT",
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
  useEffect(() => {
    const open = (e: Event) => {
      const id = (e as CustomEvent<string>).detail;
      if (id)
        void workbenchClient
          .sessionSnapshot(id)
          .then(async (snapshot) => {
            setWorkspaceId(snapshot.session.workspaceId);
            await loadSessions(snapshot.session.workspaceId);
            await selectSession(id);
            setSurface("chat");
          })
          .catch((cause) => setError(workbenchErrorMessage(cause)));
    };
    const project = (e: Event) => {
      const id = (e as CustomEvent<string>).detail;
      if (id)
        void selectProject(id).catch((cause) =>
          setError(
            `Could not switch projects: ${workbenchErrorMessage(cause)}`,
          ),
        );
    };
    window.addEventListener("pipeline:open-session", open);
    window.addEventListener("pipeline:open-project", project);
    return () => {
      window.removeEventListener("pipeline:open-session", open);
      window.removeEventListener("pipeline:open-project", project);
    };
  }, [loadSessions, selectProject, selectSession]);
  const recordDestination = useCallback(
    (tab: WorkspaceDestination) => {
      setDestinationState({ workspaceId, tab });
    },
    [workspaceId],
  );
  const navigateProject = (tab: WorkspaceDestination) => {
    recordDestination(tab);
    setSurface("project");
    setNavigationOpen(false);
    // Re-selecting the current destination also leaves an opened object view.
    window.dispatchEvent(
      new CustomEvent("pipeline:research-destination", {
        detail: { workspaceId, tab },
      }),
    );
    setProjectRequest((value) => value + 1);
  };
  const acceptSnapshot = useCallback((next: ConversationSnapshot) => {
    if (sessionRef.current === next.session.id) setSnapshot(next);
  }, []);
  const openInspector = (
    next: "settings" | "outline" | "context" | "activity",
    trigger?: HTMLButtonElement,
  ) => {
    if (contextBusyRef.current || harnessEditor) return;
    if (trigger) inspectorTriggerRef.current = trigger;
    setInspector((value) => (value === next ? null : next));
    setAssistantRequest((value) => value + 1);
    setNavigationOpen(false);
  };
  const closeInspector = (restoreFocus = true) => {
    if (contextBusyRef.current) return;
    setInspector(null);
    if (restoreFocus)
      requestAnimationFrame(() => inspectorTriggerRef.current?.focus());
  };
  const navigationFits =
    shellWidth === null ||
    shellWidth - sidebarWidth >= (workspaceId ? 848 : 480);
  const navigationCollapsed = !navigationPinned || !navigationFits;
  const pinNavigation = (value: boolean) => {
    setNavigationPinned(value);
    setNavigationOpen(false);
    try {
      localStorage.setItem(
        "pipeline.workspace.navigationPinned",
        String(value),
      );
    } catch {
      /* Optional view preference. */
    }
    navigationButtonRef.current?.focus();
  };
  const navigationDialog = useModalDialog<HTMLDivElement>(
    () => setNavigationOpen(false),
    navigationCollapsed && navigationOpen,
    true,
  );
  useEffect(() => {
    setNavigationOpen(false);
    setToolPickerOpen(false);
  }, [sessionId, workspaceId, surface]);
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if (
        (event.metaKey || event.ctrlKey) &&
        event.key.toLowerCase() === "k" &&
        !event.altKey &&
        workspaceId &&
        !harnessEditor &&
        !contextBusy
      ) {
        // Keep shortcuts local to the project; another modal owns its keyboard.
        if (document.querySelector('[aria-modal="true"]') && !toolPickerOpen)
          return;
        event.preventDefault();
        setToolPickerOpen((value) => !value);
        setNavigationOpen(false);
      }
    };
    window.addEventListener("keydown", shortcut);
    return () => window.removeEventListener("keydown", shortcut);
  }, [workspaceId, harnessEditor, contextBusy, toolPickerOpen]);

  const currentWorkspace =
    workspaces.find((item) => item.id === workspaceId) ?? null;
  const projectFolder = currentWorkspace?.root
    ? currentWorkspace.root
        .replace(/[\\/]+$/, "")
        .split(/[\\/]/)
        .pop() || currentWorkspace.root
    : null;

  return {
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
    requests,
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
    navigationPinned,
    toolPickerOpen,
    setToolPickerOpen,
    conversationMenuRef,
    assistantRequest,
    harnessEditor,
    setHarnessEditor,
    selectedMessage,
    jumpTarget,
    contextBusy,
    setContextBusy,
    contextBusyRef,
    inspectorTriggerRef,
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
  };
}

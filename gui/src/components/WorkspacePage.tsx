import WorkspaceMenu from "./WorkspaceMenu";
import WorkspaceToolPicker from "./WorkspaceToolPicker";
import useModalDialog from "../hooks/useModalDialog";
import WorkspaceComposerControls from "./WorkspaceComposerControls";
import RetainedWorkspaceView from "./RetainedWorkspaceView";
import WorkspaceProjectNavigation from "./WorkspaceProjectNavigation";
import { loadDeskLayout } from "../lib/deskLayout";
import { isWorkspaceDestination, workspaceDestinations, type WorkspaceDestination } from "../lib/workspaceNavigation";
import WorkspaceDesk from "./WorkspaceDesk";
import useContainerWidth from "../hooks/useContainerWidth";
import WorkspaceConversationView, {
  payloadText,
  isMessage,
  roleFor,
} from "./WorkspaceConversationView";
import WorkspaceContextTray from "./WorkspaceContextTray";
import { deskClient, type ContextItem } from "../lib/deskClient";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import "./WorkspaceConversation.css";
import WorkspaceIcon from "./WorkspaceIcon";
import SidebarPanel, { SidebarHeader } from "./SidebarPanel";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";
import WorkspaceComposerMenu from "./WorkspaceComposerMenu";
import WorkspaceConversationOutline from "./WorkspaceConversationOutline";
import {
  ConversationMenu,
  MoveConversationDialog,
} from "./WorkspaceConversationActions";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  ConversationSnapshot,
  WorkbenchEvent,
  WorkbenchSession,
  Workspace,
  WorkspaceModel,
  ReviewHandoff,
} from "../lib/workbenchTypes";

const WorkspaceProjectSurface = lazy(() => import("./WorkspaceProjectSurface"));
const WorkspaceProjectDialog = lazy(() => import("./WorkspaceProjectDialog"));
const WorkspaceFollowups = lazy(() => import("./research-programs/Followups"));
const WorkspaceTaskCards = lazy(() => import("./WorkspaceTaskCards"));
const WorkspaceResearchPanel = lazy(() => import("./WorkspaceResearchPanel"));
const WorkspaceHarnessEditor = lazy(() => import("./WorkspaceHarnessEditor"));
const TRANSCRIPT_PAGE_SIZE = 200;
const STREAM_FLUSH_MS = 50;
/** Placeholder title until the first exchange is auto-titled or the user renames. */
const DEFAULT_TITLE = "New conversation";

function newOperation(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

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
  onOpenSettings,
  onReviewHandoff,
  onTasks,
}: {
  onTasks?: (sessionId: string, taskId?: string) => void;
  onOpenSettings: () => void;
  onReviewHandoff?: (handoff: ReviewHandoff) => void;
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
  const [inspector, setInspector] = useState<"settings" | "outline" | "context" | "activity" | null>(null);
  const [projectRequest, setProjectRequest] = useState(0);
  const [resetRequest, setResetRequest] = useState(0);
  const [destinationState, setDestinationState] = useState<{ workspaceId: string | null; tab: WorkspaceDestination } | null>(null);
  const savedDestination = useMemo(() => workspaceId ? loadDeskLayout(workspaceId).tab : "overview", [workspaceId]);
  const destination = destinationState?.workspaceId === workspaceId ? destinationState.tab : isWorkspaceDestination(savedDestination) ? savedDestination : "overview";
  const [shellRef, shellWidth] = useContainerWidth<HTMLDivElement>();
  const [navigationOpen, setNavigationOpen] = useState(false);
  const navigationButtonRef = useRef<HTMLButtonElement>(null);
  const [navigationPinned, setNavigationPinned] = useState(() => {
    try { return localStorage.getItem("pipeline.workspace.navigationPinned") === "true"; } catch { return false; }
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
  const [surface, setSurface] = useState<"chat" | "project">(() =>
    Boolean(localStorage.getItem("pipeline.workspace.workspaceId"))
      ? "project"
      : "chat",
  );
  useEffect(() => {
    const open = (event: Event) => {
      if (
        (event as CustomEvent<{ workspaceId: string }>).detail.workspaceId ===
        workspaceId
      )
        setSurface("project");
    };
    window.addEventListener("pipeline:open-file", open);
    return () => window.removeEventListener("pipeline:open-file", open);
  }, [workspaceId]);
  const [projectDialog, setProjectDialog] = useState(false);
  const [moveTarget, setMoveTarget] = useState<WorkbenchSession | null>(null);
  const [moving, setMoving] = useState(false);
  const [moveError, setMoveError] = useState<string | null>(null);
  const [transcriptPage, setTranscriptPage] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const sessionRef = useRef(sessionId);
  const snapshotRef = useRef(snapshot);
  const pendingUserRef = useRef(pendingUser);
  const activeRef = useRef(active);
  const submittingRef = useRef(false);
  const completedTurnsRef = useRef(new Set<string>());
  const selectionSaveRef = useRef<Promise<void>>(Promise.resolve());
  const sessionWriteRef = useRef<Promise<unknown>>(Promise.resolve());
  const sessionListSequence = useRef(0);
  const draftSaveRef = useRef<Promise<unknown>>(Promise.resolve());
  const streamBufferRef = useRef("");
  const streamFlushRef = useRef<number | null>(null);
  sessionRef.current = sessionId;
  snapshotRef.current = snapshot;
  pendingUserRef.current = pendingUser;
  activeRef.current = active;

  const draftRef = useRef(draft);
  draftRef.current = draft;
  const persistDraft = useCallback((selectedSession: string, text: string) => {
    // Navigation can flush a draft while its debounce save is in flight.
    const pending = sessionWriteRef.current
      .catch(() => undefined)
      .then(async () => {
        const latest =
          await workbenchClient.conversationSnapshot(selectedSession);
        if (latest.session.draft === text) {
          try {
            if (
              localStorage.getItem(
                `pipeline.pendingDraft.${selectedSession}`,
              ) === text
            )
              localStorage.removeItem(
                `pipeline.pendingDraft.${selectedSession}`,
              );
          } catch {
            /* Optional recovery cache. */
          }
          return latest;
        }
        const updated = await workbenchClient.updateSession({
          sessionId: selectedSession,
          expectedRevision: latest.session.revision,
          operationId: newOperation("save-draft"),
          draft: text,
        });
        try {
          if (
            localStorage.getItem(`pipeline.pendingDraft.${selectedSession}`) ===
            text
          )
            localStorage.removeItem(`pipeline.pendingDraft.${selectedSession}`);
        } catch {
          /* Optional recovery cache. */
        }
        return {
          ...latest,
          session: updated.record,
          sequence: updated.sequence,
        };
      });
    draftSaveRef.current = pending;
    sessionWriteRef.current = pending;
    return pending;
  }, []);

  const editDraft = (text: string) => {
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
      try { remembered = localStorage.getItem(`pipeline.workspace.lastSession.${selectedWorkspace ?? "unfiled"}`); } catch { /* Optional navigation state. */ }
      setSessions(response.sessions);
      setSessionId((current) =>
        response.sessions.some((session) => session.id === current)
          ? current
          : (response.sessions.find(session => session.id === remembered)?.id ?? response.sessions[0]?.id ?? null),
      );
    },
    [showArchived],
  );

  const hydrate = useCallback(async (selectedSession = sessionRef.current) => {
    if (!selectedSession) {
      if (!sessionRef.current) setSnapshot(null);
      return;
    }
    const next = await workbenchClient.conversationSnapshot(selectedSession);
    if (sessionRef.current !== selectedSession) return;
    setSnapshot(next);
    let recoveredDraft: string | null = null;
    try {
      recoveredDraft = localStorage.getItem(
        `pipeline.pendingDraft.${selectedSession}`,
      );
    } catch {
      /* Optional recovery cache. */
    }
    setDraft(recoveredDraft ?? next.session.draft);
    const overrides = next.session.overrides;
    setModel(typeof overrides.model === "string" ? overrides.model : "");
    setEffort(typeof overrides.effort === "string" ? overrides.effort : "");
    const inProgress = [...next.turns]
      .reverse()
      .find((turn) => !turn.terminalAt && turn.providerTurnId);
    if (inProgress && next.activeBinding) {
      setActive({
        threadId: next.activeBinding.providerThreadId,
        turnId: inProgress.providerTurnId!,
      });
    } else {
      setActive(null);
    }
  }, []);

  /** Refreshes one conversation's record in the list and, when it is open,
   *  in the snapshot, without touching the in-progress composer draft. */
  const refreshSessionRecord = useCallback(async (id: string) => {
    const latest = await workbenchClient.conversationSnapshot(id);
    setSessions((old) =>
      old.map((session) => (session.id === id ? latest.session : session)),
    );
    if (sessionRef.current === id)
      setSnapshot((old) =>
        old && old.session.id === id
          ? { ...old, session: latest.session, workspace: latest.workspace }
          : old,
      );
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
              `Workspace runtime could not connect. Saved conversations remain available: ${workbenchErrorMessage(cause)}`,
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
    if (sessionId && snapshot?.session.id === sessionId && snapshot.session.workspaceId === workspaceId) {
      try { localStorage.setItem(`pipeline.workspace.lastSession.${workspaceId ?? "unfiled"}`, sessionId); } catch { /* Optional preference. */ }
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
          if (sessionRef.current === selectedSession) setSnapshot(next);
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

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<WorkbenchEvent>("workbench:event", ({ payload }) => {
      if (disposed) return;
      const boundThread = snapshotRef.current?.activeBinding?.providerThreadId;
      const expectedThread = activeRef.current?.threadId ?? boundThread;
      const matchesThread = (threadId: unknown) =>
        submittingRef.current || !expectedThread || threadId === expectedThread;
      if (payload.kind === "connectionClosed") {
        const preserved = pendingUserRef.current;
        if (preserved) {
          setDraft(preserved);
          const current = snapshotRef.current;
          if (current)
            void persistDraft(current.session.id, preserved).catch(
              () => undefined,
            );
        }
        pendingUserRef.current = null;
        activeRef.current = null;
        submittingRef.current = false;
        setError(
          `Workspace connection closed. ${preserved ? "The unconfirmed message was restored to the composer. " : ""}${workbenchErrorMessage(payload.reason ?? "")}`,
        );
        setSubmitting(false);
        setActive(null);
        setPendingUser(null);
        setStream("");
        setRequests([]);
      }
      if (
        payload.kind === "accountUpdated" ||
        payload.kind === "accountLoginCompleted"
      ) {
        void workbenchClient
          .accountState(true)
          .then(async (account) => {
            if (account.status === "chatgpt")
              setModels((await workbenchClient.modelCatalog()).models);
            else
              setError(
                "Workspace is no longer signed in to ChatGPT. Your draft and conversation remain local.",
              );
          })
          .catch((cause) => setError(workbenchErrorMessage(cause)));
      }
      if (
        payload.kind === "sessionTitleUpdated" &&
        typeof payload.sessionId === "string"
      ) {
        void refreshSessionRecord(payload.sessionId).catch(() => undefined);
      }
      if (
        payload.kind === "sessionTitleFailed" &&
        payload.sessionId === sessionRef.current
      ) {
        setError(
          `Automatic title unavailable: ${workbenchErrorMessage(payload.error ?? "")}. Rename the conversation from its menu.`,
        );
      }
      if (
        payload.kind === "serverRequest" &&
        String(payload.params?.tool ?? "").startsWith("workbench_task_") &&
        matchesThread(payload.params?.threadId)
      ) {
        localStorage.setItem("pipeline.tasks.enabled", "true");
        setTasksEnabled(true);
      }
      if (
        payload.kind === "serverRequest" &&
        matchesThread(payload.params?.threadId)
      ) {
        setRequests((old) =>
          old.some((item) => item.requestId === payload.requestId)
            ? old
            : [...old, payload],
        );
      }
      if (payload.kind === "serverRequestResolved")
        setRequests((old) =>
          old.filter((item) => item.requestId !== payload.requestId),
        );
      if (
        payload.kind === "agentMessageDelta" &&
        matchesThread(payload.threadId)
      ) {
        streamBufferRef.current += String(payload.delta ?? "");
        if (streamFlushRef.current === null) {
          streamFlushRef.current = window.setTimeout(() => {
            const delta = streamBufferRef.current;
            streamBufferRef.current = "";
            streamFlushRef.current = null;
            if (delta) setStream((old) => old + delta);
          }, STREAM_FLUSH_MS);
        }
      }
      if (payload.kind === "turnCompleted" && matchesThread(payload.threadId)) {
        const terminalKey = `${String(payload.threadId)}:${String(payload.turnId)}`;
        completedTurnsRef.current.add(terminalKey);
        if (completedTurnsRef.current.size > 100) {
          const oldest = completedTurnsRef.current.values().next().value;
          if (oldest) completedTurnsRef.current.delete(oldest);
        }
        let restoredFailedDraft = false;
        if (payload.status !== "completed" && pendingUserRef.current) {
          const preserved = pendingUserRef.current;
          setDraft(preserved);
          const current = snapshotRef.current;
          if (current) {
            restoredFailedDraft = true;
            void persistDraft(current.session.id, preserved)
              .then((next) => {
                if (sessionRef.current === current.session.id)
                  setSnapshot(next);
              })
              .catch(() => undefined);
          }
          setError(
            `The turn ended with status ${String(payload.status ?? "failed")}. The message was restored to the composer and was not sent to another provider.`,
          );
        }
        if (streamFlushRef.current !== null)
          window.clearTimeout(streamFlushRef.current);
        streamFlushRef.current = null;
        streamBufferRef.current = "";
        submittingRef.current = false;
        activeRef.current = null;
        pendingUserRef.current = null;
        setSubmitting(false);
        setActive(null);
        setPendingUser(null);
        setStream("");
        if (!restoredFailedDraft) {
          const selectedSession = sessionRef.current;
          if (selectedSession) {
            void workbenchClient
              .reconcileSession(selectedSession)
              .catch(() => false)
              .finally(() => void hydrate(selectedSession));
          }
        }
      }
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
      if (streamFlushRef.current !== null)
        window.clearTimeout(streamFlushRef.current);
      streamFlushRef.current = null;
      streamBufferRef.current = "";
    };
  }, [hydrate, persistDraft, refreshSessionRecord]);

  const saveCurrentDraft = async () => {
    await selectionSaveRef.current;
    if (sessionId && !active && !submittingRef.current)
      await persistDraft(sessionId, draft);
  };

  const selectProject = async (id: string | null) => {
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
  };

  const selectSession = async (id: string) => {
    if (contextBusyRef.current || harnessEditor) return;
    await saveCurrentDraft();
    if (id !== sessionId) {
      setSnapshot(null);
      setDraft("");
    }
    setSessionId(id);
    if (!workspaceId) setSurface("chat");
    setAssistantRequest(value => value + 1);
  };

  const createSession = async () => {
    if (contextBusyRef.current || harnessEditor) return;
    await saveCurrentDraft();
    const created = await workbenchClient.createSession({
      workspaceId,
      title: DEFAULT_TITLE,
      operationId: newOperation("create-session"),
    });
    await loadSessions(workspaceId);
    setSnapshot(null);
    setDraft("");
    setSessionId(created.record.id);
    if (!workspaceId) setSurface("chat");
    setAssistantRequest(value => value + 1);
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
      setAssistantRequest(value => value + 1);
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
    const latest = await workbenchClient.conversationSnapshot(session.id);
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
    const latest = await workbenchClient.conversationSnapshot(session.id);
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
      const latest = await workbenchClient.conversationSnapshot(session.id);
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
        const latest =
          await workbenchClient.conversationSnapshot(selectedSession);
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
          setSnapshot({
            ...latest,
            session: updated.record,
            sequence: updated.sequence,
          });
        }
      });
    sessionWriteRef.current = pending;
    selectionSaveRef.current = pending;
    void pending.catch(cause => {
      if (sessionRef.current === selectedSession) setError(`Model selection could not be saved: ${workbenchErrorMessage(cause)}. Select the model again to retry.`);
    });
  };

  const send = async () => {
    if (
      !sessionId ||
      snapshotRef.current?.session.id !== sessionId ||
      !draft.trim() ||
      active ||
      submittingRef.current ||
      contextBusyRef.current
    )
      return;
    submittingRef.current = true;
    setSubmitting(true);
    const text = draft.trim();
    pendingUserRef.current = text;
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
      const turn = { threadId: result.threadId, turnId: result.turnId };
      const terminalKey = `${result.threadId}:${result.turnId}`;
      if (completedTurnsRef.current.delete(terminalKey)) {
        activeRef.current = null;
        setActive(null);
      } else {
        activeRef.current = turn;
        setActive(turn);
      }
      setSubmitting(false);
      submittingRef.current = false;
    } catch (cause) {
      submittingRef.current = false;
      setSubmitting(false);
      if (!dispatched) {
        pendingUserRef.current = null; setPendingUser(null); setDraft(text);
        try { localStorage.setItem(`pipeline.pendingDraft.${sessionId}`, text); } catch { /* Keep the visible draft. */ }
        setError(`Message not sent because its settings or draft could not be saved: ${workbenchErrorMessage(cause)}`);
        return;
      }
      setError(
        `The submission may not have been acknowledged. It was preserved as a draft and will not be retried automatically. ${workbenchErrorMessage(cause)}`,
      );
      pendingUserRef.current = null;
      setPendingUser(null);
      await hydrate(sessionId);
    }
  };

  const resolve = async (
    event: WorkbenchEvent,
    result?: Record<string, unknown>,
  ) => {
    if (event.requestId === undefined || !event.method) return;
    setRequests((old) =>
      old.filter((item) => item.requestId !== event.requestId),
    );
    await workbenchClient
      .resolveServerRequest({
        requestId: event.requestId,
        method: event.method,
        result: result ?? null,
        declineMessage: result ? null : "The user declined this request",
      })
      .catch((cause) => setError(workbenchErrorMessage(cause)));
  };

  const allRenderedItems = useMemo(
    () => snapshot?.items.filter(isMessage) ?? [],
    [snapshot],
  );
  const maximumTranscriptPage = Math.max(
    0,
    Math.ceil(allRenderedItems.length / TRANSCRIPT_PAGE_SIZE) - 1,
  );
  useEffect(() => {
    setTranscriptPage((page) => Math.min(page, maximumTranscriptPage));
  }, [maximumTranscriptPage]);
  const transcriptEnd = Math.max(
    0,
    allRenderedItems.length - transcriptPage * TRANSCRIPT_PAGE_SIZE,
  );
  const transcriptStart = Math.max(0, transcriptEnd - TRANSCRIPT_PAGE_SIZE);
  const renderedItems = useMemo(
    () => allRenderedItems.slice(transcriptStart, transcriptEnd),
    [allRenderedItems, transcriptEnd, transcriptStart],
  );
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
    setTranscriptPage(
      Math.floor((allRenderedItems.length - 1 - index) / TRANSCRIPT_PAGE_SIZE),
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
      setWorkspaceId((e as CustomEvent<string>).detail);
      setSurface("project");
    };
    window.addEventListener("pipeline:open-session", open);
    window.addEventListener("pipeline:open-project", project);
    return () => {
      window.removeEventListener("pipeline:open-session", open);
      window.removeEventListener("pipeline:open-project", project);
    };
  }, [loadSessions, selectSession]);
  const recordDestination = useCallback((tab: WorkspaceDestination) => {
    setDestinationState({ workspaceId, tab });
  }, [workspaceId]);
  const navigateProject = (tab: WorkspaceDestination) => {
    recordDestination(tab); setSurface("project"); setNavigationOpen(false);
    // Re-selecting the current destination also leaves an opened object view.
    window.dispatchEvent(new CustomEvent("pipeline:research-destination", { detail: { workspaceId, tab } }));
    setProjectRequest(value => value + 1);
  };
  const acceptSnapshot = useCallback((next: ConversationSnapshot) => {
    if (sessionRef.current === next.session.id) setSnapshot(next);
  }, []);
  const openInspector = (next: "settings" | "outline" | "context" | "activity", trigger?: HTMLButtonElement) => {
    if (contextBusyRef.current || harnessEditor) return;
    if (trigger) inspectorTriggerRef.current = trigger;
    setInspector(value => value === next ? null : next);
    setAssistantRequest(value => value + 1); setNavigationOpen(false);
  };
  const closeInspector = (restoreFocus = true) => {
    if (contextBusyRef.current) return;
    setInspector(null);
    if (restoreFocus) requestAnimationFrame(() => inspectorTriggerRef.current?.focus());
  };
  const navigationFits = shellWidth === null || shellWidth - sidebarWidth >= (workspaceId ? 848 : 480);
  const navigationCollapsed = !navigationPinned || !navigationFits;
  const pinNavigation = (value: boolean) => {
    setNavigationPinned(value); setNavigationOpen(false);
    try { localStorage.setItem("pipeline.workspace.navigationPinned", String(value)); } catch { /* Optional view preference. */ }
    navigationButtonRef.current?.focus();
  };
  const navigationDialog = useModalDialog<HTMLDivElement>(() => setNavigationOpen(false), navigationCollapsed && navigationOpen, true);
  useEffect(() => { setNavigationOpen(false); setToolPickerOpen(false); }, [sessionId, workspaceId, surface]);
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k" && !event.altKey && workspaceId && !harnessEditor && !contextBusy) {
        // Keep shortcuts local to Workspace; another modal owns its keyboard.
        if (document.querySelector('[aria-modal="true"]') && !toolPickerOpen) return;
        event.preventDefault(); setToolPickerOpen(value => !value); setNavigationOpen(false);
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

  const navigation = (
      <SidebarPanel
        fill={navigationCollapsed}
        aria-label="Workspace navigation"
        className={navigationCollapsed ? "workspace-navigation-drawer" : ""}
        width={navigationCollapsed ? shellWidth ?? sidebarWidth : sidebarWidth}
        defaultWidth={224}
        min={200}
        max={440}
        onResize={setSidebarWidth}
        resizeLabel="Resize Workspace sidebar"
      >
        <SidebarHeader
          title="Workspace"
          actions={
            <>
              <button
                type="button"
                title="Connection settings"
                aria-label="Workspace connection settings"
                onClick={onOpenSettings}
                className="workspace-sidebar-icon-button"
              >
                <WorkspaceIcon name="settings" size={18} />
              </button>
              <button type="button" className="workspace-sidebar-icon-button" aria-label="Close navigation" title="Close navigation"
                onClick={() => { if (navigationCollapsed) setNavigationOpen(false); else pinNavigation(false); }}><WorkspaceIcon name="close"/></button>
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
                  onClick={() => { setNavigationOpen(false); setProjectDialog(true); }}
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
                  void selectProject(event.target.value || null).catch(
                    (cause) => setError(workbenchErrorMessage(cause)),
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
              {currentWorkspace && <p className="mt-2 truncate text-xs text-gray-500" title={currentWorkspace.root ?? undefined}>{currentWorkspace.missingRootAt ? "Folder not found" : projectFolder ?? "No folder attached"}</p>}

            </div>
          </div>
          {workspaceId && <WorkspaceProjectNavigation key={workspaceId} workspaceId={workspaceId} destination={destination}
            disabled={contextBusy || harnessEditor} onNavigate={navigateProject}
            onResetLayout={() => { setSidebarWidth(224); setResetRequest(value => value + 1); }} />}
          <details className="workspace-conversations" open={!workspaceId || undefined}>
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

        {navigationCollapsed && <button type="button" className="workspace-pin-navigation" disabled={!navigationFits} onClick={() => pinNavigation(true)}>Keep navigation open</button>}
      </SidebarPanel>
  );

  return (
    <div ref={shellRef} className="workspace-chat-shell flex h-full min-h-0 min-w-0 bg-white dark:bg-neutral-950">
      {navigationCollapsed ? <div ref={navigationDialog} hidden={!navigationOpen} role="dialog" aria-modal={navigationOpen ? true : undefined} aria-label="Browse Workspace" className="workspace-navigation-overlay">
        <div className="workspace-navigation-backdrop" onPointerDown={() => setNavigationOpen(false)} />
        {navigation}
      </div> : navigation}
      {toolPickerOpen && workspaceId && <WorkspaceToolPicker workspaceId={workspaceId} current={destination} onChoose={navigateProject} onClose={() => setToolPickerOpen(false)} />}

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
      <WorkspaceDesk key={workspaceId ?? "unfiled"} workspaceId={workspaceId}
        navigationHidden={navigationCollapsed} navigationButtonRef={navigationButtonRef}
        onNavigation={() => { if (!navigationCollapsed) pinNavigation(false); else setNavigationOpen(value => !value); }}
        navigationOpen={navigationOpen} projectName={currentWorkspace?.name}
        toolLabel={workspaceDestinations[destination].label} onTools={workspaceId ? () => setToolPickerOpen(true) : undefined}
        toolsDisabled={contextBusy || harnessEditor} attentionCount={visibleRequests.length}
        active={Boolean(active) || submitting}
        onStop={active ? () => { void workbenchClient.interruptTurn(active.threadId, active.turnId); } : undefined}
        assistantRequest={assistantRequest} projectRequest={projectRequest} resetRequest={resetRequest}
        project={workspaceId || (harnessEditor && snapshot) ? <>
          {workspaceId && <div hidden={harnessEditor} className="workspace-retained-view">
            <Suspense fallback={<section className="workspace-panel-loading">Opening project…</section>}>
              <WorkspaceProjectSurface key={workspaceId} workspaceId={workspaceId} sessionId={sessionId}
                destination={destination} onDestination={recordDestination} navigationInSidebar snapshot={snapshot}
                onSnapshot={acceptSnapshot} onConversation={openResearchConversation}
                onWorkspaceChanged={updateProjectWorkspace} onReviewHandoff={onReviewHandoff} />
            </Suspense>
          </div>}
          {snapshot && harnessEditor && <Suspense fallback={<section className="workspace-panel-loading">Opening assistant editor…</section>}>
            <WorkspaceHarnessEditor key={snapshot.session.id} snapshot={snapshot} onSnapshot={acceptSnapshot}
              beforeChange={saveCurrentDraft}
              disabled={Boolean(active) || submitting}
              onBusy={value => { contextBusyRef.current = value; setContextBusy(value); }}
              onClose={() => { setHarnessEditor(false); setInspector("settings"); }} />
          </Suspense>}
        </> : null}
      >
        <section className="workspace-assistant relative flex min-h-0 min-w-0 flex-1 flex-col"
          onKeyDown={event => { if (event.key === "Escape" && inspector && !contextBusyRef.current) { event.stopPropagation(); closeInspector(); } }}>
          <header className="workspace-chat-header workspace-chat-header-minimal">
            <div className="workspace-chat-heading">
              <h1 className="truncate text-sm font-semibold">{snapshot?.session.title ?? DEFAULT_TITLE}</h1>
              {(active || submitting) && <p role="status">ChatGPT is working…</p>}
            </div>
            {workspaceId && <button type="button" className="workspace-sidebar-icon-button" disabled={contextBusy || harnessEditor} aria-label="New chat" title="New chat"
              onClick={() => void createSession().catch(cause => setError(workbenchErrorMessage(cause)))}><WorkspaceIcon name="plus"/></button>}
            <WorkspaceMenu label="Conversation menu" triggerRef={conversationMenuRef} disabled={!snapshot || contextBusy || harnessEditor}>
              {sessions.length > 1 && <label>Conversation<select aria-label="Assistant conversation" value={sessionId ?? ""}
                onChange={event => void selectSession(event.target.value).catch(cause => setError(workbenchErrorMessage(cause)))}>
                {sessions.map(session => <option key={session.id} value={session.id}>{session.title}{session.archivedAt ? " · Archived" : ""}</option>)}
              </select></label>}
              <button type="button" onClick={() => openInspector("outline", conversationMenuRef.current ?? undefined)}>Outline</button>
              <button type="button" onClick={() => openInspector("context", conversationMenuRef.current ?? undefined)}>Context</button>
              <button type="button" onClick={() => openInspector("activity", conversationMenuRef.current ?? undefined)}>Activity & follow-ups</button>
              <button type="button" onClick={() => void exportCurrent()}>Export conversation</button>
              {snapshot && <button type="button" onClick={() => void archiveSession(snapshot.session).catch(cause => setError(workbenchErrorMessage(cause)))}>{snapshot.session.archivedAt ? "Restore conversation" : "Archive conversation"}</button>}
            </WorkspaceMenu>
          </header>
          <WorkspaceConversationView
            header={null}
            inspectorOpen={Boolean(inspector)}
            inspector={<>
              <RetainedWorkspaceView key={`settings-${sessionId}`} active={inspector === "settings"}>
                {snapshot && <Suspense fallback={<section className="workspace-panel-loading">Opening assistant settings…</section>}>
                  <WorkspaceResearchPanel key={snapshot.session.id} embedded active={inspector === "settings"} title="Assistant settings" snapshot={snapshot} onSnapshot={acceptSnapshot}
                    beforeChange={saveCurrentDraft}
                    initialTab="setup" allowedTabs={["setup", "recipes"]} onBusy={value => { contextBusyRef.current = value; setContextBusy(value); }} settingsDisabled={Boolean(active) || submitting || contextBusy}
                    onClose={() => closeInspector()} onError={setError}
                    onEditHarness={() => { setInspector(null); setHarnessEditor(true); setProjectRequest(value => value + 1); }}
                    onMove={() => { setMoveError(null); setMoveTarget(snapshot.session); }} />
                </Suspense>}
              </RetainedWorkspaceView>
              <RetainedWorkspaceView key={`activity-${sessionId}`} active={inspector === "activity"}>
                <section className="workspace-simple-inspector" aria-label="Assistant activity">
                  <div className="workspace-inspector-heading"><h2>Activity</h2><button type="button" onClick={() => closeInspector()} aria-label="Close inspector">×</button></div>
                  <p role="status">{active || submitting ? "ChatGPT is working…" : "No response running."}</p>
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
                  <details className="mt-5"><summary className="cursor-pointer text-xs">Recent responses</summary>
                    {snapshot?.turns.slice(-10).reverse().map(turn => <div key={turn.id} className="workspace-activity-row">{turn.state ?? (turn.terminalAt ? "Completed" : "Working")}<span>{turn.createdAt ?? ""}</span></div>)}
                  </details>
                </section>
              </RetainedWorkspaceView>
              {inspector === "outline" ? <WorkspaceConversationOutline embedded key={sessionId}
              entries={outlineEntries} selectedId={selectedMessage}
              onJump={id => { closeInspector(false); jumpToMessage(id); }} onClose={() => closeInspector()} />
              : inspector === "context" ? <section className="workspace-simple-inspector" aria-label="Conversation context">
                <div className="workspace-inspector-heading"><h2>Conversation context</h2><button type="button" onClick={() => closeInspector()} aria-label="Close inspector">×</button></div>
                <p>{snapshot?.workspace?.name ?? "Unfiled conversation"}</p>
                <p className="text-xs text-gray-500">Expand Sources above your message to see selected material and its roles. Opening a file does not add it to chat.</p>
                {workspaceId ? <button type="button" onClick={() => navigateProject("documents")}>Browse project documents</button> : <button type="button" onClick={() => { setNavigationOpen(false); setProjectDialog(true); }}>New project</button>}
                <button type="button" onClick={() => setInspector("settings")}>Instructions and tools</button>
              </section> : null}</>}
            composerControls={<WorkspaceComposerControls models={models} model={model} effort={effort}
              disabled={!snapshot || contextBusy || Boolean(active) || submitting}
              onChange={(nextModel, nextEffort) => { setModel(nextModel); setEffort(nextEffort); saveSelection(nextModel, nextEffort); }} />}
            composerSetup={snapshot && <button type="button" className="workspace-assistant-setup" aria-label="Assistant settings"
              title={typeof snapshot.session.overrides.mode === "string" ? `Assistant settings · ${snapshot.session.overrides.mode === "inspect" ? "Read only" : "Allow edits"}` : "Assistant settings"} disabled={contextBusy || harnessEditor}
              onClick={event => openInspector("settings", event.currentTarget)}>
              <WorkspaceIcon name="settings" />
            </button>}
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

import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import ReactMarkdown from "react-markdown";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import "./WorkspaceConversation.css";
import WorkspaceIcon from "./WorkspaceIcon";
import SafeMarkdownLink from "./SafeMarkdownLink";
import WorkspaceMessageActions from "./WorkspaceMessageActions";
import WorkspaceComposerMenu from "./WorkspaceComposerMenu";
import WorkspaceConversationOutline from "./WorkspaceConversationOutline";
import { ConversationMenu, MoveConversationDialog } from "./WorkspaceConversationActions";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  ConversationSnapshot,
  TranscriptItem,
  WorkbenchEvent,
  WorkbenchSession,
  Workspace,
  WorkspaceModel,
  ReviewHandoff,
} from "../lib/workbenchTypes";

const WorkspaceProjectSurface = lazy(() => import("./WorkspaceProjectSurface"));
const WorkspaceProjectDialog = lazy(() => import("./WorkspaceProjectDialog"));
const WorkspaceResearchPanel = lazy(() => import("./WorkspaceResearchPanel"));
const WorkspaceHarnessEditor = lazy(() => import("./WorkspaceHarnessEditor"));
const TRANSCRIPT_PAGE_SIZE = 200;
const STREAM_FLUSH_MS = 50;
/** Placeholder title until the first exchange is auto-titled or the user renames. */
const DEFAULT_TITLE = "New conversation";

function newOperation(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

function payloadText(item: TranscriptItem): string {
  const payload = item.payload;
  if (!payload) return "";
  for (const key of ["text", "message", "content"]) {
    const value = payload[key];
    if (typeof value === "string") return value;
    if (Array.isArray(value)) {
      const joined = value.map((part) => {
        if (typeof part === "string") return part;
        if (part && typeof part === "object" && "text" in part && typeof part.text === "string") return part.text;
        return "";
      }).filter(Boolean).join("\n");
      if (joined) return joined;
    }
  }
  return "";
}

function isMessage(item: TranscriptItem) {
  return item.itemKind.toLowerCase().includes("message");
}

function roleFor(item: TranscriptItem) {
  return item.itemKind.toLowerCase().includes("user") ? "You" : "ChatGPT";
}

function RequestCard({ event, onResolve }: { event: WorkbenchEvent; onResolve: (event: WorkbenchEvent, result?: Record<string, unknown>) => void }) {
  const params = event.params ?? {};
  const questions = Array.isArray(params.questions) ? params.questions as Array<Record<string, unknown>> : [];
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const isQuestion = event.method === "item/tool/requestUserInput";
  const isPermission = event.method === "item/permissions/requestApproval";
  return (
    <div className="rounded-xl border border-amber-300 bg-amber-50 p-4 text-sm dark:border-amber-800 dark:bg-amber-950/30">
      <div className="font-semibold text-amber-950 dark:text-amber-100">{isQuestion ? "ChatGPT has a question" : isPermission ? "Permission requested" : "Approval requested"}</div>
      {!isQuestion && <p className="mt-2 whitespace-pre-wrap text-amber-900 dark:text-amber-200">{String(params.reason ?? params.command ?? "Review this request before continuing.")}</p>}
      {questions.map((question) => {
        const id = String(question.id ?? "question");
        const options = Array.isArray(question.options) ? question.options as Array<Record<string, unknown>> : [];
        return <label key={id} className="mt-3 block">
          <span className="block font-medium">{String(question.question ?? "Response")}</span>
          {options.length ? <select value={answers[id] ?? ""} onChange={(e) => setAnswers((old) => ({ ...old, [id]: e.target.value }))} className="mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-900">
            <option value="">Choose…</option>
            {options.map((option) => <option key={String(option.label)} value={String(option.label)}>{String(option.label)}</option>)}
          </select> : <input type={question.isSecret ? "password" : "text"} value={answers[id] ?? ""} onChange={(e) => setAnswers((old) => ({ ...old, [id]: e.target.value }))} className="mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-900" />}
        </label>;
      })}
      <div className="mt-3 flex gap-2">
        <button type="button" onClick={() => {
          if (isQuestion) {
            onResolve(event, { answers: Object.fromEntries(Object.entries(answers).map(([id, answer]) => [id, { answers: [answer] }])) });
          } else if (isPermission) {
            onResolve(event, { permissions: params.permissions as Record<string, unknown>, scope: "turn" });
          } else {
            onResolve(event, { decision: "accept" });
          }
        }} className="rounded bg-amber-900 px-3 py-1.5 font-medium text-white dark:bg-amber-100 dark:text-amber-950">{isQuestion ? "Submit" : "Allow once"}</button>
        <button type="button" onClick={() => {
          if (isQuestion) onResolve(event);
          else if (isPermission) onResolve(event, { permissions: {}, scope: "turn" });
          else onResolve(event, { decision: "decline" });
        }} className="rounded border border-amber-400 px-3 py-1.5">Decline</button>
      </div>
    </div>
  );
}

export default function WorkspacePage({ onOpenSettings, onReviewHandoff }: { onOpenSettings: () => void; onReviewHandoff?: (handoff: ReviewHandoff) => void }) {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspaceId, setWorkspaceId] = useState<string | null>(() => localStorage.getItem("pipeline.workspace.workspaceId"));
  const [sessions, setSessions] = useState<WorkbenchSession[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(() => localStorage.getItem("pipeline.workspace.sessionId"));
  const [snapshot, setSnapshot] = useState<ConversationSnapshot | null>(null);
  const [models, setModels] = useState<WorkspaceModel[]>([]);
  const [model, setModel] = useState("");
  const [effort, setEffort] = useState("");
  const [draft, setDraft] = useState("");
  const [stream, setStream] = useState("");
  const [pendingUser, setPendingUser] = useState<string | null>(null);
  const [active, setActive] = useState<{ threadId: string; turnId: string } | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [requests, setRequests] = useState<WorkbenchEvent[]>([]);
  const [showArchived, setShowArchived] = useState(false);
  const [showResearch, setShowResearch] = useState(false);
  const [showOutline, setShowOutline] = useState(false);
  const [harnessEditor, setHarnessEditor] = useState(false);
  const [selectedMessage, setSelectedMessage] = useState<string | null>(null);
  const [jumpTarget, setJumpTarget] = useState<string | null>(null);
  const [contextBusy, setContextBusy] = useState(false);
  const contextBusyRef = useRef(false);
  const outlineButtonRef = useRef<HTMLButtonElement>(null);
  const messageRef = useRef<HTMLTextAreaElement>(null);
  const transcriptRef = useRef<HTMLDivElement>(null);
  const followLatestRef = useRef(true);
  const [surface, setSurface] = useState<"chat" | "project">("chat");
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
  const draftSaveRef = useRef<Promise<unknown>>(Promise.resolve());
  const streamBufferRef = useRef("");
  const streamFlushRef = useRef<number | null>(null);
  sessionRef.current = sessionId;
  snapshotRef.current = snapshot;
  pendingUserRef.current = pendingUser;
  activeRef.current = active;

  const persistDraft = useCallback((selectedSession: string, text: string) => {
    // Navigation can flush a draft while its debounce save is in flight.
    const pending = draftSaveRef.current.catch(() => undefined).then(async () => {
      const latest = await workbenchClient.conversationSnapshot(selectedSession);
      if (latest.session.draft === text) return latest;
      const updated = await workbenchClient.updateSession({
        sessionId: selectedSession,
        expectedRevision: latest.session.revision,
        operationId: newOperation("save-draft"),
        draft: text,
      });
      return { ...latest, session: updated.record, sequence: updated.sequence };
    });
    draftSaveRef.current = pending;
    return pending;
  }, []);

  const loadSessions = useCallback(async (selectedWorkspace: string | null, archived = showArchived) => {
    const response = await workbenchClient.listSessions(selectedWorkspace, archived);
    setSessions(response.sessions);
    setSessionId((current) => response.sessions.some((session) => session.id === current) ? current : response.sessions[0]?.id ?? null);
  }, [showArchived]);

  const hydrate = useCallback(async (selectedSession = sessionRef.current) => {
    if (!selectedSession) {
      if (!sessionRef.current) setSnapshot(null);
      return;
    }
    const next = await workbenchClient.conversationSnapshot(selectedSession);
    if (sessionRef.current !== selectedSession) return;
    setSnapshot(next);
    setDraft(next.session.draft);
    const overrides = next.session.overrides;
    setModel(typeof overrides.model === "string" ? overrides.model : "");
    setEffort(typeof overrides.effort === "string" ? overrides.effort : "");
    const inProgress = [...next.turns].reverse().find((turn) => !turn.terminalAt && turn.providerTurnId);
    if (inProgress && next.activeBinding) {
      setActive({ threadId: next.activeBinding.providerThreadId, turnId: inProgress.providerTurnId! });
    } else {
      setActive(null);
    }
  }, []);

  /** Refreshes one conversation's record in the list and, when it is open,
   *  in the snapshot, without touching the in-progress composer draft. */
  const refreshSessionRecord = useCallback(async (id: string) => {
    const latest = await workbenchClient.conversationSnapshot(id);
    setSessions((old) => old.map((session) => session.id === id ? latest.session : session));
    if (sessionRef.current === id) setSnapshot((old) => old && old.session.id === id ? { ...old, session: latest.session, workspace: latest.workspace } : old);
  }, []);

  useEffect(() => {
    let disposed = false;
    // App-owned conversations remain available even when an installed Codex
    // update changes or temporarily breaks the optional live runtime.
    void workbenchClient.listWorkspaces(true)
      .then((listed) => {
        if (!disposed) setWorkspaces(listed.workspaces);
      })
      .catch((cause) => {
        if (!disposed) setError(workbenchErrorMessage(cause));
      });
    void workbenchClient.pendingRequests()
      .then((pending) => {
        if (!disposed) setRequests(pending);
      })
      .catch((cause) => {
        if (!disposed) setError(workbenchErrorMessage(cause));
      });
    if (surface === "chat") void workbenchClient.connectCodex()
      .then(() => workbenchClient.accountState())
      .then(async (account) => {
        if (account.status !== "chatgpt") return;
        const catalog = await workbenchClient.modelCatalog();
        if (!disposed) setModels(catalog.models);
      })
      .catch((cause) => {
        if (!disposed) setError(`Workspace runtime could not connect. Saved conversations remain available: ${workbenchErrorMessage(cause)}`);
      });
    return () => { disposed = true; };
  }, [surface]);

  useEffect(() => { void loadSessions(workspaceId); }, [loadSessions, workspaceId]);

  useEffect(() => {
    if (workspaceId) localStorage.setItem("pipeline.workspace.workspaceId", workspaceId);
    else localStorage.removeItem("pipeline.workspace.workspaceId");
  }, [workspaceId]);

  useEffect(() => {
    if (sessionId) localStorage.setItem("pipeline.workspace.sessionId", sessionId);
    else localStorage.removeItem("pipeline.workspace.sessionId");
  }, [sessionId]);

  // The harness editor is conversation-scoped; leaving the conversation closes it.
  useEffect(() => { setHarnessEditor(false); }, [sessionId, surface]);

  useEffect(() => {
    if (!sessionId) { setSnapshot(null); return; }
    setTranscriptPage(0); setSelectedMessage(null); setJumpTarget(null);
    followLatestRef.current = true;
    if (surface === "project") { void hydrate(sessionId); return; }
    void workbenchClient.reconcileSession(sessionId)
      .catch((cause) => setError(`Showing the local transcript because native history could not be refreshed: ${workbenchErrorMessage(cause)}`))
      .finally(() => void hydrate(sessionId));
  }, [hydrate, sessionId, surface]);

  useEffect(() => {
    const current = snapshot;
    if (!current || current.session.id !== sessionId || pendingUser || active || submitting || contextBusy || current.session.draft === draft) return;
    const selectedSession = current.session.id;
    const text = draft;
    const timer = window.setTimeout(() => {
      void persistDraft(selectedSession, text).then((next) => {
        if (sessionRef.current === selectedSession) setSnapshot(next);
      }).catch((cause) => setError(`Draft could not be saved: ${workbenchErrorMessage(cause)}`));
    }, 350);
    return () => window.clearTimeout(timer);
  }, [active, contextBusy, draft, pendingUser, persistDraft, sessionId, snapshot, submitting]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<WorkbenchEvent>("workbench:event", ({ payload }) => {
      if (disposed) return;
      const boundThread = snapshotRef.current?.activeBinding?.providerThreadId;
      const expectedThread = activeRef.current?.threadId ?? boundThread;
      const matchesThread = (threadId: unknown) => submittingRef.current
        || !expectedThread
        || threadId === expectedThread;
      if (payload.kind === "connectionClosed") {
        const preserved = pendingUserRef.current;
        if (preserved) {
          setDraft(preserved);
          const current = snapshotRef.current;
          if (current) void persistDraft(current.session.id, preserved).catch(() => undefined);
        }
        pendingUserRef.current = null;
        activeRef.current = null;
        submittingRef.current = false;
        setError(`Workspace connection closed. ${preserved ? "The unconfirmed message was restored to the composer. " : ""}${workbenchErrorMessage(payload.reason ?? "")}`);
        setSubmitting(false);
        setActive(null);
        setPendingUser(null);
        setStream("");
        setRequests([]);
      }
      if (payload.kind === "accountUpdated" || payload.kind === "accountLoginCompleted") {
        void workbenchClient.accountState(true).then(async (account) => {
          if (account.status === "chatgpt") setModels((await workbenchClient.modelCatalog()).models);
          else setError("Workspace is no longer signed in to ChatGPT. Your draft and conversation remain local.");
        }).catch((cause) => setError(workbenchErrorMessage(cause)));
      }
      if (payload.kind === "sessionTitleUpdated" && typeof payload.sessionId === "string") {
        void refreshSessionRecord(payload.sessionId).catch(() => undefined);
      }
      if (payload.kind === "sessionTitleFailed" && payload.sessionId === sessionRef.current) {
        setError(`Automatic title unavailable: ${workbenchErrorMessage(payload.error ?? "")}. Rename the conversation from its menu.`);
      }
      if (payload.kind === "serverRequest" && matchesThread(payload.params?.threadId)) {
        setRequests((old) => old.some((item) => item.requestId === payload.requestId) ? old : [...old, payload]);
      }
      if (payload.kind === "serverRequestResolved") setRequests((old) => old.filter((item) => item.requestId !== payload.requestId));
      if (payload.kind === "agentMessageDelta" && matchesThread(payload.threadId)) {
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
            void persistDraft(current.session.id, preserved).then((next) => {
              if (sessionRef.current === current.session.id) setSnapshot(next);
            }).catch(() => undefined);
          }
          setError(`The turn ended with status ${String(payload.status ?? "failed")}. The message was restored to the composer and was not sent to another provider.`);
        }
        if (streamFlushRef.current !== null) window.clearTimeout(streamFlushRef.current);
        streamFlushRef.current = null;
        streamBufferRef.current = "";
        submittingRef.current = false;
        activeRef.current = null;
        pendingUserRef.current = null;
        setSubmitting(false); setActive(null); setPendingUser(null); setStream("");
        if (!restoredFailedDraft) {
          const selectedSession = sessionRef.current;
          if (selectedSession) {
            void workbenchClient.reconcileSession(selectedSession)
              .catch(() => false)
              .finally(() => void hydrate(selectedSession));
          }
        }
      }
    }).then((dispose) => { if (disposed) dispose(); else unlisten = dispose; });
    return () => {
      disposed = true;
      unlisten?.();
      if (streamFlushRef.current !== null) window.clearTimeout(streamFlushRef.current);
      streamFlushRef.current = null;
      streamBufferRef.current = "";
    };
  }, [hydrate, persistDraft, refreshSessionRecord]);

  const saveCurrentDraft = async () => {
    await selectionSaveRef.current;
    if (sessionId && !active && !submittingRef.current) await persistDraft(sessionId, draft);
  };

  const selectProject = async (id: string | null) => {
    if (contextBusyRef.current) return;
    await saveCurrentDraft();
    if (id !== workspaceId) {
      setSnapshot(null); setSessionId(null); setDraft(""); setSessions([]);
      setWorkspaceId(id);
    }
    setSurface("chat");
  };

  const selectSession = async (id: string) => {
    if (contextBusyRef.current) return;
    await saveCurrentDraft();
    if (id !== sessionId) { setSnapshot(null); setDraft(""); }
    setSessionId(id); setSurface("chat");
  };

  const createSession = async () => {
    if (contextBusyRef.current) return;
    await saveCurrentDraft();
    const created = await workbenchClient.createSession({ workspaceId, title: DEFAULT_TITLE, operationId: newOperation("create-session") });
    await loadSessions(workspaceId);
    setSnapshot(null); setDraft(""); setSessionId(created.record.id);
    setSurface("chat");
  };

  const openProject = () => {
    void saveCurrentDraft().then(() => setSurface("project")).catch(cause => setError(workbenchErrorMessage(cause)));
  };

  const projectCreated = async (created: Workspace) => {
    await saveCurrentDraft().catch(() => undefined);
    setWorkspaces((old) => [created, ...old.filter(item => item.id !== created.id)]);
    setWorkspaceId(created.id);
    setSurface("project");
    setProjectDialog(false);
    await loadSessions(created.id);
  };

  const openResearchConversation = useCallback(async (text: string, checkpoint?: string) => {
    if (!workspaceId) return;
    let session: WorkbenchSession;
    if (checkpoint) {
      const { projectClient } = await import("../lib/projectClient");
      session = await projectClient.taskSession(workspaceId, checkpoint);
    } else {
      session = (await workbenchClient.createSession({ workspaceId, title: "Selected passage", operationId: newOperation("selection-chat") })).record;
      session = (await workbenchClient.updateSession({ sessionId: session.id, expectedRevision: session.revision, operationId: newOperation("selection-context"), draft: text, presetId: "research_assistant" })).record;
    }
    await loadSessions(workspaceId);
    setSessionId(session.id); setSurface("chat");
  }, [loadSessions, workspaceId]);
  const updateProjectWorkspace = useCallback((next: Workspace) => setWorkspaces(old => old.map(w => w.id === next.id ? next : w)), []);

  const archiveSession = async (session: WorkbenchSession) => {
    const latest = await workbenchClient.conversationSnapshot(session.id);
    await workbenchClient.updateSession({ sessionId: session.id, expectedRevision: latest.session.revision, operationId: newOperation("archive-session"), archived: !latest.session.archivedAt });
    await loadSessions(workspaceId, showArchived);
    if (sessionRef.current === session.id) await refreshSessionRecord(session.id);
  };

  const renameSession = async (session: WorkbenchSession) => {
    const answer = window.prompt("Conversation title", session.title);
    if (answer === null) return;
    const title = answer.trim();
    if (!title || title === session.title) return;
    const latest = await workbenchClient.conversationSnapshot(session.id);
    await workbenchClient.updateSession({ sessionId: session.id, expectedRevision: latest.session.revision, operationId: newOperation("rename-session"), title });
    await refreshSessionRecord(session.id);
  };

  const generateTitle = async (session: WorkbenchSession) => {
    setError(null);
    await workbenchClient.generateSessionTitle(session.id);
    await refreshSessionRecord(session.id);
  };

  const deleteSession = async (session: WorkbenchSession) => {
    if (!window.confirm(`Delete “${session.title}”? Its transcript and native thread history are removed permanently. Research records it produced stay with their project.`)) return;
    await workbenchClient.deleteSession({ sessionId: session.id, operationId: newOperation("delete-session") });
    if (sessionRef.current === session.id) { setSnapshot(null); setSessionId(null); setDraft(""); }
    await loadSessions(workspaceId, showArchived);
  };

  const moveSession = async (session: WorkbenchSession, target: string | null) => {
    setMoving(true); setMoveError(null);
    try {
      await saveCurrentDraft();
      const latest = await workbenchClient.conversationSnapshot(session.id);
      await workbenchClient.moveSession({ sessionId: session.id, expectedRevision: latest.session.revision, operationId: newOperation("move-session"), workspaceId: target });
      setMoveTarget(null);
      if (sessionRef.current === session.id) {
        // Follow the conversation into its new project instead of losing it
        // behind the sidebar filter.
        if (target !== workspaceId) { setSessions([]); setWorkspaceId(target); }
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
    const path = await save({ defaultPath: `${current.session.title.replace(/[^a-z0-9_-]+/gi, "-") || "conversation"}.md`, filters: [{ name: "Markdown", extensions: ["md"] }] });
    if (path) await workbenchClient.exportConversation(current.session.id, path);
  };

  const saveSelection = (nextModel: string, nextEffort: string) => {
    const selectedSession = sessionRef.current;
    if (!selectedSession) return;
    selectionSaveRef.current = selectionSaveRef.current
      .catch(() => undefined)
      .then(async () => {
        const latest = await workbenchClient.conversationSnapshot(selectedSession);
        const updated = await workbenchClient.updateSession({ sessionId: latest.session.id, expectedRevision: latest.session.revision, operationId: newOperation("model-selection"), overrides: { ...latest.session.overrides, model: nextModel || null, effort: nextEffort || null } });
        if (sessionRef.current === selectedSession) {
          setSnapshot({ ...latest, session: updated.record, sequence: updated.sequence });
        }
      })
      .catch((cause) => setError(`Model selection could not be saved: ${workbenchErrorMessage(cause)}`));
  };

  const send = async () => {
    if (!sessionId || snapshotRef.current?.session.id !== sessionId || !draft.trim() || active || submittingRef.current || contextBusyRef.current) return;
    submittingRef.current = true;
    setSubmitting(true);
    const text = draft.trim();
    pendingUserRef.current = text;
    if (streamFlushRef.current !== null) window.clearTimeout(streamFlushRef.current);
    streamFlushRef.current = null;
    streamBufferRef.current = "";
    followLatestRef.current = true;
    setSelectedMessage(null); setJumpTarget(null);
    setTranscriptPage(0); setError(null); setPendingUser(text); setStream(""); setDraft("");
    try {
      await selectionSaveRef.current;
      await draftSaveRef.current;
      const result = await workbenchClient.sendTurn({ sessionId, text, clientSubmissionId: newOperation("submission"), model: model || null, effort: effort || null });
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
      setError(`The submission may not have been acknowledged. It was preserved as a draft and will not be retried automatically. ${workbenchErrorMessage(cause)}`);
      pendingUserRef.current = null;
      setPendingUser(null);
      await hydrate(sessionId);
    }
  };

  const resolve = async (event: WorkbenchEvent, result?: Record<string, unknown>) => {
    if (event.requestId === undefined || !event.method) return;
    setRequests((old) => old.filter((item) => item.requestId !== event.requestId));
    await workbenchClient.resolveServerRequest({ requestId: event.requestId, method: event.method, result: result ?? null, declineMessage: result ? null : "The user declined this request" }).catch((cause) => setError(workbenchErrorMessage(cause)));
  };

  const allRenderedItems = useMemo(() => snapshot?.items.filter(isMessage) ?? [], [snapshot]);
  const maximumTranscriptPage = Math.max(0, Math.ceil(allRenderedItems.length / TRANSCRIPT_PAGE_SIZE) - 1);
  useEffect(() => {
    setTranscriptPage((page) => Math.min(page, maximumTranscriptPage));
  }, [maximumTranscriptPage]);
  const transcriptEnd = Math.max(0, allRenderedItems.length - transcriptPage * TRANSCRIPT_PAGE_SIZE);
  const transcriptStart = Math.max(0, transcriptEnd - TRANSCRIPT_PAGE_SIZE);
  const renderedItems = useMemo(
    () => allRenderedItems.slice(transcriptStart, transcriptEnd),
    [allRenderedItems, transcriptEnd, transcriptStart],
  );
  const outlineEntries = useMemo(() => allRenderedItems.map(item => ({ id: item.id, text: payloadText(item), role: roleFor(item) as "You" | "ChatGPT" })).filter(item => item.text), [allRenderedItems]);
  const jumpToMessage = (id: string) => {
    const index = allRenderedItems.findIndex(item => item.id === id);
    if (index < 0) return;
    followLatestRef.current = false;
    setTranscriptPage(Math.floor((allRenderedItems.length - 1 - index) / TRANSCRIPT_PAGE_SIZE));
    setSelectedMessage(id); setJumpTarget(id);
  };
  useEffect(() => {
    if (!jumpTarget) return;
    const target = document.getElementById(`workspace-message-${jumpTarget}`);
    if (target) { target.scrollIntoView({ block: "start" }); target.focus({ preventScroll: true }); setJumpTarget(null); }
  }, [jumpTarget, renderedItems]);
  useEffect(() => {
    if (transcriptPage !== 0 || !followLatestRef.current || jumpTarget) return;
    const container = transcriptRef.current;
    if (container) container.scrollTop = container.scrollHeight;
  }, [stream, pendingUser, renderedItems, transcriptPage, jumpTarget]);
  const latest = () => {
    setTranscriptPage(0); setSelectedMessage(null); setJumpTarget(null); followLatestRef.current = true;
    const container = transcriptRef.current;
    if (container) container.scrollTop = container.scrollHeight;
  };
  const visibleRequests = requests.filter((request) =>
    request.params?.threadId === snapshot?.activeBinding?.providerThreadId || request.params?.threadId === active?.threadId,
  );
  const selectedModel = models.find((candidate) => candidate.model === model);
  const currentWorkspace = workspaces.find(item => item.id === workspaceId) ?? null;
  const projectFolder = currentWorkspace?.root ? currentWorkspace.root.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || currentWorkspace.root : null;

  return <div className="workspace-chat-shell flex h-full min-h-0 bg-white dark:bg-neutral-950">
    <aside className="flex w-64 shrink-0 flex-col border-r border-gray-200 bg-gray-50 dark:border-neutral-800 dark:bg-neutral-900">
      <div className="border-b border-gray-200 p-3 dark:border-neutral-800">
        <div className="flex gap-2">
          <select disabled={contextBusy} aria-label="Project" value={workspaceId ?? ""} onChange={(event) => { void selectProject(event.target.value || null).catch(cause => setError(workbenchErrorMessage(cause))); }} className="min-w-0 flex-1 rounded-lg border bg-white px-2 py-1.5 text-sm dark:bg-neutral-950"><option value="">Unfiled conversations</option>{workspaces.filter((item) => !item.archivedAt).map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select>
          <button type="button" title="New project" aria-label="New project" disabled={contextBusy} onClick={() => setProjectDialog(true)} className="rounded-lg border px-2">＋</button>
          <button type="button" title="Connection settings" aria-label="Workspace connection settings" onClick={onOpenSettings} className="rounded-lg border px-2">⚙</button>
        </div>
        {currentWorkspace
          ? <div className="mt-3 rounded-lg border border-gray-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-950">
            <p className="truncate text-sm font-semibold" title={currentWorkspace.name}>{currentWorkspace.name}</p>
            <p className="truncate text-xs text-gray-500 dark:text-neutral-400" title={currentWorkspace.root ?? undefined}>{currentWorkspace.missingRootAt ? "Folder not found" : projectFolder ?? "No folder attached"}</p>
            <button type="button" disabled={contextBusy} aria-current={surface === "project" ? "page" : undefined} onClick={openProject} className={`mt-2 w-full rounded-md border px-3 py-1.5 text-xs ${surface === "project" ? "border-gray-900 bg-gray-900 text-white dark:border-neutral-100 dark:bg-neutral-100 dark:text-neutral-900" : "border-gray-300 hover:bg-gray-50 dark:border-neutral-700 dark:hover:bg-neutral-800"}`}>{surface === "project" ? "Project open" : "Open project"}</button>
          </div>
          : <p className="mt-3 text-xs text-gray-500 dark:text-neutral-400">These conversations belong to no project. Create a project to keep a paper, its files, and notes together.</p>}
        <button type="button" disabled={contextBusy} onClick={() => void createSession().catch(cause => setError(workbenchErrorMessage(cause)))} className="mt-3 w-full rounded-lg bg-gray-900 px-3 py-2 text-sm font-medium text-white dark:bg-neutral-100 dark:text-neutral-900">New conversation</button>
      </div>
      {surface === "project" && (active || submitting || visibleRequests.length > 0) && <div role="status" className="m-2 space-y-2 rounded border p-3 text-xs"><p>{visibleRequests.length ? "A conversation needs your response." : "A conversation is working."}</p><button type="button" className="underline" onClick={() => setSurface("chat")}>Open conversation</button>{active && <button type="button" className="ml-3 text-red-600 underline" onClick={() => void workbenchClient.interruptTurn(active.threadId, active.turnId)}>Stop</button>}</div>}
      <p className="px-4 pt-3 text-[11px] font-semibold uppercase tracking-wide text-gray-400 dark:text-neutral-500">Conversations</p>
      <div className="min-h-0 flex-1 overflow-auto p-2">{!sessions.length && <p className="px-2 py-3 text-xs text-gray-500 dark:text-neutral-400">No conversations yet.</p>}{sessions.map((session) => <div key={session.id} className={`mb-1 flex items-center rounded-lg pr-1 ${session.id === sessionId ? "bg-white shadow-sm dark:bg-neutral-800" : "text-gray-600 hover:bg-white/70 dark:text-neutral-400"}`}>
        <button type="button" disabled={contextBusy} onClick={() => { void selectSession(session.id).catch(cause => setError(workbenchErrorMessage(cause))); }} className="min-w-0 flex-1 px-3 py-2 text-left text-sm"><span className="block truncate">{session.title}</span>{session.archivedAt && <span className="text-xs text-gray-400">Archived</span>}</button>
        <ConversationMenu session={session} disabled={contextBusy} onRename={() => void renameSession(session).catch(cause => setError(workbenchErrorMessage(cause)))} onGenerateTitle={() => void generateTitle(session).catch(cause => setError(`Title could not be generated: ${workbenchErrorMessage(cause)}`))} onMove={() => { setMoveError(null); setMoveTarget(session); }} onArchive={() => void archiveSession(session).catch(cause => setError(workbenchErrorMessage(cause)))} onDelete={() => void deleteSession(session).catch(cause => setError(workbenchErrorMessage(cause)))} />
      </div>)}</div>
      <label className="border-t border-gray-200 p-3 text-xs text-gray-500 dark:border-neutral-800"><input type="checkbox" disabled={contextBusy} checked={showArchived} onChange={(event) => { setShowArchived(event.target.checked); void loadSessions(workspaceId, event.target.checked); }} className="mr-2" />Show archived</label>
    </aside>
    {projectDialog && <Suspense fallback={null}><WorkspaceProjectDialog onClose={() => setProjectDialog(false)} onCreated={projectCreated}/></Suspense>}
    {moveTarget && <MoveConversationDialog key={moveTarget.id} session={moveTarget} workspaces={workspaces} busy={moving} error={moveError} onMove={(target) => void moveSession(moveTarget, target)} onClose={() => { if (!moving) setMoveTarget(null); }} />}
    {surface === "project" && workspaceId && <Suspense fallback={<section className="flex-1 p-8 text-sm text-gray-500">Loading project…</section>}><WorkspaceProjectSurface key={workspaceId} workspaceId={workspaceId} onConversation={openResearchConversation} onWorkspaceChanged={updateProjectWorkspace} onReviewHandoff={onReviewHandoff}/></Suspense>}
    {surface === "chat" && snapshot && harnessEditor && <Suspense fallback={<section className="flex-1 p-8 text-sm text-gray-500">Loading harness editor…</section>}><WorkspaceHarnessEditor key={snapshot.session.id} snapshot={snapshot} onSnapshot={(next) => { setSnapshot(next); setDraft(next.session.draft); }} onClose={() => { setHarnessEditor(false); setShowResearch(true); }} /></Suspense>}
    <section className={(surface === "project" && workspaceId) || harnessEditor ? "hidden" : "relative flex min-w-0 flex-1 flex-col"}>
      <header className="workspace-chat-header">
        <div className="workspace-chat-heading"><h1 className="truncate text-sm font-semibold">{snapshot?.session.title ?? "Workspace"}</h1><p className="text-xs text-gray-500">{active || submitting ? "ChatGPT is working…" : snapshot?.activeBinding ? "Ready to resume" : "New standalone conversation"}</p></div>
        <div className="workspace-chat-toolbar">
        <select disabled={contextBusy || Boolean(active) || submitting} aria-label="Model" value={model} onChange={(event) => { const next = event.target.value; setModel(next); const defaultEffort = models.find((item) => item.model === next)?.defaultReasoningEffort ?? ""; setEffort(defaultEffort); void saveSelection(next, defaultEffort); }} className="rounded border bg-transparent px-2 py-1 text-xs"><option value="">Automatic model</option>{models.map((item) => <option key={item.id} value={item.model}>{item.displayName}</option>)}</select>
        {model && <select disabled={contextBusy || Boolean(active) || submitting} aria-label="Reasoning effort" value={effort} onChange={(event) => { setEffort(event.target.value); void saveSelection(model, event.target.value); }} className="rounded border bg-transparent px-2 py-1 text-xs">{selectedModel?.supportedReasoningEfforts.map((item) => <option key={item.reasoningEffort} value={item.reasoningEffort}>{item.reasoningEffort}</option>)}</select>}
        <span className="workspace-chat-divider" aria-hidden="true" />
        {snapshot && <button ref={outlineButtonRef} type="button" aria-expanded={showOutline} onClick={() => { setShowOutline(value => !value); setShowResearch(false); }} className={`rounded border px-2 py-1 text-xs ${showOutline ? "bg-gray-100 dark:bg-neutral-800" : ""}`}><WorkspaceIcon name="outline" />Outline</button>}
        {snapshot && <button disabled={contextBusy} type="button" onClick={() => { setShowOutline(false); setShowResearch((value) => !value); }} className={`rounded border px-2 py-1 text-xs ${showResearch ? "bg-gray-100 dark:bg-neutral-800" : ""}`}><WorkspaceIcon name="research" />Research</button>}
        {snapshot && <button type="button" onClick={() => void exportCurrent()} className="rounded border px-2 py-1 text-xs"title="Export conversation"><WorkspaceIcon name="export" /><span className="sr-only">Export</span></button>}
        {snapshot && <button type="button" disabled={contextBusy} onClick={() => void archiveSession(snapshot.session).catch(cause => setError(workbenchErrorMessage(cause)))} className="rounded border px-2 py-1 text-xs"title={snapshot.session.archivedAt ? "Restore conversation" : "Archive conversation"}><WorkspaceIcon name="archive" /><span className="sr-only">{snapshot.session.archivedAt ? "Restore" : "Archive"}</span></button>}
        </div>
      </header>
      <div ref={transcriptRef} onScroll={event => { const node = event.currentTarget; followLatestRef.current = node.scrollHeight - node.scrollTop - node.clientHeight < 80; }} className="workspace-transcript min-h-0 flex-1 overflow-auto"><div className="workspace-transcript-content">
        {(!snapshot || (!allRenderedItems.length && !pendingUser && !stream)) && <div className="workspace-chat-empty"><WorkspaceIcon name="message" size={30} style={{ margin: "0 auto" }} /><h2>{snapshot ? "What are you working on?" : "A place to think things through"}</h2><p>{snapshot ? "Ask a question, explore an idea, or add files to work from." : "Start a conversation to explore an idea or work through your research."}</p></div>}
        {transcriptStart > 0 && <button type="button" onClick={() => { followLatestRef.current = false; setTranscriptPage((value) => value + 1); }} className="mx-auto block rounded border px-3 py-2 text-xs text-gray-600">Show 200 earlier messages</button>}
        {renderedItems.map(item => {
          const text = payloadText(item);
          const user = roleFor(item) === "You";
          return text ? <article key={item.id} id={`workspace-message-${item.id}`} tabIndex={-1} aria-label={`${roleFor(item)} message`} className={`workspace-message workspace-message--${user ? "user" : "assistant"}${selectedMessage === item.id ? " ring-2 ring-blue-300 ring-offset-4 dark:ring-offset-neutral-950" : ""}`}>
            <div className="workspace-message-author">{!user && <span className="workspace-assistant-mark"><WorkspaceIcon name="message" size={14} /></span>}{roleFor(item)}</div>
            <div className="workspace-message-body"><ReactMarkdown remarkPlugins={[remarkMath]} rehypePlugins={[rehypeKatex]} components={{ a: SafeMarkdownLink }}>{text}</ReactMarkdown></div>
            <WorkspaceMessageActions text={text} label={user ? "prompt" : "response"} />
          </article> : null;
        })}
        {pendingUser && <article className="workspace-message workspace-message--user"><div className="workspace-message-author">You</div><div className="workspace-message-body whitespace-pre-wrap">{pendingUser}</div></article>}
        {stream && <article className="workspace-message workspace-message--assistant"><div className="workspace-message-author"><span className="workspace-assistant-mark"><WorkspaceIcon name="message" size={14} /></span>ChatGPT<span className="ml-1 h-1.5 w-1.5 rounded-full bg-current motion-safe:animate-pulse" /></div><div className="workspace-message-body"><ReactMarkdown remarkPlugins={[remarkMath]} rehypePlugins={[rehypeKatex]} components={{ a: SafeMarkdownLink }}>{stream}</ReactMarkdown></div></article>}
        {visibleRequests.map((request) => <RequestCard key={String(request.requestId)} event={request} onResolve={(event, result) => void resolve(event, result)} />)}
        {error && <div role="alert" className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300">{error}</div>}
        {transcriptEnd < allRenderedItems.length && <button type="button" onClick={() => setTranscriptPage((value) => Math.max(0, value - 1))} className="mx-auto block rounded border px-3 py-2 text-xs text-gray-600">Show 200 newer messages</button>}
      </div></div>
      <div className="workspace-composer-dock"><div className="mx-auto max-w-3xl">
        <div className="workspace-composer">
          {snapshot?.session.paperId && <button type="button" disabled={contextBusy} onClick={() => { setShowResearch(true); setShowOutline(false); }} className="mb-1 ml-2 rounded-lg bg-blue-50 px-2 py-1 text-xs text-blue-700 dark:bg-blue-950/30 dark:text-blue-300">Document selected · Research</button>}
          <textarea ref={messageRef} aria-label="Message" value={draft} disabled={!sessionId || snapshot?.session.id !== sessionId || submitting || Boolean(active) || contextBusy} onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); void send(); } }} placeholder="Ask ChatGPT…" rows={3} className="max-h-52 min-h-20 w-full resize-y border-0 bg-transparent p-2 text-sm outline-none disabled:opacity-50" />
          <div className="flex items-center justify-between gap-2">
            <div className="flex min-w-0 items-center gap-2">
              <WorkspaceComposerMenu key={sessionId ?? "empty"} snapshot={snapshot} workspaces={workspaces} disabled={submitting || Boolean(active) || contextBusy} onProject={selectProject} onCreateProject={() => setProjectDialog(true)} onOpenProject={workspaceId ? openProject : undefined} onSnapshot={next => { if (sessionRef.current === next.session.id) setSnapshot(next); }} onBusy={value => { contextBusyRef.current = value; setContextBusy(value); if (value) setShowResearch(false); }} onResearch={() => { setShowResearch(true); setShowOutline(false); }} onDictation={() => messageRef.current?.focus()} />
              <span className="truncate text-xs text-gray-500">{contextBusy ? "Updating files…" : snapshot?.workspace?.name ?? "Unfiled"}</span>
            </div>
            <div className="flex items-center gap-2"><button type="button" onClick={latest} className="rounded-lg px-2 py-1 text-xs text-gray-500 hover:bg-gray-100 dark:hover:bg-neutral-800">↓ Latest</button>{active ? <button type="button" onClick={() => void workbenchClient.interruptTurn(active.threadId, active.turnId)} className="rounded-xl border border-red-300 px-4 py-2 text-sm text-red-600">Stop</button> : <button type="button" disabled={!sessionId || snapshot?.session.id !== sessionId || !draft.trim() || submitting || contextBusy} onClick={() => void send()} className="workspace-send-button" aria-label={submitting ? "Sending…" : "Send ↑"}>{submitting ? "Sending…" : <>Send<WorkspaceIcon name="arrow-up" size={17} /></>}</button>}</div>
          </div>
        </div>
        <p className="mt-2 text-center text-[11px] text-gray-400">Enter to send · Shift+Enter for a new line</p>
      </div></div>
    </section>
    {surface === "chat" && !harnessEditor && showOutline && <WorkspaceConversationOutline key={sessionId} entries={outlineEntries} selectedId={selectedMessage} onJump={jumpToMessage} onClose={() => { setShowOutline(false); outlineButtonRef.current?.focus(); }} />}
    {surface === "chat" && !harnessEditor && snapshot && showResearch && <Suspense fallback={<aside className="w-[28rem] border-l border-gray-200 p-5 text-sm text-gray-500 dark:border-neutral-800">Loading research tools…</aside>}><WorkspaceResearchPanel snapshot={snapshot} onSnapshot={(next) => { setSnapshot(next); setDraft(next.session.draft); }} onClose={() => setShowResearch(false)} onError={setError} onReviewHandoff={onReviewHandoff} onEditHarness={() => { setShowOutline(false); setShowResearch(false); setHarnessEditor(true); }} onMove={() => { setMoveError(null); setMoveTarget(snapshot.session); }} /></Suspense>}
  </div>;
}

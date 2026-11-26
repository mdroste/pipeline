import { createSubmissionIntents } from "./submissionIntents";
import {
  conversationProvider,
  type ConversationProvider,
} from "../../lib/workspaceProviders";
import {
  useCallback,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { conversationClient } from "../../lib/workbench/conversationClient";
import { runtimeClient } from "../../lib/workbench/runtimeClient";
import {
  keepNewerSession,
  mergeSessionSnapshot,
} from "../../lib/workspaceSessionState";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { createStreamStore } from "./streamStore";
import {
  transcriptHasPrompt,
  unsavedTurnText,
  type TurnText,
} from "./turnText";
import { useActiveTurn } from "./useActiveTurn";
import type {
  ActiveTurn,
  WorkbenchEvent,
  WorkspaceModel,
} from "../../lib/workbench/runtimeTypes";
import type {
  ConversationSnapshot,
  SessionSnapshot,
  WorkbenchSession,
} from "../../lib/workbench/sessionTypes";
import type {
  PendingSubmission,
  SubmissionOwner,
  TurnEnd,
  FileChanges,
} from "./conversationRuntimeTypes";

const readActiveTurn = () => runtimeClient.activeTurn();

/** One identity owner for hydration, submission acknowledgement and terminal events. */
function useConversationState(
  sessionId: string | null,
  setSessions: Dispatch<SetStateAction<WorkbenchSession[]>>,
) {
  const [snapshot, setSnapshot] = useState<ConversationSnapshot | null>(null);
  const [models, setModels] = useState<WorkspaceModel[]>([]);
  const [model, setModel] = useState("");
  const [runtime, setRuntime] = useState<ConversationProvider>("codex");
  const [effort, setEffort] = useState("");
  const [draft, setDraft] = useState("");
  const [streamStore] = useState(createStreamStore);
  const setStream = streamStore.set;
  const [pendingUser, setPendingUser] = useState<string | null>(null);
  const [active, setActive] = useState<{
    threadId: string;
    turnId: string;
  } | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [requests, setRequests] = useState<WorkbenchEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  // Warnings about the conversation on screen. They outlast the response that
  // raised them, and a stopped response's notice must not replace them.
  const [warnings, setWarnings] = useState<string[]>([]);
  // Live file-change items, so an approval can show the files it would change.
  // State, because a request can reach the page before the item it refers to.
  const [fileChanges, setFileChanges] = useState<FileChanges>(() => new Map());
  // Mirrors submissionRef for rendering: a message may still be running in
  // a conversation the researcher has since left.
  const [submissionOwner, setSubmissionOwner] =
    useState<SubmissionOwner | null>(null);
  const { activeTurn, refreshActiveTurn, observeActiveTurn } =
    useActiveTurn(readActiveTurn);
  const sessionRef = useRef(sessionId);
  const snapshotRef = useRef(snapshot);
  const submissionRef = useRef<PendingSubmission | null>(null);
  const activeRef = useRef(active);
  const submittingRef = useRef(false);
  const completedTurnsRef = useRef(new Map<string, TurnEnd>());
  // What went wrong in a conversation while another was on screen, shown when
  // the researcher returns to it.
  const missedProblemsRef = useRef(new Map<string, string>());
  const streamBufferRef = useRef("");
  const streamFlushRef = useRef<number | null>(null);
  const streamItemRef = useRef<string | null>(null);
  const turnTextRef = useRef<TurnText | null>(null);
  sessionRef.current = sessionId;
  snapshotRef.current = snapshot;
  activeRef.current = active;

  const draftEditRevision = useRef(0);
  const hydrationGeneration = useRef(0);
  // A conversation whose saved draft must reach the composer on the next load
  // that lands, whichever caller's it is.
  const draftOwedRef = useRef<string | null>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const clearStream = useCallback(() => {
    if (streamFlushRef.current !== null)
      window.clearTimeout(streamFlushRef.current);
    streamFlushRef.current = null;
    streamBufferRef.current = "";
    streamItemRef.current = null;
    setStream("");
  }, []);
  const hydrate = useCallback(
    async function load(
      selectedSession = sessionRef.current,
      reloadDraft = false,
    ): Promise<void> {
      if (!selectedSession) {
        if (!sessionRef.current) setSnapshot(null);
        return;
      }
      // A late hydration for a conversation the user has left must not
      // invalidate the load of the conversation now on screen.
      if (selectedSession !== sessionRef.current) return;
      if (reloadDraft) draftOwedRef.current = selectedSession;
      const generation = ++hydrationGeneration.current;
      const activeAtStart = activeRef.current;
      const ownerAtStart = submissionRef.current;
      const editRevision = draftEditRevision.current;
      const initialize = snapshotRef.current?.session.id !== selectedSession;
      let next: ConversationSnapshot;
      let hostTurn: ActiveTurn | null | undefined;
      try {
        [next, hostTurn] = await Promise.all([
          conversationClient.conversationSnapshot(selectedSession),
          refreshActiveTurn(),
        ]);
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
        generation !== hydrationGeneration.current
      )
        return;
      if (
        activeRef.current !== activeAtStart ||
        submissionRef.current !== ownerAtStart
      ) {
        // That change may belong to the conversation just left. Nothing else
        // retries, so a conversation still waiting for its first load reads
        // again rather than staying blank.
        if (snapshotRef.current?.session.id !== selectedSession)
          await load(selectedSession);
        return;
      }
      next = keepNewerSession(snapshotRef.current, next);
      const loaded = next;
      setSnapshot((old) => keepNewerSession(old, loaded));
      let recoveredDraft: string | null = null;
      try {
        recoveredDraft = localStorage.getItem(
          `pipeline.pendingDraft.${selectedSession}`,
        );
      } catch {
        /* Optional recovery cache. */
      }
      const turnHere =
        hostTurn?.sessionId === selectedSession ? hostTurn : null;
      // A send whose page closed before its acknowledgement. Its text is still
      // the saved draft, and offering that again would let it be sent twice.
      if (
        !submissionRef.current &&
        turnHere?.composer &&
        turnHere.setup &&
        !turnHere.turnId
      ) {
        submissionRef.current = {
          sessionId: selectedSession,
          text: (recoveredDraft ?? next.session.draft).trim(),
          adopted: true,
        };
        submittingRef.current = true;
        setSubmissionOwner({ sessionId: selectedSession, threadId: null });
      }
      const owner = submissionRef.current;
      const pendingHere = owner?.sessionId === selectedSession;
      setPendingUser(
        pendingHere && !transcriptHasPrompt(next, owner) ? owner.text : null,
      );
      setSubmitting(pendingHere && !owner.turnId);
      const inProgress = [...next.turns]
        .reverse()
        .find(
          (turn) =>
            !turn.terminalAt &&
            turn.providerTurnId &&
            turn.bindingId === next.activeBinding?.id,
        );
      if (!activeRef.current || (!pendingHere && !inProgress)) clearStream();
      // An acknowledged send's text is in the thread but can stay the saved
      // draft for a moment. Offering it again would let it be sent twice.
      const sentHere = Boolean(turnHere?.composer && turnHere.turnId);
      if (
        (initialize || draftOwedRef.current === selectedSession) &&
        draftEditRevision.current === editRevision
      ) {
        draftOwedRef.current = null;
        setDraft(
          pendingHere || sentHere ? "" : (recoveredDraft ?? next.session.draft),
        );
      }
      const overrides = next.session.overrides;
      setRuntime(conversationProvider(overrides.runtime));
      setModel(typeof overrides.model === "string" ? overrides.model : "");
      setEffort(typeof overrides.effort === "string" ? overrides.effort : "");
      let running: { threadId: string; turnId: string } | null = null;
      if (pendingHere && owner.threadId && owner.turnId) {
        running = { threadId: owner.threadId, turnId: owner.turnId };
      } else if (inProgress && next.activeBinding && !turnHere?.automated) {
        running = {
          threadId: next.activeBinding.providerThreadId,
          turnId: inProgress.providerTurnId!,
        };
      } else if (turnHere?.threadId && turnHere.turnId && !turnHere.automated) {
        running = { threadId: turnHere.threadId, turnId: turnHere.turnId };
      }
      setActive(running);
      // The answer so far, including what arrived while this conversation was
      // not on screen. Only the part the transcript doesn't show yet stays live.
      const said = turnTextRef.current;
      if (running && said?.turnId === running.turnId) {
        const { text, itemId } = unsavedTurnText(said, next.items);
        clearStream();
        streamItemRef.current = itemId;
        setStream(text);
      }
    },
    [clearStream, refreshActiveTurn, setStream],
  );

  // Refresh records without replacing the composer draft.
  const refreshSessionRecord = useCallback(async (id: string) => {
    const latest = await conversationClient.sessionSnapshot(id);
    setSessions((old) =>
      old.map((session) => (session.id === id ? latest.session : session)),
    );
    if (sessionRef.current === id)
      setSnapshot((old) => mergeSessionSnapshot(old, latest));
  }, []);

  // A session save carries no transcript, so it never replaces the one on screen.
  const acceptSnapshot = useCallback((next: SessionSnapshot) => {
    if (sessionRef.current !== next.session.id) return;
    setSnapshot((old) =>
      "items" in next
        ? (next as ConversationSnapshot)
        : mergeSessionSnapshot(old, next),
    );
  }, []);
  const reconcileSelection = useCallback(
    (onSelected: () => void) => {
      // Errors speak for the conversation that was on screen, and warnings too.
      // One this conversation raised while it was left waits here.
      const missed = sessionId
        ? missedProblemsRef.current.get(sessionId)
        : null;
      if (sessionId) missedProblemsRef.current.delete(sessionId);
      setError(missed ?? null);
      setWarnings([]);
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
      onSelected();
      setNotice(null);
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
    },
    [hydrate, sessionId],
  );
  const navigateTo = (selected: string | null) => {
    hydrationGeneration.current++;
    setSnapshot(null);
    snapshotRef.current = null;
    setDraft("");
    sessionRef.current = selected;
  };
  const state = {
    reconcileSelection,
    navigateTo,
    sessionId,
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
    setStream,
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
    warnings,
    setWarnings,
    submissionOwner,
    setSubmissionOwner,
    activeTurn,
    refreshActiveTurn,
    observeActiveTurn,
    sessionRef,
    snapshotRef,
    submissionRef,
    activeRef,
    submittingRef,
    completedTurnsRef,
    missedProblemsRef,
    streamBufferRef,
    streamFlushRef,
    streamItemRef,
    turnTextRef,
    fileChanges,
    setFileChanges,
    draftEditRevision,
    hydrationGeneration,
    draftOwedRef,
    draftRef,
    clearStream,
    hydrate,
    refreshSessionRecord,
    acceptSnapshot,
  };
  return state;
}
export type WorkspaceConversationState = ReturnType<
  typeof useConversationState
>;
export function useWorkspaceConversationRuntime(
  sessionId: string | null,
  setSessions: Dispatch<SetStateAction<WorkbenchSession[]>>,
) {
  const state = useConversationState(sessionId, setSessions);
  return { ...state, submission: createSubmissionIntents(state) };
}
export type WorkspaceConversationRuntime = ReturnType<
  typeof useWorkspaceConversationRuntime
>;

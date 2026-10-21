import { useEffect, useRef, useState, type RefObject } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { ConversationSnapshot } from "../lib/workbenchTypes";

export function useConversationActions({
  sessionId,
  sessionRef,
  snapshotRef,
  activeRef,
  hydrate,
}: {
  sessionId: string | null;
  sessionRef: RefObject<string | null>;
  snapshotRef: RefObject<ConversationSnapshot | null>;
  activeRef: RefObject<{ threadId: string; turnId: string } | null>;
  hydrate: (sessionId: string) => Promise<void>;
}) {
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<string | null>(null);
  const [pendingStop, setPendingStop] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const stops = useRef(new Set<string>());
  const exportPending = useRef(false);
  const key = (id: string, turn: { threadId: string; turnId: string }) =>
    JSON.stringify([id, turn.threadId, turn.turnId]);
  useEffect(() => {
    setActionError(null);
    setActionNotice(null);
  }, [sessionId]);

  const stopCurrent = async () => {
    const id = sessionRef.current;
    const turn = activeRef.current;
    if (!id || !turn) return;
    const request = key(id, turn);
    if (stops.current.has(request)) return;
    stops.current.add(request);
    setPendingStop(request);
    setActionError(null);
    setActionNotice(null);
    try {
      await workbenchClient.interruptTurn(turn.threadId, turn.turnId);
      // The terminal event owns cancellation state; an acknowledgement is not
      // evidence that the model has already stopped.
    } catch (cause) {
      let message = `Could not stop the response: ${workbenchErrorMessage(cause)}. The response may still be running. Try Stop again.`;
      if (sessionRef.current === id) setActionError(message);
      try {
        await workbenchClient.reconcileSession(id);
        if (sessionRef.current === id) await hydrate(id);
      } catch (refreshError) {
        message += ` Could not refresh its status: ${workbenchErrorMessage(refreshError)}`;
      }
      if (sessionRef.current === id) setActionError(message);
    } finally {
      stops.current.delete(request);
      setPendingStop((current) => (current === request ? null : current));
    }
  };

  const exportCurrent = async () => {
    const current = snapshotRef.current;
    if (
      !current ||
      current.session.id !== sessionRef.current ||
      exportPending.current
    )
      return;
    exportPending.current = true;
    setExporting(true);
    setActionError(null);
    setActionNotice(null);
    try {
      const path = await save({
        defaultPath: `${current.session.title.replace(/[^a-z0-9_-]+/gi, "-") || "conversation"}.md`,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (!path) return;
      await workbenchClient.exportConversation(current.session.id, path);
      if (sessionRef.current === current.session.id)
        setActionNotice(`Conversation exported to ${path}`);
    } catch (cause) {
      if (sessionRef.current === current.session.id)
        setActionError(
          `Could not export the conversation: ${workbenchErrorMessage(cause)}. Try Export conversation again.`,
        );
    } finally {
      exportPending.current = false;
      setExporting(false);
    }
  };

  return {
    stopCurrent,
    exportCurrent,
    exporting,
    actionError,
    actionNotice,
    stopping: Boolean(
      sessionId &&
      activeRef.current &&
      pendingStop === key(sessionId, activeRef.current),
    ),
  };
}

import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { appEvents } from "../lib/appEvents";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { WorkbenchEvent } from "../lib/workbenchTypes";
import {
  conversationClient,
  type ConversationIndexRow,
  type IndexCursor,
} from "../lib/workbench/conversationClient";

export type IndexedConversation = ConversationIndexRow;
const inflight = new Map<string, ReturnType<typeof conversationClient.index>>();
const CHANGES_THE_LIST = new Set(["sessionTitleUpdated", "turnCompleted"]);

export default function useConversationIndex(
  enabled = true,
  archived = false,
  query = "",
  unfiled = false,
  palette = false,
) {
  const [conversations, setConversations] = useState<IndexedConversation[]>([]);
  const [loading, setLoading] = useState(enabled);
  const [error, setError] = useState<string | null>(null);
  const [settledQuery, setSettledQuery] = useState(query);
  useEffect(() => {
    if (!query) {
      setSettledQuery("");
      return;
    }
    const timer = setTimeout(() => setSettledQuery(query), 100);
    return () => clearTimeout(timer);
  }, [query]);
  const [request, setRequest] = useState(0);
  const next = useRef<IndexCursor | null>(null);
  const generation = useRef(0);
  const reading = useRef(false);
  const [more, setMore] = useState(false);
  const read = useCallback(
    async (cursor: IndexCursor | null, version: number) => {
      reading.current = true;
      setLoading(true);
      try {
        const key = JSON.stringify([
          archived,
          unfiled,
          settledQuery,
          palette,
          cursor,
        ]);
        let pending = inflight.get(key);
        if (!pending) {
          const load = conversationClient.index(
            archived,
            unfiled,
            settledQuery,
            palette,
            cursor,
          );
          pending = load.finally(() => {
            if (inflight.get(key) === pending) inflight.delete(key);
          });
          inflight.set(key, pending);
        }
        const result = await pending;
        if (generation.current !== version) return;
        setConversations((previous) =>
          cursor ? [...previous, ...result.items] : result.items,
        );
        next.current = result.nextCursor;
        setMore(Boolean(next.current));
        setError(null);
      } catch (cause) {
        if (generation.current === version)
          setError(workbenchErrorMessage(cause));
      } finally {
        if (generation.current === version) {
          reading.current = false;
          setLoading(false);
        }
      }
    },
    [archived, unfiled, settledQuery, palette],
  );
  useEffect(() => {
    if (!enabled) return;
    next.current = null;
    setMore(false);
    void read(null, ++generation.current);
    return () => {
      generation.current++;
    };
  }, [enabled, read, request]);
  const loadMore = () => {
    if (next.current && !reading.current)
      void read(next.current, generation.current);
  };
  const refresh = useCallback(() => {
    inflight.clear();
    setRequest((value) => value + 1);
  }, []);
  useEffect(() => {
    if (!enabled) return;
    const offChanged = appEvents.on("conversations-changed", refresh);
    const offHost = listen<WorkbenchEvent>("workbench:event", ({ payload }) => {
      if (CHANGES_THE_LIST.has(payload.kind)) refresh();
    }).catch(() => null);
    return () => {
      offChanged();
      void offHost.then((stop) => stop?.());
    };
  }, [enabled, refresh]);
  return { conversations, loading, error, refresh, more, loadMore };
}

import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import type { ConversationSnapshot } from "../lib/workbenchTypes";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import { isMessage } from "../components/WorkspaceConversationView";

export default function useWorkspaceTranscript(
  snapshot: ConversationSnapshot | null,
  setSnapshot: Dispatch<SetStateAction<ConversationSnapshot | null>>,
  setError: (error: string) => void,
) {
  const [transcriptPage, setTranscriptPage] = useState(0);
  const [loadingEarlier, setLoadingEarlier] = useState(false);
  const loading = useRef(false);
  const current = useRef(snapshot);
  current.current = snapshot;
  const allRenderedItems = useMemo(
    () => snapshot?.items.filter(isMessage) ?? [],
    [snapshot],
  );
  const maximumPage = Math.max(0, Math.ceil(allRenderedItems.length / 200) - 1);
  useEffect(
    () => setTranscriptPage((page) => Math.min(page, maximumPage)),
    [maximumPage],
  );
  const transcriptEnd = Math.max(
    0,
    allRenderedItems.length - transcriptPage * 200,
  );
  const transcriptStart = Math.max(0, transcriptEnd - 200);
  const renderedItems = useMemo(
    () => allRenderedItems.slice(transcriptStart, transcriptEnd),
    [allRenderedItems, transcriptStart, transcriptEnd],
  );
  const loadEarlier = async () => {
    if (transcriptStart > 0) {
      setTranscriptPage((page) => page + 1);
      return;
    }
    const before = snapshot?.olderCursor;
    if (!snapshot || !before || loading.current) return;
    const session = snapshot.session.id;
    loading.current = true;
    setLoadingEarlier(true);
    try {
      const page = await workbenchClient.transcriptPage(session, before);
      // Refresh/navigation invalidates the page request even within the same session.
      if (
        current.current?.session.id !== session ||
        current.current.olderCursor !== before
      )
        return;
      setSnapshot((old) => {
        if (!old || old.session.id !== session || old.olderCursor !== before)
          return old;
        const ids = new Set(old.items.map((item) => item.id));
        return {
          ...old,
          items: [
            ...page.items.filter((item) => !ids.has(item.id)),
            ...old.items,
          ],
          olderCursor: page.nextCursor,
        };
      });
      // Fill a short window before advancing, so a tool-heavy page cannot
      // shift an unseen boundary message past the next displayed window.
      if (renderedItems.length === 200) setTranscriptPage((value) => value + 1);
    } catch (cause) {
      if (current.current?.session.id === session)
        setError(workbenchErrorMessage(cause));
    } finally {
      loading.current = false;
      setLoadingEarlier(false);
    }
  };
  return {
    allRenderedItems,
    renderedItems,
    transcriptStart,
    transcriptEnd,
    transcriptPage,
    setTranscriptPage,
    loadEarlier,
    loadingEarlier,
  };
}

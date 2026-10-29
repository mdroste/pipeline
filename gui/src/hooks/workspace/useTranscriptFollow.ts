import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MutableRefObject,
  type PointerEvent,
  type RefObject,
  type UIEvent,
  type WheelEvent,
} from "react";

/** Distance from the end that still counts as reading the latest message. */
const LATEST_THRESHOLD_PX = 4;
const READING_KEYS = new Set(["PageUp", "ArrowUp", "Home"]);

interface PagingAnchor {
  direction: "earlier" | "newer";
  height: number;
  top: number;
  lastId: string | undefined;
  start: number;
  end: number;
  total: number;
}

/**
 * One follow model for the conversation transcript. Following stops on a
 * reading gesture (wheel or keys upward, a scrollbar drag, any upward
 * scroll), resumes only when the reader returns to the latest message, and
 * re-pins whenever the transcript or its content changes size while
 * following. Paging keeps the reader next to the messages they left.
 */
export default function useTranscriptFollow({
  transcriptRef,
  followLatestRef,
  sessionId,
  stream,
  pendingUser,
  items,
  start,
  end,
  total,
  loadingEarlier,
}: {
  transcriptRef: RefObject<HTMLDivElement | null>;
  followLatestRef: MutableRefObject<boolean>;
  sessionId: string | undefined;
  stream: string;
  pendingUser: string | null;
  items: readonly { id: string }[];
  start: number;
  end: number;
  total: number;
  loadingEarlier: boolean;
}) {
  const [following, setFollowing] = useState(true);
  // The scroll position as last observed or set here. A lower position on
  // the next scroll event can only come from the reader.
  const lastTop = useRef(0);
  const paging = useRef<PagingAnchor | null>(null);
  const atLatest = end >= total;
  const atLatestRef = useRef(atLatest);
  atLatestRef.current = atLatest;
  const lastId = items[items.length - 1]?.id;

  const follow = useCallback(
    (value: boolean) => {
      followLatestRef.current = value;
      setFollowing(value);
    },
    [followLatestRef],
  );
  useEffect(() => setFollowing(true), [sessionId]);

  const pin = useCallback(() => {
    const node = transcriptRef.current;
    if (!node) return;
    if (followLatestRef.current && atLatestRef.current)
      node.scrollTop = node.scrollHeight;
    lastTop.current = node.scrollTop;
  }, [transcriptRef, followLatestRef]);

  // Paging restores the reader's place before the follow pin runs.
  useLayoutEffect(() => {
    const anchor = paging.current;
    const node = transcriptRef.current;
    if (!anchor || !node || loadingEarlier) return;
    paging.current = null;
    if (anchor.start === start && anchor.end === end && anchor.total === total)
      return;
    if (anchor.direction === "newer") {
      // The new window begins right after the message the reader left.
      node.scrollTop = 0;
    } else if (lastId === anchor.lastId) {
      // Earlier messages were added above: keep the same messages in view.
      node.scrollTop = node.scrollHeight - anchor.height + anchor.top;
    } else {
      // An earlier window ends right before the message the reader left.
      const messages = node.querySelectorAll('[id^="workspace-message-"]');
      const last = messages[messages.length - 1];
      if (last)
        node.scrollTop +=
          last.getBoundingClientRect().bottom -
          node.getBoundingClientRect().bottom;
    }
    lastTop.current = node.scrollTop;
  }, [transcriptRef, start, end, total, lastId, loadingEarlier]);

  // Pin before paint so streamed text never drifts below the fold.
  useLayoutEffect(() => {
    setFollowing(followLatestRef.current);
    pin();
  }, [followLatestRef, pin, stream, pendingUser, items, atLatest]);

  // Composer growth, notices, late images and task cards resize the
  // transcript without a scroll event; so does returning from a hidden pane.
  useEffect(() => {
    const node = transcriptRef.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => pin());
    observer.observe(node);
    const content = node.querySelector(".workspace-transcript-content");
    if (content) observer.observe(content);
    return () => observer.disconnect();
  }, [transcriptRef, pin]);

  const onScroll = (event: UIEvent<HTMLDivElement>) => {
    const node = event.currentTarget;
    if (!node.clientHeight) return; // Hidden behind an inspector.
    const top = node.scrollTop;
    const remaining = node.scrollHeight - node.clientHeight - top;
    // Content removed above clamps the position to the end; only a lower
    // position short of the end is the reader's own.
    const movedUp = top < lastTop.current - 1 && remaining >= 1;
    lastTop.current = top;
    if (movedUp) follow(false);
    else if (atLatestRef.current && remaining <= LATEST_THRESHOLD_PX)
      follow(true);
  };
  const onWheel = (event: WheelEvent<HTMLDivElement>) => {
    if (event.deltaY < 0 && event.currentTarget.scrollTop > 0) follow(false);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (READING_KEYS.has(event.key) || (event.key === " " && event.shiftKey))
      follow(false);
  };
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const node = event.currentTarget;
    // A press on the scrollbar itself, which lies outside the client area.
    if (
      event.target === node &&
      event.clientX - node.getBoundingClientRect().left >= node.clientWidth
    )
      follow(false);
  };
  const beforePaging = (direction: PagingAnchor["direction"]) => {
    follow(false);
    const node = transcriptRef.current;
    paging.current = node
      ? {
          direction,
          height: node.scrollHeight,
          top: node.scrollTop,
          lastId,
          start,
          end,
          total,
        }
      : null;
  };

  return {
    following,
    follow,
    beforePaging,
    scrollHandlers: { onScroll, onWheel, onKeyDown, onPointerDown },
  };
}

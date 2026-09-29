import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
import { expect, it, vi } from "vitest";
import useWorkspaceTranscript from "./useWorkspaceTranscript";
import type {
  ConversationSnapshot,
  TranscriptItem,
} from "../lib/workbenchTypes";
const mocks = vi.hoisted(() => ({ transcriptPage: vi.fn() }));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
const items = (start: number): TranscriptItem[] =>
  Array.from({ length: 200 }, (_, i) => ({
    id: String(start + i),
    providerItemId: String(start + i),
    turnId: null,
    itemKind: "agentMessage",
    payload: { text: String(start + i) },
    isFinal: true,
    createdAt: "now",
    updatedAt: "now",
  }));
const initial = {
  session: { id: "A" },
  items: items(200),
  olderCursor: { createdAt: "now", id: "200" },
} as ConversationSnapshot;
it("fetches older pages on demand while rendering at most 200 messages", async () => {
  mocks.transcriptPage.mockResolvedValue({ items: items(0), nextCursor: null });
  const { result } = renderHook(() => {
    const [snapshot, setSnapshot] = useState<ConversationSnapshot | null>(
      initial,
    );
    return {
      ...useWorkspaceTranscript(snapshot, setSnapshot, vi.fn()),
      snapshot,
      setSnapshot,
    };
  });
  expect(result.current.renderedItems[0].id).toBe("200");
  expect(mocks.transcriptPage).not.toHaveBeenCalled();
  await act(async () => {
    await result.current.loadEarlier();
  });
  expect(result.current.renderedItems.length).toBe(200);
  expect(result.current.renderedItems[0].id).toBe("0");
  expect(result.current.snapshot!.olderCursor).toBeNull();
  act(() => result.current.setTranscriptPage(0));
  expect(result.current.renderedItems[0].id).toBe("200");
});
it("does not append an older page to a refreshed or different conversation", async () => {
  let resolve!: (v: any) => void;
  mocks.transcriptPage.mockImplementation(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const { result } = renderHook(() => {
    const [snapshot, setSnapshot] = useState<ConversationSnapshot | null>(
      initial,
    );
    return {
      ...useWorkspaceTranscript(snapshot, setSnapshot, vi.fn()),
      snapshot,
      setSnapshot,
    };
  });
  let pending!: Promise<void>;
  act(() => {
    pending = result.current.loadEarlier();
  });
  act(() =>
    result.current.setSnapshot({
      ...initial,
      session: { ...initial.session, id: "B" },
    }),
  );
  await act(async () => {
    resolve({ items: items(0), nextCursor: null });
    await pending;
  });
  expect(result.current.snapshot!.items.length).toBe(200);
  expect(result.current.snapshot!.session.id).toBe("B");
});

it("does not skip a boundary message when a page includes tool items", async () => {
  mocks.transcriptPage.mockResolvedValue({ items: items(0), nextCursor: null });
  const short = { ...initial, items: items(200).slice(1) };
  const { result } = renderHook(() => {
    const [snapshot, setSnapshot] = useState<ConversationSnapshot | null>(
      short,
    );
    return useWorkspaceTranscript(snapshot, setSnapshot, vi.fn());
  });
  const seen = new Set(result.current.renderedItems.map((i) => i.id));
  await act(async () => {
    await result.current.loadEarlier();
  });
  result.current.renderedItems.forEach((i) => seen.add(i.id));
  await act(async () => {
    await result.current.loadEarlier();
  });
  result.current.renderedItems.forEach((i) => seen.add(i.id));
  expect(seen.size).toBe(399);
});

import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useProjectReadingState } from "./useProjectReadingState";
import type { DeskLayout } from "../../lib/deskLayout";
import type { DocumentSelection } from "../../lib/projectClient";

const passage = {
  revisionId: "pinned",
  revisionHash: "hash",
  start: 5,
  end: 9,
  page: null,
  region: null,
  quote: "Proof",
} satisfies DocumentSelection;

beforeEach(() => localStorage.clear());

it("forgets the pinned position when a comparison closes", () => {
  const { result } = renderHook(() =>
    useProjectReadingState("w", {} as DeskLayout, vi.fn()),
  );
  act(() => {
    result.current.setPinnedRevision("pinned");
    result.current.setPinnedSelection(passage);
  });
  expect(result.current.pinnedSelection).toEqual(passage);
  act(() => result.current.setPinnedRevision(null));
  expect(result.current.pinnedRevision).toBeNull();
  expect(result.current.pinnedSelection).toBeUndefined();
});

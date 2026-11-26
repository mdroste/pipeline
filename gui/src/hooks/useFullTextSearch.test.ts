import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import useFullTextSearch from "./useFullTextSearch";
import type { Workspace } from "../lib/workbench/sessionTypes";
import type { DocumentSearchSource } from "../lib/researchSearch";

const mocks = vi.hoisted(() => ({ paper: vi.fn(), files: vi.fn() }));
vi.mock("../lib/workbench/researchClient", () => ({
  researchClient: { paperSearch: mocks.paper },
}));
vi.mock("../lib/fileWorkspaceClient", () => ({
  workspaceFileAdapter: () => ({ search: mocks.files }),
}));
const projects = [
  { id: "p", name: "Paper project", root: "/paper" },
] as Workspace[];
const documents = [
  {
    project: projects[0],
    title: "Manuscript",
    revision: {
      id: "v1",
      contentHash: "hash",
      textReference: "saved.txt",
      captureComplete: true,
    },
  },
] as DocumentSearchSource[];
const hit = {
  revisionId: "v1",
  contentHash: "hash",
  start: 8200,
  end: 8206,
  line: 90,
  page: 4,
  excerpt: "A needle in the manuscript",
};
beforeEach(() => {
  vi.clearAllMocks();
  mocks.paper.mockResolvedValue([hit]);
  mocks.files.mockResolvedValue({
    matches: [
      { path: "analysis/code.do", line: 41, text: "needle in this file" },
    ],
    searchedFiles: 9,
    skippedFiles: 0,
    truncated: false,
  });
});
afterEach(() => vi.useRealTimers());
it("searches contents and preserves exact version, byte range, page, and file line", async () => {
  const { result } = renderHook(() =>
    useFullTextSearch(projects, documents, "needle", "All types", false),
  );
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(mocks.paper).toHaveBeenCalledWith("p", "v1", "needle", 21);
  expect(result.current.entries[0].route).toMatchObject({
    revisionId: "v1",
    documentStart: 8200,
    documentEnd: 8206,
    documentPage: 4,
    documentHash: "hash",
  });
  expect(result.current.entries[1].route).toMatchObject({
    file: "analysis/code.do",
    fileLine: 41,
  });
  expect(result.current.documents).toBe(1);
  expect(result.current.files).toBe(9);
});
it("debounces typing and invalidates old results immediately when scope changes", async () => {
  vi.useFakeTimers();
  let finish!: (value: unknown) => void;
  mocks.paper.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const emptyProjects: Workspace[] = [];
  const { result, rerender } = renderHook(
    ({ query, scope }) =>
      useFullTextSearch(scope, documents, query, "Documents", false),
    { initialProps: { query: "old", scope: projects } },
  );
  rerender({ query: "needle", scope: projects });
  await act(() => vi.advanceTimersByTimeAsync(300));
  expect(mocks.paper).toHaveBeenCalledTimes(1);
  rerender({ query: "needle", scope: emptyProjects });
  expect(result.current.entries).toEqual([]);
  await act(async () => finish([hit]));
  await act(() => vi.advanceTimersByTimeAsync(300));
  expect(result.current.entries).toEqual([]);
  expect(result.current.loading).toBe(false);
});
it("discloses missing capture, failed sources, and skipped or truncated file search", async () => {
  const sources = [
    ...documents,
    {
      ...documents[0],
      title: "No text",
      revision: { ...documents[0].revision, id: "v2", textReference: null },
    },
  ];
  mocks.paper.mockRejectedValue(new Error("offline"));
  mocks.files.mockResolvedValue({
    matches: [],
    searchedFiles: 3,
    skippedFiles: 2,
    truncated: true,
  });
  const { result } = renderHook(() =>
    useFullTextSearch(projects, sources, "needle", "All types", false),
  );
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.notices.join(" ")).toMatch(/no captured text/);
  expect(result.current.notices.join(" ")).toMatch(
    /document text search unavailable/,
  );
  expect(result.current.notices.join(" ")).toMatch(/2 file entries/);
  expect(result.current.notices.join(" ")).toMatch(/time or result limit/);
  mocks.paper.mockResolvedValue([hit]);
  act(() => result.current.retry());
  await waitFor(() => expect(result.current.documents).toBe(1));
  expect(result.current.notices.join(" ")).not.toMatch(
    /document text search unavailable/,
  );
});
it("loads additional passages and resets the depth for a different phrase", async () => {
  mocks.paper.mockImplementation((_p, _v, _q, limit) =>
    Promise.resolve(
      Array.from({ length: limit }, (_, start) => ({ ...hit, start })),
    ),
  );
  const { result, rerender } = renderHook(
    ({ query }) =>
      useFullTextSearch(projects, documents, query, "Documents", false),
    { initialProps: { query: "needle" } },
  );
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.entries).toHaveLength(20);
  expect(result.current.more).toBe(true);
  act(() => result.current.loadMore());
  await waitFor(() => expect(result.current.entries).toHaveLength(40));
  rerender({ query: "different" });
  expect(result.current.entries).toEqual([]);
  await waitFor(() => expect(result.current.entries).toHaveLength(20));
});
it("does no content reads for short queries or unrelated type filters", async () => {
  vi.useFakeTimers();
  const { rerender } = renderHook(
    ({ query, kind }) =>
      useFullTextSearch(projects, documents, query, kind, false),
    { initialProps: { query: "x", kind: "All types" } },
  );
  await act(() => vi.advanceTimersByTimeAsync(400));
  rerender({ query: "needle", kind: "Actions" });
  await act(() => vi.advanceTimersByTimeAsync(400));
  expect(mocks.paper).not.toHaveBeenCalled();
  expect(mocks.files).not.toHaveBeenCalled();
});
it("keeps at most three native reads running across query changes", async () => {
  vi.useFakeTimers();
  const sources = Array.from({ length: 6 }, (_, i) => ({
    ...documents[0],
    revision: { ...documents[0].revision, id: String(i) },
  }));
  const pending: (() => void)[] = [];
  mocks.paper.mockImplementation(
    () => new Promise((resolve) => pending.push(() => resolve([]))),
  );
  const { rerender, unmount } = renderHook(
    ({ query }) =>
      useFullTextSearch(projects, sources, query, "Documents", false),
    { initialProps: { query: "first" } },
  );
  await act(() => vi.advanceTimersByTimeAsync(300));
  expect(mocks.paper).toHaveBeenCalledTimes(3);
  rerender({ query: "second" });
  await act(() => vi.advanceTimersByTimeAsync(300));
  expect(mocks.paper).toHaveBeenCalledTimes(3);
  await act(async () => pending.shift()!());
  expect(mocks.paper).toHaveBeenCalledTimes(4);
  expect(mocks.paper.mock.calls[3][2]).toBe("second");
  unmount();
  await act(async () => pending.splice(0).forEach((finish) => finish()));
});

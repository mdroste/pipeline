import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import useResearchSearch from "./useResearchSearch";
import type { Workspace } from "../lib/workbench/sessionTypes";
import type { RunSummary } from "../lib/reviewTypes";
const mocks = vi.hoisted(() => ({
  home: vi.fn(),
  links: vi.fn(),
  report: vi.fn(),
}));
vi.mock("../lib/projectClient", () => ({
  projectClient: { home: mocks.home },
}));
vi.mock("../lib/workbench/releaseClient", () => ({
  listProjectReviewLinks: mocks.links,
}));
vi.mock("../lib/reviewHistoryClient", () => ({
  reviewHistoryClient: { report: mocks.report },
}));
const projects = ["a", "b"].map((id) => ({
  id,
  name: id,
  root: `/${id}`,
})) as Workspace[];
const runs: RunSummary[] = [];
const home = (body: string) => ({
  papers: [],
  inventory: null,
  notes: [{ id: body, body, kind: "decision", state: "accepted", revision: 1 }],
});
beforeEach(() => {
  vi.resetAllMocks();
  mocks.links.mockResolvedValue([]);
  mocks.home.mockResolvedValue(home("available"));
  mocks.report.mockResolvedValue({ step_outputs: [] });
});
it("ignores a slow response after project scope changes", async () => {
  let finish!: (value: unknown) => void;
  mocks.home.mockImplementation((id: string) =>
    id === "a"
      ? new Promise((resolve) => {
          finish = resolve;
        })
      : Promise.resolve(home("B note")),
  );
  const { result, rerender } = renderHook(
    ({ id }) => useResearchSearch(projects, runs, id),
    { initialProps: { id: "a" } },
  );
  rerender({ id: "b" });
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.entries[0].label).toBe("B note");
  await act(async () => finish(home("Stale A note")));
  expect(result.current.entries.map((entry) => entry.label)).toEqual([
    "B note",
  ]);
});
it("bounds report reads and exposes older findings instead of claiming complete coverage", async () => {
  const reviews = Array.from({ length: 55 }, (_, i) => ({
    run_id: `r-${i}`,
    input_name: `Review ${i}`,
    input_path: `/a/${i}.pdf`,
  })) as RunSummary[];
  const { result } = renderHook(() =>
    useResearchSearch(projects, reviews, "a"),
  );
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(mocks.report).toHaveBeenCalledTimes(50);
  expect(result.current.more).toBe(true);
  act(() => result.current.loadMore());
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(mocks.report).toHaveBeenCalledWith({ runId: "r-54" });
  expect(result.current.more).toBe(false);
});
it("uses explicit handoff links for project findings outside the project folder", async () => {
  mocks.links.mockResolvedValue([
    {
      id: "handoff",
      stagedPath: "/staged/source.pdf",
      externalReference: "external",
      contentHash: "hash",
    },
  ]);
  const reviews = [
    {
      run_id: "external",
      input_name: "Saved review",
      input_path: "/staged/source.pdf",
    },
  ] as RunSummary[];
  const { result } = renderHook(() =>
    useResearchSearch(projects, reviews, "a"),
  );
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.runProjects.get("external")?.id).toBe("a");
  expect(mocks.report).toHaveBeenCalledWith({ runId: "external" });
});

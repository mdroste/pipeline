import { beforeEach, expect, it } from "vitest";
import {
  attentionVersion,
  deferred,
  orderAttention,
  loadAttentionPreferences,
  saveAttentionPreferences,
} from "./attention";
import { markActivitySeen, readActivitySince } from "./visits";
import { recentWork, rememberWork } from "./continuation";
import type { OverviewRow } from "./types";
const task: OverviewRow = {
  id: "task-1",
  version: "1",
  text: "Check assumptions",
  category: "task",
  action: { kind: "tab", tab: "tasks", label: "Open", targetId: "1" },
};
beforeEach(() => localStorage.clear());
it("defers one revision for 24 hours, but never hides a blocker", () => {
  const prefs = {
    [task.id]: { version: attentionVersion(task), laterUntil: 1000 },
  };
  expect(deferred(task, prefs, 999)).toBe(true);
  expect(deferred(task, prefs, 1000)).toBe(false);
  expect(deferred({ ...task, version: "2" }, prefs, 999)).toBe(false);
  expect(deferred({ ...task, category: "blocker" }, prefs, 999)).toBe(false);
  saveAttentionPreferences("p", prefs);
  expect(loadAttentionPreferences("p")).toEqual(prefs);
});
it("keeps blockers above a pinned task", () => {
  const blocker = { ...task, id: "block", category: "blocker" as const };
  expect(
    orderAttention([task, blocker], {
      [task.id]: { version: "1", pinned: true },
    }),
  ).toEqual([blocker, task]);
});
it("only advances activity when the researcher acknowledges it", () => {
  const first = new Date("2026-11-20T00:00:00Z"),
    later = new Date("2026-11-23T00:00:00Z");
  const initial = readActivitySince("p", first, []);
  expect(readActivitySince("p", later, [])).toEqual(initial);
  markActivitySeen("p", later, []);
  expect(readActivitySince("p", later, []).since).toBe(later.toISOString());
});
it("retains exact file navigation per project without duplicating buffers", () => {
  rememberWork("p", {
    id: "file:a",
    title: "a.tex",
    tab: "files",
    location: { path: "a.tex", line: 42 },
  });
  expect(recentWork("p")[0].location).toEqual({ path: "a.tex", line: 42 });
  expect(recentWork("other")).toEqual([]);
});

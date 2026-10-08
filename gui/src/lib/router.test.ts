import { beforeEach, expect, it, vi } from "vitest";
import {
  parseRoute,
  restoreRoute,
  router,
  serializeRoute,
  type AppRoute,
} from "./router";

beforeEach(() => {
  localStorage.clear();
  router.init({ page: "home" });
  router.setGuard(null);
});

it("serializes and parses every destination round-trip", () => {
  const routes: AppRoute[] = [
    { page: "home" },
    { page: "main" },
    { page: "project-index" },
    { page: "workspace" },
    {
      page: "workspace",
      projectId: "ws one",
      surface: "project",
      destination: "checks",
    },
    { page: "workspace", sessionId: "sess-1", surface: "chat" },
    { page: "workspace", projectId: "研究/α%?#" },
    { page: "tasks" },
    { page: "tasks", taskId: "t1", taskSessionId: "s1" },
    { page: "tasks", discoveryId: "d1" },
    { page: "pipeline" },
    { page: "gallery" },
    { page: "batch" },
    { page: "projects" },
    { page: "history" },
    { page: "history", runId: "run-9" },
    { page: "settings" },
    { page: "settings", section: "workflow", targetId: "usage-limit-fallback" },
    { page: "settings", section: "偏好/α%?#" },
    { page: "help" },
    { page: "help", section: "privacy" },
  ];
  for (const route of routes) {
    expect(parseRoute(serializeRoute(route)), serializeRoute(route)).toEqual(
      route,
    );
  }
  expect(parseRoute("#/nowhere")).toBeNull();
  expect(parseRoute("")).toBeNull();
  expect(parseRoute("#/help?section=bogus")).toEqual({ page: "help" });
});

it.each([
  "#/project/%",
  "#/project/%2",
  "#/project/%GG",
  "#/settings/%E0%A4%A",
  "#/settings/%FF",
  "#/project/%ED%A0%80",
  "#/project/valid/%",
])("rejects malformed encoded segments in %s", (route) => {
  expect(parseRoute(route)).toBeNull();
});

it("falls back from a corrupted saved route to the legacy page or default", () => {
  localStorage.setItem("pipeline.ui.route", "#/project/%");
  localStorage.setItem("pipeline.ui.page", "home");
  expect(restoreRoute(localStorage)).toEqual({ page: "home" });
  localStorage.removeItem("pipeline.ui.page");
  expect(restoreRoute(localStorage)).toBeNull();
});

it("treats unreadable saved navigation as optional", () => {
  const storage = {
    getItem: vi.fn((key: string) => {
      if (key === "pipeline.ui.route")
        throw new DOMException("Blocked", "SecurityError");
      return "project-index";
    }),
  };
  expect(restoreRoute(storage)).toEqual({ page: "project-index" });
  storage.getItem.mockImplementation(() => {
    throw new DOMException("Blocked", "SecurityError");
  });
  expect(restoreRoute(storage)).toBeNull();
});

it("persists committed routes and restores them, ignoring junk", () => {
  router.apply({ page: "settings", section: "general" });
  expect(restoreRoute(localStorage)).toEqual({
    page: "settings",
    section: "general",
  });
  localStorage.setItem("pipeline.ui.route", "#/not/a/route");
  localStorage.setItem("pipeline.ui.page", "project-index");
  expect(restoreRoute(localStorage)).toEqual({ page: "project-index" });
  localStorage.removeItem("pipeline.ui.page");
  expect(restoreRoute(localStorage)).toBeNull();
});

it("runs the guard on navigate and skips it on apply", async () => {
  const guard = vi.fn(async (next: AppRoute) => next.page !== "settings");
  router.setGuard(guard);
  expect(await router.navigate({ page: "settings" })).toBe(false);
  expect(router.route).toEqual({ page: "home" });
  expect(await router.navigate({ page: "tasks" })).toBe(true);
  expect(router.route).toEqual({ page: "tasks" });
  router.apply({ page: "settings" });
  expect(router.route).toEqual({ page: "settings" });
  expect(guard).toHaveBeenCalledTimes(2);
});

it("notifies subscribers with an increasing revision, even for equal routes", () => {
  const seen: number[] = [];
  const off = router.subscribe(({ revision }) => seen.push(revision));
  router.apply({ page: "tasks" });
  router.apply({ page: "tasks" });
  expect(seen.length).toBe(2);
  expect(seen[1]).toBeGreaterThan(seen[0]);
  off();
  router.apply({ page: "home" });
  expect(seen.length).toBe(2);
});

it("recovers from malformed initial and changed hashes and still follows valid navigation", async () => {
  window.history.replaceState(null, "", "#/settings/%E0%A4%A");
  router.installHashSync();
  expect(router.route).toEqual({ page: "home" });
  expect(window.location.hash).toBe("#/home");
  router.apply({ page: "history", runId: "r1" });
  expect(window.location.hash).toBe("#/reviews/history?run=r1");
  await Promise.resolve();
  window.history.replaceState(null, "", "#/project/%");
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  expect(router.route).toEqual({ page: "history", runId: "r1" });
  expect(window.location.hash).toBe("#/reviews/history?run=r1");
  await Promise.resolve();
  // Simulate external (back/forward) hash change.
  window.location.hash = "#/automations";
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  await vi.waitFor(() => expect(router.route).toEqual({ page: "tasks" }));
});

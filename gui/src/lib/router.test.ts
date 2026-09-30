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

it("reflects routes into the hash and follows hash-driven back navigation", async () => {
  router.installHashSync();
  router.apply({ page: "history", runId: "r1" });
  expect(window.location.hash).toBe("#/reviews/history?run=r1");
  // Simulate external (back/forward) hash change.
  window.location.hash = "#/automations";
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  await vi.waitFor(() => expect(router.route).toEqual({ page: "tasks" }));
});

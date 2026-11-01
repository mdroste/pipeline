import { beforeEach, expect, it, vi } from "vitest";
import {
  GET_STARTED_KEY,
  onGetStartedRequest,
  openGetStarted,
  parseGetStarted,
  readGetStarted,
  saveGetStarted,
  shouldOfferGetStarted,
} from "./getStarted";

beforeEach(() => localStorage.clear());

it("offers the guide once on a first launch and not after it is closed", () => {
  expect(readGetStarted()).toBeNull();
  expect(shouldOfferGetStarted()).toBe(true);
  saveGetStarted({ dismissedAt: "2026-10-28T12:00:00.000Z" });
  expect(shouldOfferGetStarted()).toBe(false);
  expect(JSON.parse(localStorage.getItem(GET_STARTED_KEY)!)).toEqual({
    version: 1,
    dismissedAt: "2026-10-28T12:00:00.000Z",
  });
});

it("keeps a dismissal saved with the earlier kind-of-work choice", () => {
  localStorage.setItem(
    GET_STARTED_KEY,
    JSON.stringify({
      version: 1,
      dismissedAt: "2026-10-28T12:00:00.000Z",
      persona: "r-word",
    }),
  );
  expect(shouldOfferGetStarted()).toBe(false);
  saveGetStarted({ dismissedAt: "2026-10-29T12:00:00.000Z" });
  expect(JSON.parse(localStorage.getItem(GET_STARTED_KEY)!)).toEqual({
    version: 1,
    dismissedAt: "2026-10-29T12:00:00.000Z",
  });
});

it("does not open over an existing installation's work", () => {
  localStorage.setItem("pipeline.ui.route", "#/projects");
  expect(shouldOfferGetStarted()).toBe(false);
  localStorage.clear();
  // Unrelated keys (for example the theme) are not Pipeline state.
  localStorage.setItem("theme", "dark");
  expect(shouldOfferGetStarted()).toBe(true);
});

it("tolerates corrupt records and unavailable storage", () => {
  expect(parseGetStarted("broken")).toEqual({
    version: 1,
    dismissedAt: null,
  });
  expect(parseGetStarted(JSON.stringify({ dismissedAt: 7 }))).toEqual({
    version: 1,
    dismissedAt: null,
  });
  const unavailable = {
    getItem: () => {
      throw new Error("blocked");
    },
    setItem: () => {
      throw new Error("blocked");
    },
    get length(): number {
      throw new Error("blocked");
    },
    key: () => null,
  };
  expect(shouldOfferGetStarted(unavailable)).toBe(false);
  expect(() =>
    saveGetStarted({ dismissedAt: null }, unavailable),
  ).not.toThrow();
});

it("delivers open requests only to mounted listeners", () => {
  const listener = vi.fn();
  const stop = onGetStartedRequest(listener);
  openGetStarted();
  openGetStarted("example");
  stop();
  openGetStarted();
  expect(listener.mock.calls).toEqual([["setup"], ["example"]]);
});

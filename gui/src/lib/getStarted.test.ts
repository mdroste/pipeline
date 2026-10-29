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
  saveGetStarted({ persona: "r-word" });
  // Choosing a persona alone does not count as closing the guide.
  expect(shouldOfferGetStarted()).toBe(true);
  saveGetStarted({ dismissedAt: "2026-10-28T12:00:00.000Z" });
  expect(shouldOfferGetStarted()).toBe(false);
  expect(JSON.parse(localStorage.getItem(GET_STARTED_KEY)!)).toEqual({
    version: 1,
    dismissedAt: "2026-10-28T12:00:00.000Z",
    persona: "r-word",
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
    persona: null,
  });
  expect(
    parseGetStarted(JSON.stringify({ dismissedAt: 7, persona: "sas" })),
  ).toEqual({ version: 1, dismissedAt: null, persona: null });
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
  expect(() => saveGetStarted({ persona: "other" }, unavailable)).not.toThrow();
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

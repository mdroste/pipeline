import { beforeEach, expect, it, vi } from "vitest";
import {
  DEFAULT_PREFERENCES,
  PREFERENCES_KEY,
  parsePreferences,
  readAppPreferences,
  saveAppPreferences,
  shouldSendMessage,
  startupRoute,
} from "./appPreferences";

beforeEach(() => localStorage.clear());
it("preserves old startup behavior and supplies defaults for missing or corrupt preferences", () => {
  for (const raw of [null, "broken", "null", "[]"])
    expect(parsePreferences(raw)).toEqual(DEFAULT_PREFERENCES);
  localStorage.setItem("pipeline.ui.page", "workspace");
  expect(startupRoute(localStorage)).toEqual({ page: "workspace" });
  localStorage.setItem("pipeline.ui.route", "#/reviews/history?run=run-7");
  expect(startupRoute(localStorage)).toEqual({
    page: "history",
    runId: "run-7",
  });
  localStorage.setItem("pipeline.ui.route", "#/nowhere");
  expect(startupRoute(localStorage)).toEqual({ page: "workspace" });
  saveAppPreferences({ startup: "home" });
  expect(startupRoute(localStorage)).toEqual({ page: "home" });
  expect(localStorage.getItem("pipeline.workspace.workspaceId")).toBeNull();
});
it("bounds malformed values and retains unrelated preferences when saving", () => {
  const parsed = parsePreferences(
    JSON.stringify({
      interfaceScale: 900,
      editorFontSize: -3,
      reportScale: "large",
      notifyFailure: "false",
      density: "compact",
    }),
  );
  expect(parsed.interfaceScale).toBe(125);
  expect(parsed.editorFontSize).toBe(11);
  expect(parsed.reportScale).toBe(100);
  expect(parsed.notifyFailure).toBe(true);
  saveAppPreferences({ density: "compact", editorWrap: true });
  saveAppPreferences({ reportScale: 125 });
  expect(readAppPreferences()).toMatchObject({
    density: "compact",
    editorWrap: true,
    reportScale: 125,
  });
  expect(JSON.parse(localStorage.getItem(PREFERENCES_KEY)!)).toMatchObject({
    version: 1,
    reportScale: 125,
  });
});
it("does not apply preferences when persistence fails", () => {
  const write = vi.spyOn(localStorage, "setItem").mockImplementation(() => {
    throw new Error("full");
  });
  expect(() => saveAppPreferences({ editorFontSize: 18 })).toThrow();
  expect(readAppPreferences().editorFontSize).toBe(13);
  write.mockRestore();
});
it("honors send shortcuts without sending IME composition, newlines, or alternate-modifier input", () => {
  const enter = {
    key: "Enter",
    shiftKey: false,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    isComposing: false,
  };
  expect(shouldSendMessage(enter, "enter")).toBe(true);
  expect(shouldSendMessage(enter, "mod-enter")).toBe(false);
  for (const key of ["metaKey", "ctrlKey"] as const)
    expect(shouldSendMessage({ ...enter, [key]: true }, "mod-enter")).toBe(
      true,
    );
  for (const key of ["shiftKey", "altKey", "isComposing"] as const)
    for (const shortcut of ["enter", "mod-enter"] as const)
      expect(
        shouldSendMessage({ ...enter, metaKey: true, [key]: true }, shortcut),
      ).toBe(false);
});

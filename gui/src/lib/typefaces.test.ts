import { afterEach, beforeEach, expect, it, vi } from "vitest";
import bootstrap from "../../public/theme-bootstrap.js?raw";
import { parsePreferences, PREFERENCES_KEY } from "./appPreferences";
import { TYPEFACE_OPTIONS } from "./typefaces";

beforeEach(() => {
  localStorage.clear();
  vi.stubGlobal("matchMedia", () => ({ matches: false }));
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  delete document.documentElement.dataset.typeface;
});

it.each(TYPEFACE_OPTIONS)(
  "restores $label before React with the same preference",
  ({ value }) => {
    const raw = JSON.stringify({ typeface: value, interfaceScale: 110 });
    localStorage.setItem(PREFERENCES_KEY, raw);
    new Function(bootstrap)();
    expect(document.documentElement.dataset.typeface).toBe(value);
    expect(parsePreferences(raw)).toMatchObject({
      typeface: value,
      interfaceScale: 110,
    });
  },
);

it.each([
  null,
  "broken",
  "null",
  "[]",
  "{}",
  '{"typeface":"unknown"}',
  '{"typeface":12}',
])("uses Inter for a missing or corrupt choice: %s", (raw) => {
  if (raw !== null) localStorage.setItem(PREFERENCES_KEY, raw);
  document.documentElement.dataset.typeface = "source";
  new Function(bootstrap)();
  expect(document.documentElement.dataset.typeface).toBe("inter");
  expect(parsePreferences(raw).typeface).toBe("inter");
});

it("can paint the bundled default when local storage is unavailable", () => {
  vi.spyOn(localStorage, "getItem").mockImplementation(() => {
    throw new Error("denied");
  });
  new Function(bootstrap)();
  expect(document.documentElement.dataset.typeface).toBe("inter");
});

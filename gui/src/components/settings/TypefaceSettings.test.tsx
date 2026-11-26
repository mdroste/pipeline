import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AppearanceSettings } from "./PreferenceSettings";
import { useAppAppearance } from "../../hooks/useAppAppearance";
import {
  PREFERENCES_KEY,
  readAppPreferences,
  saveAppPreferences,
} from "../../lib/appPreferences";
import { TYPEFACE_OPTIONS } from "../../lib/typefaces";
import { settingsSearchEntries } from "./search";
import { makeSettings } from "../../test/settingsFixtures";

function Appearance() {
  useAppAppearance();
  return <AppearanceSettings theme="system" onThemeChange={() => {}} />;
}
beforeEach(() => localStorage.clear());
afterEach(() => {
  vi.restoreAllMocks();
  delete document.documentElement.dataset.typeface;
  document.documentElement.style.fontSize = "";
  delete document.documentElement.dataset.density;
});

it("applies and persists all four choices without changing scale or editor preferences", async () => {
  saveAppPreferences({
    interfaceScale: 110,
    reportScale: 125,
    editorFontSize: 18,
  });
  const user = userEvent.setup();
  const first = render(<Appearance />);
  const select = screen.getByRole("combobox", { name: "Typeface" });
  expect(select).toHaveValue("inter");
  expect(within(select).getAllByRole("option")).toHaveLength(4);
  for (const { value } of TYPEFACE_OPTIONS) {
    await user.selectOptions(select, value);
    expect(document.documentElement.dataset.typeface).toBe(value);
    expect(readAppPreferences()).toMatchObject({
      typeface: value,
      interfaceScale: 110,
      reportScale: 125,
      editorFontSize: 18,
    });
  }
  expect(select).toHaveAccessibleDescription(
    "Source Sans 3 for the interface; Source Serif 4 for reports.",
  );
  first.unmount();
  render(<Appearance />);
  expect(screen.getByRole("combobox", { name: "Typeface" })).toHaveValue(
    "source",
  );
  expect(document.documentElement.dataset.typeface).toBe("source");
  expect(JSON.parse(localStorage.getItem(PREFERENCES_KEY)!)).toMatchObject({
    typeface: "source",
  });
});

it("retains the active typeface and shows the save failure", async () => {
  saveAppPreferences({ typeface: "ibm-plex-sans" });
  const user = userEvent.setup();
  render(<Appearance />);
  vi.spyOn(localStorage, "setItem").mockImplementation(() => {
    throw new Error("full");
  });
  await user.selectOptions(
    screen.getByRole("combobox", { name: "Typeface" }),
    "source",
  );
  expect(screen.getByRole("alert")).toHaveTextContent(
    "Could not save this preference",
  );
  expect(screen.getByRole("combobox", { name: "Typeface" })).toHaveValue(
    "ibm-plex-sans",
  );
  expect(document.documentElement.dataset.typeface).toBe("ibm-plex-sans");
  expect(readAppPreferences().typeface).toBe("ibm-plex-sans");
});

it("finds typefaces by family name and targets the selector", () => {
  saveAppPreferences({ typeface: "instrument-sans" });
  render(<Appearance />);
  const entry = settingsSearchEntries(
    makeSettings(),
    readAppPreferences(),
    "system",
  ).find((entry) => entry.label === "Typeface")!;
  expect(entry.value).toBe("Instrument Sans");
  for (const { label } of TYPEFACE_OPTIONS)
    expect(entry.description).toContain(label);
  expect(document.getElementById(entry.target)?.querySelector("select")).toBe(
    screen.getByRole("combobox", { name: "Typeface" }),
  );
});

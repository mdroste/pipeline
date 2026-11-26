import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AppearanceSettings } from "./PreferenceSettings";
import { useAppTheme } from "../../hooks/app/useAppTheme";
import { readAppPreferences } from "../../lib/appPreferences";
import { THEME_STORAGE_KEY } from "../../lib/theme";
import { settingsSearchEntries } from "./search";
import { makeSettings } from "../../test/settingsFixtures";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setTheme: vi.fn().mockResolvedValue(undefined) }),
}));

function Appearance() {
  const { theme, handleThemeChange } = useAppTheme();
  return <AppearanceSettings theme={theme} onThemeChange={handleThemeChange} />;
}

beforeEach(() => {
  localStorage.clear();
  vi.stubGlobal("matchMedia", () => ({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }));
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("offers the original appearance buttons plus Custom without a preview grid", () => {
  const { container } = render(<Appearance />);
  const modes = within(screen.getByRole("radiogroup", { name: "Appearance" }));
  expect(modes.getAllByRole("radio")).toHaveLength(4);
  expect(modes.getByRole("radio", { name: "System" })).toBeChecked();
  for (const name of ["Light", "Dark", "Custom"])
    expect(modes.getByRole("radio", { name })).not.toBeChecked();
  expect(container.querySelector(".theme-preview")).toBeNull();
  expect(
    screen.queryByRole("combobox", { name: "Custom color scheme" }),
  ).not.toBeInTheDocument();
});

it.each(["system", "light", "dark"] as const)(
  "opens Custom from %s without changing or saving the active choice",
  async (mode) => {
    localStorage.setItem(THEME_STORAGE_KEY, mode);
    const user = userEvent.setup();
    render(<Appearance />);
    const before = document.documentElement.className;
    const write = vi.spyOn(localStorage, "setItem");
    await user.click(screen.getByRole("radio", { name: "Custom" }));
    const select = screen.getByRole("combobox", {
      name: "Custom color scheme",
    });
    expect(select).toHaveValue(mode);
    expect(
      within(select)
        .getAllByRole("option")
        .slice(0, 3)
        .map((option) => option.textContent),
    ).toEqual(["System", "Light", "Dark"]);
    expect(document.documentElement.className).toBe(before);
    expect(document.documentElement.dataset.colorTheme).toBe("default");
    expect(write).not.toHaveBeenCalled();
  },
);

it("applies a selected palette and reopens Custom with the current choice", async () => {
  const user = userEvent.setup();
  const first = render(<Appearance />);
  await user.click(screen.getByRole("radio", { name: "Custom" }));
  const select = screen.getByRole("combobox", { name: "Custom color scheme" });
  expect(select).toHaveValue("system");
  expect(within(select).getAllByRole("option")).toHaveLength(15);
  await user.selectOptions(select, "harbor-dark");
  expect(document.documentElement).toHaveClass("dark");
  expect(document.documentElement.dataset.colorTheme).toBe("harbor");
  expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("harbor-dark");
  await user.selectOptions(select, "parchment-light");
  expect(document.documentElement).not.toHaveClass("dark");
  expect(document.documentElement.dataset.colorTheme).toBe("parchment");
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  first.unmount();
  render(<Appearance />);
  expect(screen.getByRole("radio", { name: "Custom" })).toBeChecked();
  expect(
    screen.getByRole("combobox", { name: "Custom color scheme" }),
  ).toHaveValue("parchment-light");
  expect(document.documentElement.dataset.colorTheme).toBe("parchment");
  await user.click(screen.getByRole("radio", { name: "Dark" }));
  expect(document.documentElement).toHaveClass("dark");
  expect(document.documentElement.dataset.colorTheme).toBe("default");
  expect(
    screen.queryByRole("combobox", { name: "Custom color scheme" }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("radio", { name: "Custom" }));
  expect(
    screen.getByRole("combobox", { name: "Custom color scheme" }),
  ).toHaveValue("dark");
  expect(document.documentElement).toHaveClass("dark");
  expect(document.documentElement.dataset.colorTheme).toBe("default");
});

it("keeps the selected mode and palette when persistence fails", async () => {
  const user = userEvent.setup();
  render(<Appearance />);
  vi.spyOn(localStorage, "setItem").mockImplementation(() => {
    throw new Error("full");
  });
  await user.click(screen.getByRole("radio", { name: "Custom" }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  await user.selectOptions(
    screen.getByRole("combobox", { name: "Custom color scheme" }),
    "iris-dark",
  );
  expect(screen.getByRole("alert")).toHaveTextContent("Could not save theme");
  expect(screen.getByRole("radio", { name: "Custom" })).toBeChecked();
  expect(
    screen.getByRole("combobox", { name: "Custom color scheme" }),
  ).toHaveValue("system");
  expect(document.documentElement).not.toHaveClass("dark");
  expect(document.documentElement.dataset.colorTheme).toBe("default");
});

it("supports arrow keys and initially opens Custom in the current appearance", async () => {
  const user = userEvent.setup();
  render(<Appearance />);
  await user.click(screen.getByRole("radio", { name: "System" }));
  await user.keyboard("{ArrowRight}");
  expect(screen.getByRole("radio", { name: "Light" })).toBeChecked();
  await user.keyboard("{ArrowRight}");
  expect(screen.getByRole("radio", { name: "Dark" })).toBeChecked();
  await user.keyboard("{ArrowRight}");
  expect(screen.getByRole("radio", { name: "Custom" })).toBeChecked();
  expect(
    screen.getByRole("combobox", { name: "Custom color scheme" }),
  ).toHaveValue("dark");
  expect(document.documentElement).toHaveClass("dark");
  expect(document.documentElement.dataset.colorTheme).toBe("default");
});

it("keeps defaults available in Custom until a standard appearance button is chosen", async () => {
  localStorage.setItem(THEME_STORAGE_KEY, "grove-dark");
  const user = userEvent.setup();
  render(<Appearance />);
  const select = screen.getByRole("combobox", { name: "Custom color scheme" });
  expect(select).toHaveValue("grove-dark");
  await user.selectOptions(select, "light");
  expect(document.documentElement).not.toHaveClass("dark");
  expect(document.documentElement.dataset.colorTheme).toBe("default");
  expect(screen.getByRole("radio", { name: "Custom" })).toBeChecked();
  expect(select).toHaveValue("light");
  await user.selectOptions(select, "system");
  expect(select).toHaveValue("system");
  await user.click(screen.getByRole("radio", { name: "System" }));
  expect(
    screen.queryByRole("combobox", { name: "Custom color scheme" }),
  ).not.toBeInTheDocument();
});

it("searches palette names through the appearance controls", () => {
  render(<Appearance />);
  const entries = settingsSearchEntries(
    makeSettings(),
    readAppPreferences(),
    "iris-dark",
  );
  const entry = entries.find((entry) => entry.label === "Appearance")!;
  expect(entry.description).toContain("Harbor, Parchment, Iris");
  expect(entry.description).toContain("Grove, Ember, Lagoon");
  expect(entry.value).toBe("Iris Dark");
  expect(
    document.getElementById(entry.target)?.querySelector('[role="radiogroup"]'),
  ).toBe(screen.getByRole("radiogroup", { name: "Appearance" }));
  expect(entries.some((entry) => entry.target.endsWith("-color-theme"))).toBe(
    false,
  );
});

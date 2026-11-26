// A single value selects both mode and palette. The old keys are read only
// during migration so choosing Default cannot resurrect an old palette.
export const THEME_STORAGE_KEY = "pipeline.ui.theme";

/** Original palettes; Default retains Pipeline's existing colors. */
export const COLOR_THEMES = [
  { id: "default", name: "Default", description: "Classic Pipeline colors" },
  { id: "harbor", name: "Harbor", description: "Cool slate and ocean blue" },
  { id: "parchment", name: "Parchment", description: "Warm paper and amber" },
  { id: "iris", name: "Iris", description: "Soft violet and plum" },
  { id: "grove", name: "Grove", description: "Sage and forest green" },
  { id: "ember", name: "Ember", description: "Warm ash and copper" },
  { id: "lagoon", name: "Lagoon", description: "Sea glass and teal" },
] as const;

export type ColorTheme = (typeof COLOR_THEMES)[number]["id"];
type ThemeMode = "light" | "dark" | "system";
export type ThemePreference =
  ThemeMode | `${Exclude<ColorTheme, "default">}-${"light" | "dark"}`;

interface ThemeOption {
  value: ThemePreference;
  label: string;
  mode: ThemeMode;
  colorTheme: ColorTheme;
}

export const THEME_OPTIONS: readonly ThemeOption[] = [
  { value: "system", label: "System", mode: "system", colorTheme: "default" },
  {
    value: "light",
    label: "Light",
    mode: "light",
    colorTheme: "default",
  },
  { value: "dark", label: "Dark", mode: "dark", colorTheme: "default" },
  ...COLOR_THEMES.filter((palette) => palette.id !== "default").flatMap(
    (palette) =>
      (["light", "dark"] as const).map((mode) => ({
        value: `${palette.id}-${mode}` as const,
        label: `${palette.name} ${mode === "light" ? "Light" : "Dark"}`,
        mode,
        colorTheme: palette.id,
      })),
  ),
];

export function themeOption(preference: ThemePreference): ThemeOption {
  return (
    THEME_OPTIONS.find((option) => option.value === preference) ??
    THEME_OPTIONS[0]
  );
}

export function parseColorTheme(value: unknown): ColorTheme {
  return COLOR_THEMES.find((theme) => theme.id === value)?.id ?? "default";
}

export function readThemePreference(
  storage: Pick<Storage, "getItem">,
): ThemePreference {
  try {
    const stored = storage.getItem(THEME_STORAGE_KEY);
    if (stored !== null)
      return (
        THEME_OPTIONS.find((option) => option.value === stored)?.value ??
        "system"
      );

    const legacyMode = storage.getItem("theme");
    if (legacyMode !== "light" && legacyMode !== "dark") return "system";
    let palette: ColorTheme = "default";
    try {
      const legacy = JSON.parse(
        storage.getItem("pipeline.ui.preferences") ?? "{}",
      );
      palette = parseColorTheme(legacy?.[`${legacyMode}ColorTheme`]);
    } catch {
      // An unreadable legacy palette must not discard a valid mode.
    }
    return palette === "default" ? legacyMode : `${palette}-${legacyMode}`;
  } catch {
    return "system";
  }
}

export function resolveDarkTheme(
  preference: ThemePreference,
  systemIsDark: boolean,
): boolean {
  const { mode } = themeOption(preference);
  return mode === "dark" || (mode === "system" && systemIsDark);
}

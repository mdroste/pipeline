export const THEME_STORAGE_KEY = "theme";

export type ThemePreference = "light" | "dark" | "system";

export function readThemePreference(storage: Pick<Storage, "getItem">): ThemePreference {
  const stored = storage.getItem(THEME_STORAGE_KEY);
  return stored === "light" || stored === "dark" || stored === "system"
    ? stored
    : "system";
}

export function resolveDarkTheme(
  preference: ThemePreference,
  systemIsDark: boolean,
): boolean {
  return preference === "dark" || (preference === "system" && systemIsDark);
}

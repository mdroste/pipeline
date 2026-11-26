(() => {
  const root = document.documentElement;
  // Keep in sync with src/lib/typefaces.ts; restore before the app's first paint.
  let typeface = "inter";
  try {
    const saved = JSON.parse(
      localStorage.getItem("pipeline.ui.preferences") || "{}",
    );
    if (
      ["inter", "instrument-sans", "ibm-plex-sans", "source"].includes(
        saved?.typeface,
      )
    )
      typeface = saved.typeface;
  } catch {
    // Missing, malformed, or unavailable preferences use the bundled default.
  }
  root.dataset.typeface = typeface;
  let preference = "system";
  // Keep in sync with THEME_OPTIONS in src/lib/theme.ts. The bootstrap tests
  // exercise every option so first paint agrees with the React preference.
  const palettes = ["harbor", "parchment", "iris", "grove", "ember", "lagoon"];
  const choices = [
    "system",
    "light",
    "dark",
    ...palettes.flatMap((palette) => [`${palette}-light`, `${palette}-dark`]),
  ];

  try {
    const stored = localStorage.getItem("pipeline.ui.theme");
    if (stored !== null) {
      preference = choices.includes(stored) ? stored : "system";
    } else {
      const legacyMode = localStorage.getItem("theme");
      if (legacyMode === "light" || legacyMode === "dark") {
        preference = legacyMode;
        try {
          const legacy = JSON.parse(
            localStorage.getItem("pipeline.ui.preferences") || "{}",
          );
          const palette = legacy?.[`${legacyMode}ColorTheme`];
          if (palettes.includes(palette))
            preference = `${palette}-${legacyMode}`;
        } catch {
          // Keep a valid legacy mode even if its palette is unreadable.
        }
      }
    }
  } catch {
    // Keep the system fallback when storage is unavailable.
  }

  const systemIsDark = window.matchMedia(
    "(prefers-color-scheme: dark)",
  ).matches;
  const parts = preference.split("-");
  const mode = parts[parts.length - 1];
  const dark = mode === "dark" || (mode === "system" && systemIsDark);
  const colorTheme = parts.length === 2 ? parts[0] : "default";
  root.classList.toggle("dark", dark);
  root.dataset.colorTheme = colorTheme;
  root.style.colorScheme = dark ? "dark" : "light";
})();

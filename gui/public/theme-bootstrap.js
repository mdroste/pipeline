(() => {
  const root = document.documentElement;
  let preference = "system";

  try {
    const stored = localStorage.getItem("theme");
    if (stored === "light" || stored === "dark" || stored === "system") {
      preference = stored;
    }
  } catch {
    // Keep the system fallback when storage is unavailable.
  }

  const systemIsDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const dark = preference === "dark" || (preference === "system" && systemIsDark);
  root.classList.toggle("dark", dark);
  root.style.colorScheme = dark ? "dark" : "light";
})();

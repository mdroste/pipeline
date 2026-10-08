import colors from "tailwindcss/colors.js";

const adaptiveAccent = {
  50: "rgb(var(--color-accent-50) / <alpha-value>)",
  100: "rgb(var(--color-accent-100) / <alpha-value>)",
  200: "rgb(var(--color-accent-200) / <alpha-value>)",
  300: "rgb(var(--color-accent-300) / <alpha-value>)",
  400: "rgb(var(--color-accent-400) / <alpha-value>)",
  500: "rgb(var(--color-accent-500) / <alpha-value>)",
  600: "rgb(var(--color-accent-600) / <alpha-value>)",
  700: "rgb(var(--color-accent-700) / <alpha-value>)",
  800: "rgb(var(--color-accent-800) / <alpha-value>)",
  900: "rgb(var(--color-accent-900) / <alpha-value>)",
  950: "rgb(var(--color-accent-950) / <alpha-value>)",
};

/** @type {import('tailwindcss').Config} */
export default {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      // Tailwind's default gray palette has a cool blue cast. Keep the
      // existing semantic gray utilities, but render them as true neutrals so
      // dark mode uses black and gray surfaces.
      colors: {
        gray: colors.neutral,
        blue: adaptiveAccent,
        // Semantic tokens from App.css. Theme switching happens in the
        // variables, so these need no dark: variant.
        surface: "var(--ui-surface)",
        raised: "var(--ui-raised)",
        sunken: "var(--ui-sunken)",
        line: "var(--ui-border)",
        "line-strong": "var(--ui-border-strong)",
        ink: "var(--ui-text)",
        "ink-muted": "var(--ui-muted)",
        accent: "var(--ui-accent)",
        "accent-soft": "var(--ui-accent-soft)",
        inverse: "var(--ui-inverse)",
        "on-inverse": "var(--ui-on-inverse)",
        danger: "var(--ui-danger)",
        warning: "var(--ui-warning)",
        "warning-soft": "var(--ui-warning-soft)",
        scrim: "var(--ui-scrim)",
      },
      borderRadius: {
        "ui-sm": "var(--ui-radius-sm)",
        "ui-md": "var(--ui-radius-md)",
        "ui-lg": "var(--ui-radius-lg)",
      },
      boxShadow: {
        "ui-1": "var(--ui-shadow-1)",
        "ui-2": "var(--ui-shadow-2)",
        "ui-3": "var(--ui-shadow-3)",
      },
      fontSize: {
        "ui-meta": ["var(--ui-text-meta)", "16px"],
        "ui-label": ["var(--ui-text-label)", "18px"],
        "ui-body": ["var(--ui-text-body)", "22px"],
        "ui-title": ["var(--ui-text-title)", "24px"],
      },
      keyframes: {
        "ui-pop": {
          from: { opacity: "0", transform: "scale(0.97)" },
          to: { opacity: "1", transform: "scale(1)" },
        },
        "ui-sheet": {
          from: { opacity: "0", transform: "translateX(16px)" },
          to: { opacity: "1", transform: "translateX(0)" },
        },
        "ui-shimmer": {
          from: { backgroundPosition: "200% 0" },
          to: { backgroundPosition: "-200% 0" },
        },
      },
      animation: {
        "ui-pop": "ui-pop 120ms ease-out",
        "ui-sheet": "ui-sheet 160ms ease-out",
        "ui-shimmer": "ui-shimmer 1.6s linear infinite",
      },
      fontFamily: {
        sans: ['"Inter"', "system-ui", "sans-serif"],
        mono: ['"JetBrains Mono"', "monospace"],
      },
    },
  },
  plugins: [],
};

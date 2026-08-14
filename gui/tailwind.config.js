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
      },
      fontFamily: {
        sans: ['"Inter"', "system-ui", "sans-serif"],
        mono: ['"JetBrains Mono"', "monospace"],
      },
    },
  },
  plugins: [],
};

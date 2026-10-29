import { expect, it } from "vitest";

// In the Tauri webview, window.confirm returns a Promise (always truthy) and
// window.prompt returns null on macOS, so guards built on them never ask.
// Use confirmDialog and promptDialog from components/DialogService instead.
const sources = import.meta.glob<string>(
  ["../**/*.{ts,tsx}", "!../**/*.test.{ts,tsx}", "!../**/*.d.ts"],
  { query: "?raw", import: "default", eager: true },
);

const NATIVE_DIALOG = [
  // window.confirm( and window?.prompt(; \s also spans line breaks, so
  // `window\n  .prompt(` is caught too.
  /\b(?:window|globalThis|self)\s*\??\.\s*(?:confirm|prompt)\s*\(/g,
  // window["confirm"], window?.['prompt'] and window[`confirm`].
  /\b(?:window|globalThis|self)\s*(?:\?\.)?\s*\[\s*(["'`])(?:confirm|prompt)\1\s*\]/g,
  // A bare call, as after `const { confirm } = window`. Method calls such as
  // client.confirm(, longer names such as confirmDialog( and a function
  // declared with the same name are not the native dialogs.
  /(?<![\w$]|\.\s*|\bfunction\s+)(?:confirm|prompt)\(/g,
];

const nativeDialogCalls = (source: string) =>
  NATIVE_DIALOG.flatMap((pattern) =>
    [...source.matchAll(pattern)].map((match) => match.index),
  );

it("scans the application sources", () => {
  expect(Object.keys(sources).length).toBeGreaterThan(100);
  expect(Object.keys(sources)).toContain("../components/DialogService.tsx");
});

it("recognizes native dialog calls but not similar names", () => {
  for (const call of [
    'window.confirm("Delete?")',
    'globalThis\n  .prompt("Name")',
    'window?.confirm("Delete?")',
    'confirm("Delete?")',
    "  if (!prompt(message)) return;",
    'window["confirm"]("Delete?")',
    "window['prompt']",
    "self[`confirm`]",
  ])
    expect(nativeDialogCalls(call), call).toHaveLength(1);
  for (const other of [
    'await confirmDialog("Delete?")',
    'await promptDialog("Name")',
    'client.confirm("Delete?")',
    'dialogs\n  .prompt("Name")',
    "function prompt(message: string) {",
    "reconfirm(value)",
    "the survey prompt (works for any input)",
    "request.prompt_chars",
  ])
    expect(nativeDialogCalls(other), other).toEqual([]);
});

it("never calls the native confirm or prompt dialogs", () => {
  const offenders = Object.entries(sources).flatMap(([path, source]) =>
    nativeDialogCalls(source).map(
      (index) =>
        `${path.replace(/^\.\.\//, "src/")}:${source.slice(0, index).split("\n").length}`,
    ),
  );
  expect(offenders).toEqual([]);
});
